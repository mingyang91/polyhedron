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

use tokio::{fs, select};
use serde::{Deserialize, Serialize};
use whisper_rs::WhisperContext;
use lesson::{LessonsManager};
use crate::config::Config;
use crate::lesson::Viseme;
use crate::whisper::run_whisper;

mod lesson;
mod config;
mod whisper;


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

#[derive(Debug)]
enum Error {
    IoError(std::io::Error),
    ConfigError(serde_yaml::Error),
}

async fn load_config() -> Result<Config, Error> {
    let config_str = fs::read_to_string("config.yaml").await.map_err(|e| Error::IoError(e))?;
    let config: Config = serde_yaml::from_str(config_str.as_str())
        .map_err(|e| Error::ConfigError(e))?;
    return Ok(config)
}

#[tokio::main]
async fn main() -> Result<(), std::io::Error> {
    tracing_subscriber::fmt::init();

    let config = load_config().await.expect("failed to load config");
    run_whisper(&config).await;

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
    let listener = TcpListener::bind("[::]:8080");
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
        loop {
            select! {
                msg = socket.next() => {
                    match msg.as_ref() {
                        Some(Ok(Message::Binary(bin))) => {
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
                        println!("Synthesized: {:?}", voice.len());
                        let _ = socket.send(Message::Binary(voice)).await;
                    }
                },
                visemes = lip_sync_rx.recv() => {
                    if let Ok(visemes) = visemes {
                        let evt = LiveLessonTextEvent::LipSync { visemes };
                        let json = serde_json::to_string(&evt).expect("failed to serialize");
                        println!("Visemes: {:?}", json);
                        let _ = socket.send(Message::Text(json)).await;
                    }
                },
            }
        }
    })
}
