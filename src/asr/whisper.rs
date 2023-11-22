use std::fmt::{Debug, Formatter};
use async_trait::async_trait;
use tokio::{select, spawn};
use tokio::sync::broadcast::Receiver;
use tokio::sync::broadcast::error::RecvError;
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
        let whisper = CONTEXT.create_handler(&SETTINGS.whisper, "".to_string())?;
        let mut output_rx = whisper.subscribe();
        let (tx, _) = tokio::sync::broadcast::channel(64);
        let shared_tx = tx.clone();
        let fut = async move {
            loop {
                select! {
                    poll = output_rx.recv() => {
                        match poll {
                            Ok(outputs) => {
                                for output in outputs {
                                    let res = match output {
                                        Output::Stable(segment) => tx.send(Event {
                                            transcript: segment.text,
                                            is_final: true,
                                        }),
                                        Output::Unstable(segment) => tx.send(Event {
                                            transcript: segment.text,
                                            is_final: false,
                                        }),
                                    };
                                    if let Err(e) = res {
                                        tracing::warn!("Failed to send whisper event: {}", e);
                                        break
                                    }
                                }
                            },
                            Err(RecvError::Closed) => break,
                            Err(RecvError::Lagged(lagged)) => {
                                tracing::warn!("Whisper ASR output lagged: {}", lagged);
                            }
                        }
                    },
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