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
    //
    // /// The name of the audio file.
    // #[structopt(short, long)]
    // audio_file: String,
    //
    /// Whether to display additional information.
    #[structopt(short, long)]
    verbose: bool,
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
        verbose,
    } = Opt::parse();

    let region_provider = RegionProviderChain::first_try(region.map(Region::new))
        .or_default_provider()
        .or_else(Region::new("us-west-2"));

    println!();

    if verbose {
        println!("Transcribe client version: {}", PKG_VERSION);
        println!(
            "Region:                    {}",
            region_provider.region().await.unwrap().as_ref()
        );
        println!();
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
                    let Ok(txt) = w else {
                        continue
                    };
                    println!("Whisper: {:?}", txt)
                }
                msg = socket.next() => {
                    match msg.as_ref() {
                        Some(Ok(Message::Binary(bin))) => {
                            let _ = whisper.send(bin.clone()).await; // whisper test
                            if origin_tx.send(bin.to_vec()).await.is_err() {
                                println!("tx closed");
                                break;
                            }
                        },
                        Some(Ok(_)) => {
                            println!("Other: {:?}", msg);
                        },
                        Some(Err(e)) => {
                            println!("Error: {:?}", e);
                        },
                        None => {
                            let _ = socket.close().await;
                            println!("Other: {:?}", msg);
                            break;
                        }
                    }
                },
                output = transcribe_rx.recv() => {
                    if let Ok(transcript) = output {
                        println!("Transcribed: {}", transcript);
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
    println!("{:?}", query);
    let voice_id = query.voice.parse().expect("Not supported voice");

    ws.on_upgrade(|mut socket| async move {
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
                        let json = serde_json::to_string(&evt).expect("failed to serialize");
                        println!("Transcribed: {}", json);
                        let _ = socket.send(Message::Text(json)).await;
                    }
                },
                translated = translate_rx.recv() => {
                    if let Ok(translated) = translated {
                        let evt = LiveLessonTextEvent::Translation { text: translated };
                        let json = serde_json::to_string(&evt).expect("failed to serialize");
                        println!("Translated: {}", json);
                        let _ = socket.send(Message::Text(json)).await;
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
