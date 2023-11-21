/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0.
 */

#![allow(clippy::result_large_err)]

#[cfg(feature = "whisper")]
extern crate whisper;
use aws_sdk_transcribestreaming::meta::PKG_VERSION;
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
use tracing::debug;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

use crate::{config::*, lesson::*};

mod config;
mod lesson;
mod asr;

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

    let shared_config = aws_config::load_from_env().await;
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
        .at("/ws/lesson-listener", get(stream_listener))
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
    lang: String,
    prompt: Option<String>,
}

#[handler]
async fn stream_speaker(
    ctx: Data<&Context>,
    query: Query<LessonSpeakerQuery>,
    ws: WebSocket,
) -> impl IntoResponse {
    let lesson = ctx
        .lessons_manager
        .create_lesson(
            query.id,
            query.lang.clone().parse().expect("Not supported lang"),
        )
        .await;
    let prompt = query.prompt.clone().unwrap_or_default();

    ws.on_upgrade(|mut socket| async move {
        let mut transcribe_rx = lesson.transcript_channel();
        loop {
            select! {
                msg = socket.next() => {
                    match msg.as_ref() {
                        Some(Ok(Message::Binary(bin))) => {
                            let frame = u8_to_i16(bin);
                            if let Err(e) = lesson.send(frame).await {
                                tracing::warn!("failed to send voice: {}", e);
                                break;
                            }
                        },
                        Some(Ok(_)) => {
                            tracing::warn!("Other: {:?}", msg);
                        },
                        Some(Err(e)) => {
                            tracing::warn!("Error: {:?}", e);
                        },
                        None => {
                            if let Err(e) = socket.close().await {
                                tracing::debug!("Message: {:?}, {}", msg, e);
                            }
                            break;
                        }
                    }
                },
                output = transcribe_rx.recv() => {
                    if let Ok(evt) = output {
                        tracing::trace!("Transcribed: {}", evt.transcript);
                        let evt = LiveLessonTextEvent::Transcription { text: evt.transcript };
                        let json = serde_json::to_string(&evt).expect("failed to serialize");
                        let _ = socket.send(Message::Text(json)).await.expect("failed to send");
                    }
                },
            }
        }
    })
}

#[derive(Deserialize, Debug)]
pub struct LessonListenerQuery {
    id: u32,
    lang: String,
    voice: String,
}

#[derive(Serialize)]
#[serde(tag = "type")]
enum LiveLessonTextEvent {
    Transcription { text: String },
    Translation { text: String },
    LipSync { visemes: Vec<Viseme> },
}

#[handler]
async fn stream_listener(
    ctx: Data<&Context>,
    query: Query<LessonListenerQuery>,
    ws: WebSocket,
) -> impl IntoResponse {
    let lesson_opt = ctx.lessons_manager.get_lesson(query.id).await;
    debug!("listener param = {:?}", query);

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
        let Some(lesson) = lesson_opt else {
            let _ = socket
                .send(Message::Text("lesson not found".to_string()))
                .await;
            return;
        };
        let mut transcript_rx = lesson.transcript_channel();
        let mut lang_lesson = lesson.get_or_init(query.lang.clone()).await;
        let mut translate_rx = lang_lesson.translated_channel();
        let mut voice_lesson = lang_lesson.get_or_init(voice_id).await;
        let mut voice_rx = voice_lesson.voice_channel();
        let mut lip_sync_rx = voice_lesson.lip_sync_channel();

        loop {
            select! {
                transcript = transcript_rx.recv() => {
                    if let Ok(evt) = transcript {
                        let evt = LiveLessonTextEvent::Transcription { text: evt.transcript };
                        match serde_json::to_string(&evt) {
                            Ok(json) => {
                                tracing::debug!("Transcribed: {}", json);
                                let _ = socket.send(Message::Text(json)).await;
                            },
                            Err(e) => {
                                tracing::error!("failed to serialize: {}", e);
                            }
                        }
                    }
                },
                translated = translate_rx.recv() => {
                    if let Ok(translated) = translated {
                        let evt = LiveLessonTextEvent::Translation { text: translated };
                        match serde_json::to_string(&evt) {
                            Ok(json) => {
                                tracing::debug!("Translated: {}", json);
                                let _ = socket.send(Message::Text(json)).await;
                            },
                            Err(e) => {
                                tracing::error!("failed to serialize: {}", e);
                            }
                        }
                    }
                },
                voice = voice_rx.recv() => {
                    if let Ok(voice) = voice {
                        let _ = socket.send(Message::Binary(voice)).await;
                    }
                },
                visemes = lip_sync_rx.recv() => {
                    if let Ok(visemes) = visemes {
                        let evt = LiveLessonTextEvent::LipSync { visemes };
                        let json = serde_json::to_string(&evt).expect("failed to serialize");
                        let _ = socket.send(Message::Text(json)).await;
                    }
                },
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