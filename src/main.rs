/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0.
 */

#![allow(clippy::result_large_err)]

use std::default::Default;
use std::error::Error;
use std::fmt::{Debug, Display, Formatter};
use std::future::Future;
use tokio::sync::mpsc::channel;
use async_stream::stream;
use aws_config::meta::region::RegionProviderChain;
use aws_sdk_transcribestreaming::primitives::Blob;
use aws_sdk_transcribestreaming::types::{AudioEvent, AudioStream, LanguageCode, MediaEncoding, TranscriptResultStream};
use aws_sdk_transcribestreaming::{config::Region, meta::PKG_VERSION};
use aws_sdk_transcribestreaming::operation::start_stream_transcription::StartStreamTranscriptionOutput;
use clap::Parser;

use poem::{Endpoint, EndpointExt, get, handler, IntoResponse, listener::TcpListener, Route, Server};
use futures_util::{Sink, SinkExt, TryFutureExt, TryStreamExt};
use poem::endpoint::{StaticFileEndpoint, StaticFilesEndpoint};
use poem::web::websocket::{Message, WebSocket};
use futures_util::stream::StreamExt;
use poem::web::{Data, Query};

use tokio::select;
use tokio::sync::mpsc::{Receiver, Sender};
use tokio_stream::Stream;
use serde::Deserialize;
use lesson::{LessonsManager};

mod lesson;


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
enum ReplyEvent {
    Transcribed(String),
    Translated(String),
    Synthesized(Vec<u8>),
}


async fn translate(client: &aws_sdk_translate::Client, transcript: Option<String>, source_lang_code: Option<String>) -> Option<String> {
    let res = client.translate_text()
        .set_text(transcript)
        .set_source_language_code(Some("zh".to_string()))
        .set_target_language_code(Some("en".to_string()))
        .send().await;
    res.expect("failed to translate").translated_text
}

async fn synthesize(client: &aws_sdk_polly::Client, transcript: String) -> Option<Vec<u8>> {
    let res = client.synthesize_speech()
        .set_text(Some(transcript))
        .voice_id("Amy".into())
        .output_format("pcm".into())
        .language_code("en-US".into())
        // .language_code("cmn-CN".into())
        .send().await;
    let bs = res.expect("failed to translate").audio_stream.collect().await.ok()?;
    Some(bs.to_vec())
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
                            origin_tx.send(bin.to_vec()).await.expect("failed to send");
                        },
                        Some(Ok(_)) => {
                            println!("Other: {:?}", msg);
                        },
                        Some(Err(e)) => {
                            println!("Error: {:?}", e);
                        },
                        None => {
                            socket.close().await.expect("failed to close");
                            println!("Other: {:?}", msg);
                            break;
                        }
                    }
                },
                output = transcribe_rx.recv() => {
                    if let Ok(transcript) = output {
                        println!("Transcribed: {}", transcript);
                        socket.send(Message::Text(transcript)).await.expect("failed to send");
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

        println!("lesson found");
        let mut transcript_rx = lesson.transcript_channel();
        println!("transcribe start");

        let mut lang_lesson = lesson.get_or_init(query.lang.clone()).await;
        let mut translate_rx = lang_lesson.translated_channel();
        println!("translate start");

        let mut voice_lesson = lang_lesson.get_or_init(voice_id).await;
        let mut voice_rx = voice_lesson.voice_channel();
        println!("synthesize start");

        loop {
            select! {
                transcript = transcript_rx.recv() => {
                    if let Ok(transcript) = transcript {
                        println!("Transcribed: {}", transcript);
                        let _ = socket.send(Message::Text(transcript)).await;
                    }
                },
                translated = translate_rx.recv() => {
                    if let Ok(translated) = translated {
                        println!("Translated: {}", translated);
                        let _ = socket.send(Message::Text(translated)).await;
                    }
                },
                voice = voice_rx.recv() => {
                    if let Ok(voice) = voice {
                        println!("Synthesized: {:?}", voice.len());
                        let _ = socket.send(Message::Binary(voice)).await;
                    }
                },
            }
        }
    })
}

#[derive(Debug)]
enum StreamTranscriptionError {
    EstablishStreamError(Box<dyn Error + Send + Sync>),
    TranscriptResultStreamError(Box<dyn Error + Send + Sync>),
    Shutdown,
    Unknown
}


impl Display for StreamTranscriptionError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            StreamTranscriptionError::EstablishStreamError(e) => write!(f, "EstablishStreamError: {}", e),
            StreamTranscriptionError::TranscriptResultStreamError(e) => write!(f, "TranscriptResultStreamError: {}", e),
            StreamTranscriptionError::Shutdown => write!(f, "Shutdown"),
            StreamTranscriptionError::Unknown => write!(f, "Unknown"),
        }
    }
}

impl Error for StreamTranscriptionError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            StreamTranscriptionError::EstablishStreamError(e) => Some(e.as_ref()),
            StreamTranscriptionError::TranscriptResultStreamError(e) => Some(e.as_ref()),
            StreamTranscriptionError::Shutdown => None,
            StreamTranscriptionError::Unknown => None,
        }
    }
}

fn process(translate_client: aws_sdk_translate::Client,
               polly_client: aws_sdk_polly::Client,
               res: Result<String, StreamTranscriptionError>) -> impl Stream<Item=Result<ReplyEvent, StreamTranscriptionError>> {
    stream! {
        match res {
            Ok(transcription) => {
                yield Ok(ReplyEvent::Transcribed(transcription.clone()));
                let translated = translate(&translate_client, Some(transcription), Some("en".to_string())).await;
                if let Some(has) = translated {
                    yield Ok(ReplyEvent::Translated(has.clone()));
                    println!("Translated: {}", has);
                    if let Some(synthesized) = synthesize(&polly_client, has).await {
                        yield Ok(ReplyEvent::Synthesized(synthesized));
                    }
                }
            },
            Err(e) => {
                yield Err(e);
            }
        }

    }
}
