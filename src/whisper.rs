use std::ffi::c_int;
use std::fmt::{Debug, Display, Formatter};
use std::thread::sleep;
use std::time::Duration;
use lazy_static::lazy_static;
use tokio::sync::{broadcast, mpsc, oneshot};
use whisper_rs::{convert_integer_to_float_audio, WhisperContext, WhisperState};
use whisper_rs_sys::WHISPER_SAMPLE_RATE;
use crate::config::{WhisperParams, CONFIG};
use crate::group::GroupedWithin;

lazy_static! {
    static ref WHISPER_CONTEXT: WhisperContext = {
        WhisperContext::new(&*CONFIG.whisper.model)
            .expect("failed to create WhisperContext")
    };
}

#[derive(Debug)]
pub(crate) enum Error {
    WhisperError {
        description: String,
        error: whisper_rs::WhisperError,
    },
}

impl Error {
    fn whisper_error(description: &str, error: whisper_rs::WhisperError) -> Self {
        Self::WhisperError {
            description: description.to_string(),
            error,
        }
    }
}

impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::WhisperError { description, error } => {
                write!(f, "WhisperError: {}: {}", description, error)
            }
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::WhisperError { error, .. } => Some(error),
        }
    }
}

fn pcm_i16_to_f32(input: &Vec<u8>) -> Vec<f32> {
    let pcm_i16 = input
        .chunks_exact(2)
        .map(|chunk| {
            let mut buf = [0u8; 2];
            buf.copy_from_slice(chunk);
            i16::from_le_bytes(buf)
        })
        .collect::<Vec<i16>>();
    convert_integer_to_float_audio(pcm_i16.as_slice())
}

#[derive(Clone, Debug)]
pub struct Segment {
    pub start_timestamp: i64,
    pub end_timestamp: i64,
    pub text: String,
}

pub struct WhisperHandler {
    tx: mpsc::Sender<Vec<u8>>,
    transcription_tx: broadcast::Sender<Vec<Segment>>,
    stop_handle: Option<oneshot::Sender<()>>,
}

impl WhisperHandler {
    pub(crate) fn new(config: WhisperParams) -> Result<Self, Error> {
        let n_samples_keep: usize = (config.keep_ms * WHISPER_SAMPLE_RATE / 1000) as usize;
        let (stop_handle, mut stop_signal) = oneshot::channel();
        let (pcm_tx, pcm_rx) = mpsc::channel::<Vec<u8>>(128);
        let mut grouped = GroupedWithin::new(
            n_samples_keep,
            Duration::from_secs(5),
            pcm_rx,
            1024
        );
        let (transcription_tx, _) = broadcast::channel::<Vec<Segment>>(128);
        let shared_transcription_tx = transcription_tx.clone();
        let mut state = WHISPER_CONTEXT.create_state()
            .map_err(|e| Error::whisper_error("failed to create WhisperState", e))?;
        tokio::task::spawn_blocking(move || {
            let mut tokens: Vec<c_int> = Default::default();
            let mut pcm_f32: Vec<f32> = Default::default();
            while let Err(oneshot::error::TryRecvError::Empty) = stop_signal.try_recv() {
                let new_pcm_f32 = match grouped.next() {
                    Err(mpsc::error::TryRecvError::Disconnected) => break,
                    Err(mpsc::error::TryRecvError::Empty) => {
                        sleep(Duration::from_millis(10));
                        continue
                    }
                    Ok(data) => {
                        pcm_i16_to_f32(&data)
                    }
                };

                pcm_f32.extend(new_pcm_f32);
                match inference(&mut state, &config, n_samples_keep, &mut tokens, &mut pcm_f32) {
                    Ok(segments) => {
                        if segments.is_empty() {
                            continue
                        }
                        if let Err(e) = shared_transcription_tx.send(segments) {
                            tracing::error!("failed to send transcription: {}", e);
                            break
                        }
                    }
                    Err(err) => {
                        tracing::error!("failed to run whisper: {}", err);
                        continue
                        // break
                    }
                }
            }
        });
        Ok(Self {
            tx: pcm_tx,
            transcription_tx,
            stop_handle: Some(stop_handle),
        })
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Vec<Segment>> {
        self.transcription_tx.subscribe()
    }

    pub async fn send(&self, data: Vec<u8>) -> Result<(), mpsc::error::SendError<Vec<u8>>> {
        self.tx.send(data).await
    }
}

fn inference(
    state: &mut WhisperState,
    config: &WhisperParams,
    n_samples_keep: usize,
    prompt_tokens: &mut Vec<c_int>,
    pcm_f32: &mut Vec<f32>
) -> Result<Vec<Segment>, Error> {
    let params = config.to_full_params(prompt_tokens.as_slice());

    let st = std::time::Instant::now();
    let _ = state.full(params, pcm_f32.as_slice())
        .map_err(|e| Error::whisper_error("failed to initialize WhisperState", e))?;
    let et = std::time::Instant::now();

    let num_segments = state
        .full_n_segments()
        .map_err(|e| Error::whisper_error("failed to get number of segments", e))?;
    let mut segments: Vec<Segment> = Vec::with_capacity(num_segments as usize);
    for i in 0..num_segments {
        let segment = state
            .full_get_segment_text(i)
            .map_err(|e| Error::whisper_error("failed to get segment", e))?;
        let start_timestamp = state
            .full_get_segment_t0(i)
            .map_err(|e| Error::whisper_error("failed to get start timestamp", e))?;
        let end_timestamp = state
            .full_get_segment_t1(i)
            .map_err(|e| Error::whisper_error("failed to get end timestamp", e))?;
        tracing::debug!("[{} - {}]: {}", start_timestamp, end_timestamp, segment);
        segments.push(Segment { start_timestamp, end_timestamp, text: segment });
    }

    if !config.no_context {
        prompt_tokens.clear();

        // keep the last n_samples_keep samples from pcm_f32
        if pcm_f32.len() > n_samples_keep {
            let _ = pcm_f32.drain(0..(pcm_f32.len() - n_samples_keep)).collect::<Vec<_>>();
        }

        let n_segments = state
            .full_n_segments()
            .map_err(|e| Error::whisper_error("failed to get number of segments", e))?;
        for i in 0..n_segments {
            let token_count = state
                .full_n_tokens(i)
                .map_err(|e| Error::whisper_error("failed to get number of tokens", e))?;
            for j in 0..token_count {
                let token = state
                    .full_get_token_id(i, j)
                    .map_err(|e| Error::whisper_error("failed to get token", e))?;
                prompt_tokens.push(token);
            }
        }
    }

    tracing::trace!("took {}ms", (et - st).as_millis());
    Ok(segments)
}

impl Drop for WhisperHandler {
    fn drop(&mut self) {
        let Some(stop_handle) = self.stop_handle.take() else {
            return tracing::warn!("WhisperHandler::drop() called without stop_handle");
        };
        if let Err(_) = stop_handle.send(()) {
            tracing::warn!("WhisperHandler::drop() failed to send stop signal");
        }
    }
}
