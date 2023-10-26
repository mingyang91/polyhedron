/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0.
 */

#![allow(clippy::result_large_err)]

use std::default::Default;
use aws_config::meta::region::RegionProviderChain;
use aws_sdk_transcribestreaming::{config::Region, meta::PKG_VERSION};
use clap::Parser;

use poem::{EndpointExt, get, handler, IntoResponse, listener::TcpListener, Route, Server};
use futures_util::{SinkExt};
use poem::endpoint::{StaticFileEndpoint, StaticFilesEndpoint};
use poem::web::websocket::{Message, WebSocket};
use futures_util::stream::StreamExt;
use poem::web::{Data, Query};

use tokio::{select};
use serde::{Deserialize, Serialize};
use lesson::{LessonsManager};
use crate::config::CONFIG;
use crate::lesson::Viseme;
use crate::whisper::WhisperHandler;

mod lesson;
mod config;
mod whisper;
mod group;


#[derive(Debug, Parser)]
struct Opt {
    /// The AWS Region.
    #[structopt(short, long)]
    region: Option<String>,
}

#[derive(Clone)]
struct Context {
    lessons_manager: LessonsManager,
}


#[tokio::main]
async fn main() -> Result<(), std::io::Error> {
    tracing_subscriber::fmt::init();

    let Opt {
        region,
    } = Opt::parse();

    let region_provider = RegionProviderChain::first_try(region.map(Region::new))
        .or_default_provider()
        .or_else(Region::new("us-west-2"));

    if tracing::enabled!(tracing::Level::DEBUG) {
        tracing::debug!("Transcribe client version: {}", PKG_VERSION);
        tracing::debug!(
            "Region:                    {}",
            region_provider.region().await.unwrap().as_ref()
        );
    }

    let shared_config = aws_config::from_env().region(region_provider).load().await;
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
        .at("lesson-speaker", StaticFileEndpoint::new("./static/index.html"))
        .at("lesson-listener", StaticFileEndpoint::new("./static/index.html"))
        .data(ctx);
    let addr = format!("{}:{}", CONFIG.server.host, CONFIG.server.port);
    let listener = TcpListener::bind(addr);
    let server = Server::new(listener);

    server.run(app).await
}


#[derive(Deserialize, Debug)]
pub struct LessonSpeakerQuery {
    id: u32,
    lang: String,
}

#[handler]
async fn stream_speaker(ctx: Data<&Context>, query: Query<LessonSpeakerQuery>, ws: WebSocket) -> impl IntoResponse {
    let lesson = ctx.lessons_manager.create_lesson(query.id, query.lang.clone().parse().expect("Not supported lang")).await;

    ws.on_upgrade(|mut socket| async move {
        let origin_tx = lesson.voice_channel();
        let mut transcribe_rx = lesson.transcript_channel();
        let whisper = WhisperHandler::new(CONFIG.whisper.clone()).expect("failed to create whisper");
        let mut whisper_transcribe_rx = whisper.subscribe();
        loop {
            select! {
                w = whisper_transcribe_rx.recv() => {
                    let Ok(_txt) = w else {
                        // TODO: handle msg
                        continue
                    };
                }
                msg = socket.next() => {
                    match msg.as_ref() {
                        Some(Ok(Message::Binary(bin))) => {
                            let _ = whisper.send(bin.to_vec()).await; // whisper test
                            if let Err(e) = origin_tx.send(bin.to_vec()).await {
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
                    if let Ok(transcript) = output {
                        tracing::trace!("Transcribed: {}", transcript);
                        let evt = LiveLessonTextEvent::Transcription { text: transcript.clone() };
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
    LipSync{ visemes: Vec<Viseme> },
}

#[handler]
async fn stream_listener(ctx: Data<&Context>, query: Query<LessonListenerQuery>, ws: WebSocket) -> impl IntoResponse {
    let lesson_opt = ctx.lessons_manager.get_lesson(query.id).await;
    tracing::debug!("listener param = {:?}", query);

    ws.on_upgrade(|mut socket| async move {
        let voice_id = match query.voice.parse() {
            Ok(id) => id,
            Err(e) => {
                let _ = socket.send(Message::Text(format!("invalid voice id: {}", e))).await;
                return
            }
        };
        let Some(lesson) = lesson_opt else {
            let _ = socket.send(Message::Text("lesson not found".to_string())).await;
            return
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
                    if let Ok(transcript) = transcript {
                        let evt = LiveLessonTextEvent::Transcription { text: transcript };
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
