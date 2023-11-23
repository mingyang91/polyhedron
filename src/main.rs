/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0.
 */

#![allow(clippy::result_large_err)]

#[cfg(feature = "whisper")]
extern crate whisper;

use aws_config::BehaviorVersion;
use aws_sdk_transcribestreaming::meta::PKG_VERSION;
use aws_sdk_transcribestreaming::types::LanguageCode;
use futures_util::{stream::StreamExt, SinkExt};
use poem::{
    endpoint::{StaticFileEndpoint, StaticFilesEndpoint},
    get, handler,
    listener::TcpListener,
    web::{
        websocket::{Message, WebSocket},
        Data, Query,
    },
    EndpointExt, IntoResponse, Route, Server,
};
use serde::{Deserialize, Serialize};
use tokio::select;
use tracing::{debug, span};
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

use crate::{config::*, lesson::*};
use crate::base64box::Base64Box;

mod config;
mod lesson;
mod asr;
mod base64box;

#[derive(Clone)]
struct Context {
    lessons_manager: LessonsManager,
}

#[tokio::main]
async fn main() -> Result<(), std::io::Error> {
    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(EnvFilter::from_default_env())
        .init();

    debug!("Transcribe client version: {}", PKG_VERSION);

    let shared_config = aws_config::load_defaults(BehaviorVersion::latest()).await;
    let ctx = Context {
        lessons_manager: LessonsManager::new(&shared_config),
    };

    let app = Route::new()
        .nest(
            "/",
            StaticFilesEndpoint::new("./static")
                .show_files_listing()
                .index_file("index.html"),
        )
        .at("/ws/lesson-speaker", get(stream_speaker))
        .at("/ws/teacher", get(stream_speaker))
        .at("/ws/lesson-listener", get(stream_listener))
        .at("/ws/student", get(stream_listener))
        .at("/ws/voice", get(stream_single))
        .at(
            "lesson-speaker",
            StaticFileEndpoint::new("./static/index.html"),
        )
        .at(
            "lesson-listener",
            StaticFileEndpoint::new("./static/index.html"),
        )
        .data(ctx);
    let addr = format!("{}:{}", SETTINGS.server.host, SETTINGS.server.port);
    let listener = TcpListener::bind(addr);
    let server = Server::new(listener);

    select! {
        res = server.run(app) => res,
        _ = tokio::signal::ctrl_c() => {
            tracing::info!("Shutting down");
            Ok(())
        },
    }
}

#[derive(Deserialize, Debug)]
pub struct LessonSpeakerQuery {
    id: u32,
    language: String,
    #[allow(dead_code)] // TODO: use this in the future
    prompt: Option<String>,
}

#[handler]
async fn stream_speaker(
    ctx: Data<&Context>,
    query: Query<LessonSpeakerQuery>,
    ws: WebSocket,
) -> impl IntoResponse {
    let lessons_manager = ctx.lessons_manager.clone();
    ws.on_upgrade(|mut socket| async move {
        let Ok(lang) = query.language.parse::<LanguageCode>() else {
            let _ = socket
                .send(Message::Text(format!("invalid language code: {}", query.language)))
                .await;
            return
        };
        let lesson = lessons_manager
            .create_lesson(
                query.id,
                AsrEngine::AWS,
                lang,
            )
            .await;

        let mut transcribe_rx = lesson.transcript_channel();
        let fut = async {
            loop {
                select! {
                    msg = socket.next() => {
                        let Some(res) = msg else { break };
                        let msg = res?;
                        if msg.is_close() {
                            break
                        }
                        let Message::Binary(bin) = msg else {
                            tracing::warn!("Other: {:?}", msg);
                            continue
                        };
                        let frame = u8_to_i16(&bin);
                        lesson.send(frame).await?
                    },
                    output = transcribe_rx.recv() => {
                        let evt = output?;
                        if evt.is_final {
                            tracing::trace!("Transcribed: {}", evt.transcript);
                        }
                        let evt = LiveLessonTextEvent::Transcription { content: evt.transcript, is_final: evt.is_final };
                        let Ok(json) = serde_json::to_string(&evt) else {
                            tracing::warn!("failed to serialize json: {:?}", evt);
                            continue
                        };
                        socket.send(Message::Text(json)).await?
                    },
                }
            }
            Ok(())
        };

        let span = span!(tracing::Level::TRACE, "lesson_speaker", lesson_id = query.id);
        let _ = span.enter();
        let res: anyhow::Result<()> = fut.await;
        match res {
            Ok(()) => {
                tracing::info!("lesson speaker closed");
            }
            Err(e) => {
                tracing::warn!("lesson speaker error: {}", e);
            }
        }
    })
}

#[derive(Deserialize, Debug)]
pub struct LessonListenerQuery {
    id: u32,
    language: String,
    voice: String,
}

#[derive(Serialize, Debug)]
#[serde(tag = "type")]
enum LiveLessonTextEvent {
    #[serde(rename = "original")]
    Transcription {
        content: String,
        #[serde(rename = "isFinal")]
        is_final: bool
    },
    Translation { content: String },
    LipSync { visemes: Vec<Viseme> },
}
#[handler]
async fn stream_listener(
    ctx: Data<&Context>,
    query: Query<LessonListenerQuery>,
    ws: WebSocket,
) -> impl IntoResponse {
    let lessons_manager = ctx.lessons_manager.clone();

    ws.on_upgrade(|mut socket| async move {
        let voice_id = match query.voice.parse() {
            Ok(id) => id,
            Err(e) => {
                let _ = socket
                    .send(Message::Text(format!("invalid voice id: {}", e)))
                    .await;
                return;
            }
        };

        let lesson_opt = lessons_manager.get_lesson(query.id).await;
        debug!("listener param = {:?}", query);
        let Some(lesson) = lesson_opt else {
            let _ = socket
                .send(Message::Text("lesson not found".to_string()))
                .await;
            return;
        };
        let mut transcript_rx = lesson.transcript_channel();
        let mut lang_lesson = lesson.get_or_init(query.language.clone()).await;
        let mut translate_rx = lang_lesson.translated_channel();
        let mut voice_lesson = lang_lesson.get_or_init(voice_id).await;
        let mut voice_rx = voice_lesson.voice_channel();
        let mut lip_sync_rx = voice_lesson.lip_sync_channel();

        let fut = async {
            loop {
                select! {
                    transcript_poll = transcript_rx.recv() => {
                        let transcript = transcript_poll?;
                        let evt = LiveLessonTextEvent::Transcription {
                            content: transcript.transcript,
                            is_final: transcript.is_final
                        };
                        let Ok(json) = serde_json::to_string(&evt) else {
                            tracing::warn!("failed to serialize: {:?}", evt);
                            continue
                        };
                        tracing::debug!("Transcribed: {}", json);
                        socket.send(Message::Text(json)).await?
                    },
                    translated_poll = translate_rx.recv() => {
                        let translated = translated_poll?;
                        let evt = LiveLessonTextEvent::Translation { content: translated };
                        let Ok(json) = serde_json::to_string(&evt) else {
                            tracing::warn!("failed to serialize: {:?}", evt);
                            continue
                        };
                        tracing::debug!("Translated: {}", json);
                        socket.send(Message::Text(json)).await?
                    },
                    voice_poll = voice_rx.recv() => {
                        let voice = voice_poll?;
                        socket.send(Message::Binary(voice)).await?
                    },
                    visemes_poll = lip_sync_rx.recv() => {
                        let visemes = visemes_poll?;
                        let evt = LiveLessonTextEvent::LipSync { visemes };
                        let Ok(json) = serde_json::to_string(&evt) else {
                            tracing::warn!("failed to serialize: {:?}", evt);
                            continue
                        };
                        socket.send(Message::Text(json)).await?
                    },
                }
            }
        };

        let span = span!(tracing::Level::TRACE, "lesson_listener", lesson_id = query.id);
        let _ = span.enter();
        let res: anyhow::Result<()> = fut.await;
        match res {
            Ok(()) => {
                tracing::info!("lesson listener closed");
            }
            Err(e) => {
                tracing::warn!("lesson listener error: {}", e);
            }
        }
    })
}

#[derive(Serialize, Debug)]
enum SingleEvent {
    #[serde(rename = "original")]
    Transcription {
        content: String,
        #[serde(rename = "isFinal")]
        is_final: bool
    },
    #[serde(rename = "translated")]
    Translation { content: String },
    #[serde(rename = "voice")]
    Voice {
        content: Base64Box
    },
}


#[derive(Deserialize, Debug)]
pub struct SingleQuery {
    id: u32,
    from: String,
    to: String,
    voice: Option<String>,
}


#[handler]
async fn stream_single(
    ctx: Data<&Context>,
    query: Query<SingleQuery>,
    ws: WebSocket
) -> impl IntoResponse {
    let lessons_manager = ctx.lessons_manager.clone();
    ws.on_upgrade(|mut socket| async move {
        let Ok(lang) = query.from.parse::<LanguageCode>() else {
            let _ = socket
                .send(Message::Text(format!("invalid language code: {}", query.from)))
                .await;
            return
        };
        let lesson = lessons_manager
            .create_lesson(
                query.id,
                AsrEngine::AWS,
                lang,
            )
            .await;

        let mut transcribe_rx = lesson.transcript_channel();
        let mut lang_lesson = lesson.get_or_init(query.to.clone()).await;
        let mut translate_rx = lang_lesson.translated_channel();
        let Ok(voice_id) = query.voice.as_deref().unwrap_or("Amy").parse() else {
            let _ = socket
                .send(Message::Text(format!("invalid voice id: {:?}", query.voice)))
                .await;
          return
        };
        let mut voice_lesson = lang_lesson.get_or_init(voice_id).await;
        let mut voice_rx = voice_lesson.voice_channel();
        // let mut lip_sync_rx = voice_lesson.lip_sync_channel();

        let fut = async {
            loop {
                let evt = select! {
                    input = socket.next() => {
                        let Some(res) = input else { break };
                        let msg = res?;
                        if msg.is_close() {
                            break
                        }
                        let Message::Binary(bin) = msg else {
                            tracing::warn!("Other: {:?}", msg);
                            continue
                        };
                        let frame = u8_to_i16(&bin);
                        lesson.send(frame).await?;
                        continue
                    },
                    transcript_poll = transcribe_rx.recv() => {
                        let evt = transcript_poll?;
                        if evt.is_final {
                            tracing::trace!("Transcribed: {}", evt.transcript);
                        }
                        SingleEvent::Transcription { content: evt.transcript, is_final: evt.is_final }
                    },
                    translated_poll = translate_rx.recv() => {
                        let translated = translated_poll?;
                        SingleEvent::Translation { content: translated }
                    },
                    voice_poll = voice_rx.recv() => {
                        let voice = voice_poll?;
                        SingleEvent::Voice { content: Base64Box(voice) }
                    },
                };

                let Ok(json) = serde_json::to_string(&evt) else {
                    tracing::warn!("failed to serialize json: {:?}", evt);
                    continue
                };
                socket.send(Message::Text(json)).await?
            }
            Ok(())
        };

        let span = span!(tracing::Level::TRACE, "lesson_speaker", lesson_id = query.id);
        let _ = span.enter();
        let res: anyhow::Result<()> = fut.await;
        match res {
            Ok(()) => {
                tracing::info!("lesson speaker closed");
            }
            Err(e) => {
                tracing::warn!("lesson speaker error: {}", e);
            }
        }
    })
}

fn u8_to_i16(input: &[u8]) -> Vec<i16> {
    input
        .chunks_exact(2)
        .map(|chunk| {
            let mut buf = [0u8; 2];
            buf.copy_from_slice(chunk);
            i16::from_le_bytes(buf)
        })
        .collect::<Vec<i16>>()
}