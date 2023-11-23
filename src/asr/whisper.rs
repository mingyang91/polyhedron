use std::fmt::{Debug, Formatter};
use async_trait::async_trait;
use tokio::{spawn};
use tokio::sync::broadcast::Receiver;
use lazy_static::lazy_static;

extern crate whisper;

use whisper::handler::{Error, Output, WhisperHandler, Context};
use crate::asr::{ASR, Event};
use crate::config::SETTINGS;

lazy_static! {
    pub static ref CONTEXT: Context = Context::new(&SETTINGS.whisper.model)
        .expect("Failed to initialize whisper context");
}

pub struct WhisperAsr {
    whisper: WhisperHandler,
    tx: tokio::sync::broadcast::Sender<Event>,
}

impl Debug for WhisperAsr {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "Whisper_ASR")
    }
}

impl WhisperAsr {
    pub async fn from_config() -> Result<WhisperAsr, Error> {
        let whisper = CONTEXT.create_handler(SETTINGS.whisper.clone(), "".to_string());
        let mut output_rx = whisper.subscribe();
        let (tx, _) = tokio::sync::broadcast::channel(64);
        let shared_tx = tx.clone();
        let fut = async move {
            while let Ok(outputs) = output_rx.recv().await {
                for output in outputs {
                    let evt = match output {
                        Output::Stable(segment) => Event {
                            transcript: segment.text,
                            is_final: true,
                        },
                        Output::Unstable(segment) => Event {
                            transcript: segment.text,
                            is_final: false,
                        },
                    };
                    if let Err(e) = tx.send(evt) {
                        tracing::warn!("Failed to send whisper event: {}", e);
                        break
                    }
                }
            }
        };
        spawn(fut);
        Ok(Self { whisper, tx: shared_tx })
    }
}

#[async_trait]
impl ASR for WhisperAsr {
    async fn frame(&mut self, frame: Vec<i16>) -> anyhow::Result<()> {
        Ok(self.whisper.send_i16(frame).await?)
    }

    fn subscribe(&mut self) -> Receiver<Event> {
        self.tx.subscribe()
    }
}