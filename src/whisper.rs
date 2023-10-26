use std::collections::VecDeque;
use std::ffi::c_int;
use std::fmt::{Debug, Display, Formatter};
use std::thread::sleep;
use std::time::Duration;
use lazy_static::lazy_static;
use tokio::sync::{broadcast, mpsc, oneshot};
use whisper_rs::{convert_integer_to_float_audio, WhisperState, WhisperContext, FullParams};
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
        let n_samples_step: usize = (config.step_ms * WHISPER_SAMPLE_RATE / 1000) as usize;
        let n_samples_len: usize = (config.length_ms * WHISPER_SAMPLE_RATE / 1000) as usize;
        let n_samples_keep: usize = (config.keep_ms * WHISPER_SAMPLE_RATE / 1000) as usize;
        let n_new_line: usize = 1.max(config.length_ms / config.step_ms - 1) as usize;
        let mut n_iter: usize = 0;

        let (stop_handle, mut stop_signal) = oneshot::channel();
        let (pcm_tx, pcm_rx) = mpsc::channel::<Vec<u8>>(128);
        let mut grouped = GroupedWithin::new(
            n_samples_step * 2,
            Duration::from_millis(config.step_ms as u64),
            pcm_rx,
            u16::MAX as usize
        );
        let (transcription_tx, _) = broadcast::channel::<Vec<Segment>>(128);
        let shared_transcription_tx = transcription_tx.clone();
        let mut state = WHISPER_CONTEXT.create_state()
            .map_err(|e| Error::whisper_error("failed to create WhisperState", e))?;
        tokio::task::spawn_blocking(move || {
            let mut prompt_tokens: Vec<c_int> = Default::default();
            let mut pcm_f32: VecDeque<f32> = VecDeque::from(vec![0f32; 30 * WHISPER_SAMPLE_RATE as usize]);
            let mut last_offset: usize = 0;
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

                let params = config.to_full_params(prompt_tokens.as_slice());
                pcm_f32.extend(new_pcm_f32);
                if pcm_f32.len() > n_samples_len + n_samples_keep {
                    let _ = pcm_f32.drain(0..(pcm_f32.len() - n_samples_keep - n_samples_len)).len();
                }
                pcm_f32.make_contiguous();
                let (data, _) = pcm_f32.as_slices();
                let segments = match inference(&mut state, params, data, 0) {
                    Ok((offset, result)) => {
                        last_offset = offset;
                        if result.is_empty() {
                            continue
                        }
                        result
                    }
                    Err(err) => {
                        tracing::warn!("failed to inference: {}", err);
                        continue
                    }
                };

                n_iter = n_iter + 1;

                if n_iter % n_new_line == 0 {
                    if let Err(e) = new_line(&mut pcm_f32, n_samples_keep, &config, &mut state, segments.len(), &mut prompt_tokens) {
                        tracing::warn!("failed to new_line: {}", e);
                    }
                    tracing::debug!("LINE: {}", segments.first().unwrap().text);

                    if let Err(e) = shared_transcription_tx.send(segments) {
                        tracing::error!("failed to send transcription: {}", e);
                        break
                    };
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
    params: FullParams,
    pcm_f32: &[f32],
    mut offset: usize,
) -> Result<(usize, Vec<Segment>), Error> {

    let _ = state.full(params, pcm_f32)
        .map_err(|e| Error::whisper_error("failed to initialize WhisperState", e))?;

    let num_segments = state
        .full_n_segments()
        .map_err(|e| Error::whisper_error("failed to get number of segments", e))?;
    let mut segments: Vec<Segment> = Vec::with_capacity(num_segments as usize);
    for i in 0..num_segments {
        let segment = state
            .full_get_segment_text(i)
            .map_err(|e| Error::whisper_error("failed to get segment", e))?;
        let timestamp_offset: i64 = (offset * 1000 / WHISPER_SAMPLE_RATE as usize) as i64;
        let start_timestamp: i64 = timestamp_offset + 10 * state
            .full_get_segment_t0(i)
            .map_err(|e| Error::whisper_error("failed to get start timestamp", e))?;
        let end_timestamp: i64 = timestamp_offset + 10 * state
            .full_get_segment_t1(i)
            .map_err(|e| Error::whisper_error("failed to get end timestamp", e))?;
        // tracing::trace!("{}", segment);
        segments.push(Segment { start_timestamp, end_timestamp, text: segment });
    }

    Ok((offset, segments))
}

fn new_line(pcm_f32: &mut VecDeque<f32>,
            n_samples_keep: usize,
            config: &WhisperParams,
            state: &mut WhisperState,
            num_segments: usize,
            prompt_tokens: &mut Vec<c_int>) -> Result<(), Error> {

    // keep the last n_samples_keep samples from pcm_f32
    if pcm_f32.len() > n_samples_keep {
        let _ = pcm_f32.drain(0..(pcm_f32.len() - n_samples_keep)).len();
    }

    if !config.no_context {
        prompt_tokens.clear();

        for i in 0..num_segments as c_int {
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
    Ok(())
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
