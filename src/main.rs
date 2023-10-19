/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0.
 */

#![allow(clippy::result_large_err)]

use std::default::Default;
use tokio::sync::mpsc::channel;
use async_stream::stream;
use aws_config::meta::region::RegionProviderChain;
use aws_sdk_transcribestreaming::primitives::Blob;
use aws_sdk_transcribestreaming::types::{AudioStream, AudioEvent, LanguageCode, MediaEncoding, TranscriptResultStream};
use aws_sdk_transcribestreaming::{config::Region, meta::PKG_VERSION, Client, Error};
use bytes::BufMut;
use clap::Parser;

use poem::{handler, listener::TcpListener, Server, get, Route, IntoResponse, Endpoint, EndpointExt};
use futures_util::{Sink, SinkExt};
use poem::endpoint::StaticFilesEndpoint;
use poem::web::websocket::{Message, WebSocket};
use futures_util::stream::StreamExt;

use tokio::select;
use tokio::sync::mpsc::{Receiver, Sender};


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

enum ReplyEvent {
    Transcribed(String),
    Translated(String),
    Synthesized(Vec<u8>),
}

const CHUNK_SIZE: usize = 8192;

/// Transcribes an audio file to text.
/// # Arguments
///
/// * `-a AUDIO_FILE` - The name of the audio file.
///   It must be a WAV file, which is converted to __pcm__ format for Amazon Transcribe.
///   Amazon transcribe also supports __ogg-opus__ and __flac__ formats.
/// * `[-r REGION]` - The Region in which the client is created.
///   If not supplied, uses the value of the **AWS_REGION** environment variable.
///   If the environment variable is not set, defaults to **us-west-2**.
/// * `[-v]` - Whether to display additional information.
async fn stream_process(mut rx: Receiver<Vec<u8>>, tx: Sender<ReplyEvent>) -> Result<(), Error> {
    tracing_subscriber::fmt::init();

    let Opt {
        region,
        // audio_file,
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
        // println!("Audio filename:            {}", &audio_file);
        println!();
    }

    let shared_config = aws_config::from_env().region(region_provider).load().await;
    let transcript_client = Client::new(&shared_config);
    let translate_client = aws_sdk_translate::Client::new(&shared_config);
    let polly_client = aws_sdk_polly::Client::new(&shared_config);

    let input_stream = stream! {
        while let Some(raw) = rx.recv().await {
            yield Ok(AudioStream::AudioEvent(AudioEvent::builder().audio_chunk(Blob::new(raw)).build()));
        }
    };

    let mut output = transcript_client
        .start_stream_transcription()
        .language_code(LanguageCode::ZhCn)//LanguageCode::EnGb
        .media_sample_rate_hertz(16000)
        .media_encoding(MediaEncoding::Pcm)
        .audio_stream(input_stream.into())
        .send()
        .await?;

    while let Some(event) = output.transcript_result_stream.recv().await? {
        match event {
            TranscriptResultStream::TranscriptEvent(transcript_event) => {
                let transcript = transcript_event.transcript.expect("transcript");
                for result in transcript.results.unwrap_or_default() {
                    if result.is_partial {
                        if verbose {
                            println!("Partial: {:?}", result);
                        }
                    } else {
                        let first_alternative = &result.alternatives.as_ref().expect("should have")[0];
                        let slice = first_alternative.transcript.as_ref().expect("should have");
                        println!("Line:   {:?}", slice);
                        tx.send(ReplyEvent::Transcribed(slice.clone())).await.expect("failed to send");
                        let lc = result.language_code.as_ref().map(|lc| lc.as_str().to_string());
                        let translated = translate(&translate_client, first_alternative.transcript.clone(), lc).await;
                        if let Some(has) = translated {
                            tx.send(ReplyEvent::Transcribed(has.clone())).await.expect("failed to send");
                            println!("Translated: {}", has);
                            if let Some(synthesized) = synthesize(&polly_client, has).await {
                                tx.send(ReplyEvent::Synthesized(synthesized)).await.expect("failed to send");
                            }
                        }

                    }
                }
            }
            otherwise => panic!("received unexpected event type: {:?}", otherwise),
        }
    }

    Ok(())
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

fn pcm_data(audio_file: &str) -> Vec<u8> {
    let reader = hound::WavReader::open(audio_file).unwrap();
    let samples_result: hound::Result<Vec<i16>> = reader.into_samples::<i16>().collect();

    let mut pcm: Vec<u8> = Vec::new();
    for sample in samples_result.unwrap() {
        pcm.put_i16_le(sample);
    }
    pcm
}

#[tokio::main]
async fn main() -> Result<(), std::io::Error> {    let app = Route::new().nest(
        "/",
        StaticFilesEndpoint::new("./static")
            .show_files_listing()
            .index_file("index.html"),
    ).at("/translate", get(stream_translate));
    let listener = TcpListener::bind("[::]:8080");
    let server = Server::new(listener);

    server.run(app).await
}


#[handler]
async fn stream_translate(ws: WebSocket) -> impl IntoResponse {
    ws.on_upgrade(|mut socket| async move {
        let (origin_tx, origin_rx) = channel::<Vec<u8>>(128);
        let (translate_tx, mut translate_rx) = channel::<ReplyEvent>(128);
        let stream_fut = stream_process(origin_rx, translate_tx);

        let ws_fut = async {
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
                output = translate_rx.recv() => {
                    if let Some(reply) = output {
                        match reply {
                            ReplyEvent::Transcribed(transcript) => {
                                println!("Transcribed: {}", transcript);
                                socket.send(Message::Text(transcript)).await.expect("failed to send");
                            },
                            ReplyEvent::Translated(translated) => {
                                println!("Translated: {}", translated);
                                socket.send(Message::Text(translated)).await.expect("failed to send");
                            },
                            ReplyEvent::Synthesized(raw) => {
                                println!("Synthesized: {:?}", raw.len());
                                socket.send(Message::Binary(raw)).await.expect("failed to send");
                            },
                        }
                    }
                },
            }
            }
        };
        select! {
            _ = stream_fut => {},
            _ = ws_fut => {},
        }
    })
}

