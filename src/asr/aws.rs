use std::error::Error;
use std::fmt::{Debug, Display, Formatter};
use async_stream::stream;
use async_trait::async_trait;
use aws_config::BehaviorVersion;
use aws_sdk_transcribestreaming::operation::start_stream_transcription::StartStreamTranscriptionOutput;
use aws_sdk_transcribestreaming::primitives::Blob;
use aws_sdk_transcribestreaming::types::{
    AudioEvent, AudioStream, LanguageCode, MediaEncoding, TranscriptResultStream,
};
use tokio::select;
use tokio::sync::broadcast::Receiver;
use tokio_stream::Stream;
use futures_util::TryStreamExt;
use tracing::{warn};
use crate::asr::{ASR, Event, slice_i16_to_u8_le};

pub struct AwsAsr {
    speaker_voice_channel: tokio::sync::mpsc::Sender<Vec<i16>>,
    speaker_transcript: tokio::sync::broadcast::Sender<Event>,
    drop_handler: Option<tokio::sync::oneshot::Sender<()>>,
}

impl Debug for AwsAsr {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "AWS_ASR")
    }
}

impl AwsAsr {
    pub async fn from_env(lang: LanguageCode) -> anyhow::Result<Self> {
        let config = aws_config::load_defaults(BehaviorVersion::latest()).await;
        let transcript_client = aws_sdk_transcribestreaming::Client::new(&config);

        let (speaker_voice_channel, mut speaker_voice_rx) = tokio::sync::mpsc::channel::<Vec<i16>>(128);
        let (speaker_transcript, _) = tokio::sync::broadcast::channel::<Event>(128);
        let shared_speaker_transcript = speaker_transcript.clone();

        let (drop_handler, drop_rx) = tokio::sync::oneshot::channel::<()>();

        tokio::spawn(async move {
            let fut = async {
                let input_stream = stream! {
                    while let Some(raw) = speaker_voice_rx.recv().await {
                        let reshape = slice_i16_to_u8_le(&raw);
                        yield Ok(AudioStream::AudioEvent(AudioEvent::builder().audio_chunk(Blob::new(reshape)).build()));
                    }
                };
                let output = transcript_client
                    .start_stream_transcription()
                    .language_code(lang) //LanguageCode::EnGb
                    .media_sample_rate_hertz(16000)
                    .media_encoding(MediaEncoding::Pcm)
                    .audio_stream(input_stream.into())
                    .send()
                    .await
                    .map_err(|e| StreamTranscriptionError::EstablishStreamError(Box::new(e)))?;

                let output_stream = to_stream(output);
                output_stream
                    .try_for_each(|text| async {
                        let _ = shared_speaker_transcript.send(text);
                        Ok(())
                    })
                    .await?;
                Ok(()) as anyhow::Result<()>
            };
            select! {
                res = fut => {
                    if let Err(e) = res {
                        warn!("Error: {:?}", e);
                    }
                }
                _ = drop_rx => {}
            }
        });

        Ok(Self {
            speaker_voice_channel,
            speaker_transcript,
            drop_handler: Some(drop_handler)
        })
    }
}

impl Drop for AwsAsr {
    fn drop(&mut self) {
        if let Some(drop_handler) = self.drop_handler.take() {
            let _ = drop_handler.send(());
        }
    }
}


#[async_trait]
impl ASR for AwsAsr {
    async fn frame(&mut self, frame: Vec<i16>) -> anyhow::Result<()> {
        Ok(self.speaker_voice_channel.send(frame).await?)
    }

    fn subscribe(&mut self) -> Receiver<Event> {
        self.speaker_transcript.subscribe()
    }
}

#[allow(dead_code)]
fn to_stream(
    mut output: StartStreamTranscriptionOutput,
) -> impl Stream<Item = Result<Event, StreamTranscriptionError>> {
    stream! {
        while let Some(event) = output
            .transcript_result_stream
            .recv()
            .await
            .map_err(|e| StreamTranscriptionError::TranscriptResultStreamError(Box::new(e)))? {
            match event {
                TranscriptResultStream::TranscriptEvent(transcript_event) => {
                    let Some(transcript) = transcript_event.transcript else {
                        continue
                    };

                    for result in transcript.results.unwrap_or_default() {
                        let Some(alternatives) = result.alternatives else {
                            continue
                        };
                        let Some(first_alternative) = alternatives.first() else {
                            continue
                        };
                        let Some(text) = &first_alternative.transcript else {
                            continue
                        };
                        let evt = Event {
                            transcript: text.clone(),
                            is_final: !result.is_partial,
                        };
                        yield Ok(evt);
                    }
                }
                _ => yield Err(StreamTranscriptionError::Unknown),
            }
        }
    }
}


#[derive(Debug)]
enum StreamTranscriptionError {
    EstablishStreamError(Box<dyn Error + Send + Sync>),
    TranscriptResultStreamError(Box<dyn Error + Send + Sync>),
    Unknown,
}

impl Display for StreamTranscriptionError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            StreamTranscriptionError::EstablishStreamError(e) => {
                write!(f, "EstablishStreamError: {}", e)
            }
            StreamTranscriptionError::TranscriptResultStreamError(e) => {
                write!(f, "TranscriptResultStreamError: {}", e)
            }
            StreamTranscriptionError::Unknown => write!(f, "Unknown"),
        }
    }
}

impl Error for StreamTranscriptionError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            StreamTranscriptionError::EstablishStreamError(e) => Some(e.as_ref()),
            StreamTranscriptionError::TranscriptResultStreamError(e) => Some(e.as_ref()),
            StreamTranscriptionError::Unknown => None,
        }
    }
}
