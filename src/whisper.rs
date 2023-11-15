use std::{
    collections::VecDeque,
    ffi::c_int,
    fmt::{Debug, Display, Formatter},
    thread::sleep,
    time::Duration,
};

use once_cell::sync::Lazy;
use tokio::sync::{broadcast, mpsc, oneshot};
use tracing::{debug, trace};
use whisper_rs::{convert_integer_to_float_audio, WhisperContext, WhisperState, WhisperToken};
use whisper_rs_sys::WHISPER_SAMPLE_RATE;

use crate::config::{Settings, SETTINGS};
use crate::{config::WhisperConfig, group::GroupedWithin};

static WHISPER_CONTEXT: Lazy<WhisperContext> = Lazy::new(|| {
    let settings = Settings::new().expect("Failed to initialize settings.");
    if tracing::enabled!(tracing::Level::DEBUG) {
        let info = print_system_info();
        debug!("system_info: n_threads = {} / {} | {}\n",
            settings.whisper.params.n_threads.unwrap_or(0),
            std::thread::available_parallelism().map(|c| c.get()).unwrap_or(0),
            info);
    }
    WhisperContext::new(&settings.whisper.model).expect("failed to create WhisperContext")
});

fn print_system_info() -> String {
    unsafe {
        let raw_info = whisper_rs_sys::whisper_print_system_info();
        let info = std::ffi::CStr::from_ptr(raw_info);
        info.to_str().unwrap_or("failed to get system info").to_string()
    }
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

fn pcm_i16_to_f32(input: &[u8]) -> Vec<f32> {
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
    tokens: Vec<c_int>,
}

pub struct WhisperHandler {
    tx: mpsc::Sender<Vec<u8>>,
    transcription_tx: broadcast::Sender<Vec<Segment>>,
    stop_handle: Option<oneshot::Sender<()>>,
}

impl WhisperHandler {
    pub(crate) fn new(config: WhisperConfig, prompt: String) -> Result<Self, Error> {
        let (stop_handle, mut stop_signal) = oneshot::channel();
        let (pcm_tx, pcm_rx) = mpsc::channel::<Vec<u8>>(128);
        let (transcription_tx, _) = broadcast::channel::<Vec<Segment>>(128);
        let shared_transcription_tx = transcription_tx.clone();
        let state = WHISPER_CONTEXT
            .create_state()
            .map_err(|e| Error::whisper_error("failed to create WhisperState", e))?;
        let preset_prompt_tokens = WHISPER_CONTEXT
            .tokenize(prompt.as_str(), SETTINGS.whisper.max_prompt_tokens)
            .map_err(|e| Error::whisper_error("failed to tokenize prompt", e))?;
        tokio::task::spawn_blocking(move || {
            let mut detector = Detector::new(state, &SETTINGS.whisper, preset_prompt_tokens);
            let mut grouped = GroupedWithin::new(
                detector.n_samples_step * 2,
                Duration::from_millis(config.step_ms as u64),
                pcm_rx,
                u16::MAX as usize,
            );
            while let Err(oneshot::error::TryRecvError::Empty) = stop_signal.try_recv() {
                let new_pcm_f32 = match grouped.next() {
                    Err(mpsc::error::TryRecvError::Disconnected) => break,
                    Err(mpsc::error::TryRecvError::Empty) => {
                        sleep(Duration::from_millis(10));
                        continue;
                    }
                    Ok(data) => pcm_i16_to_f32(&data),
                };

                detector.feed(new_pcm_f32);
                let segments = match detector.inference() {
                    Ok(result) => {
                        if result.is_empty() {
                            continue;
                        }
                        result
                    }
                    Err(err) => {
                        tracing::warn!("failed to inference: {}", err);
                        continue;
                    }
                };

                for segment in segments.iter() {
                    trace!(
                        "[{}-{}]s SEGMENT: {}",
                        segment.start_timestamp as f32 / 1000.0,
                        segment.end_timestamp as f32 / 1000.0,
                        segment.text
                    );
                }

                if let Err(e) = shared_transcription_tx.send(segments) {
                    tracing::error!("failed to send transcription: {}", e);
                    break;
                };
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

#[allow(dead_code)]
struct Detector {
    state: WhisperState<'static>,
    config: &'static WhisperConfig,
    preset_prompt_tokens: Vec<WhisperToken>,
    n_samples_keep: usize,
    n_samples_step: usize,
    n_samples_len: usize,
    prompt_tokens: Vec<c_int>,
    pcm_f32: VecDeque<f32>,
    offset: usize,
    stable_offset: usize,
}

impl Detector {
    fn new(
        state: WhisperState<'static>,
        config: &'static WhisperConfig,
        preset_prompt_tokens: Vec<WhisperToken>,
    ) -> Self {
        Detector {
            state,
            config,
            preset_prompt_tokens,
            n_samples_keep: (config.keep_ms * WHISPER_SAMPLE_RATE / 1000) as usize,
            n_samples_step: (config.step_ms * WHISPER_SAMPLE_RATE / 1000) as usize,
            n_samples_len: (config.length_ms * WHISPER_SAMPLE_RATE / 1000) as usize,
            prompt_tokens: Default::default(),
            pcm_f32: VecDeque::from(vec![0f32; 30 * WHISPER_SAMPLE_RATE as usize]),
            offset: 0,
            stable_offset: 0,
        }
    }

    fn feed(&mut self, new_pcm_f32: Vec<f32>) {
        self.pcm_f32.extend(new_pcm_f32);
        if self.pcm_f32.len() < self.n_samples_len {
            return;
        }
        let len_to_drain = self
            .pcm_f32
            .drain(0..(self.pcm_f32.len() - self.n_samples_len))
            .len();
        self.offset += len_to_drain;
    }

    fn inference(&mut self) -> Result<Vec<Segment>, Error> {
        let prompt_tokens = [
            self.preset_prompt_tokens.as_slice(),
            self.prompt_tokens.as_slice(),
        ]
        .concat();
        let params = self.config.params.to_full_params(prompt_tokens.as_slice());
        let start = std::time::Instant::now();
        let _ = self
            .state
            .full(params, self.pcm_f32.make_contiguous())
            .map_err(|e| Error::whisper_error("failed to initialize WhisperState", e))?;
        let end = std::time::Instant::now();
        if end - start > Duration::from_millis(self.config.step_ms as u64) {
            tracing::warn!(
                "full([{}]) took {} ms too slow",
                self.pcm_f32.len(),
                (end - start).as_millis()
            );
        }

        let timestamp_offset: i64 = (self.offset * 1000 / WHISPER_SAMPLE_RATE as usize) as i64;
        let stable_offset: i64 = (self.stable_offset * 1000 / WHISPER_SAMPLE_RATE as usize) as i64;
        let num_segments = self
            .state
            .full_n_segments()
            .map_err(|e| Error::whisper_error("failed to get number of segments", e))?;
        let mut segments: Vec<Segment> = Vec::with_capacity(num_segments as usize);
        for i in 0..num_segments {
            let end_timestamp: i64 = timestamp_offset
                + 10 * self
                    .state
                    .full_get_segment_t1(i)
                    .map_err(|e| Error::whisper_error("failed to get end timestamp", e))?;
            if end_timestamp <= stable_offset {
                continue;
            }

            let start_timestamp: i64 = timestamp_offset
                + 10 * self
                    .state
                    .full_get_segment_t0(i)
                    .map_err(|e| Error::whisper_error("failed to get start timestamp", e))?;
            let segment = self
                .state
                .full_get_segment_text(i)
                .map_err(|e| Error::whisper_error("failed to get segment", e))?;
            let num_tokens = self
                .state
                .full_n_tokens(i)
                .map_err(|e| Error::whisper_error("failed to get segment tokens", e))?;
            let mut segment_tokens = Vec::with_capacity(num_tokens as usize);
            for j in 0..num_tokens {
                segment_tokens.push(
                    self.state
                        .full_get_token_id(i, j)
                        .map_err(|e| Error::whisper_error("failed to get token", e))?,
                );
            }

            segments.push(Segment {
                start_timestamp,
                end_timestamp,
                text: segment.trim().to_string(),
                tokens: segment_tokens,
            });
        }

        let Some((_last, init)) = segments.split_last() else {
            return Ok(Vec::default());
        };

        let Some((last_2_seg, _)) = init.split_last() else {
            return Ok(Vec::default());
        };

        let offset = (last_2_seg.end_timestamp - timestamp_offset) as usize / 1000
            * WHISPER_SAMPLE_RATE as usize;
        self.stable_offset = offset;
        self.drop_stable_by_segments(init);
        Ok(init.into())
    }

    fn drop_stable_by_segments(&mut self, stable_segments: &[Segment]) {
        let Some(last) = stable_segments.last() else {
            return;
        };
        let drop_offset: usize =
            last.end_timestamp as usize / 1000 * WHISPER_SAMPLE_RATE as usize - self.offset;
        if drop_offset > self.pcm_f32.len() {
            return; // Arithmetic overflow
        }
        let len_to_drain = self.pcm_f32.drain(0..drop_offset).len();
        self.offset += len_to_drain;

        for segment in stable_segments.iter() {
            self.prompt_tokens.extend(&segment.tokens);
        }
        if self.prompt_tokens.len() > self.config.max_prompt_tokens {
            let _ = self
                .prompt_tokens
                .drain(0..(self.prompt_tokens.len() - self.config.max_prompt_tokens))
                .len();
        }
    }
}

impl Drop for WhisperHandler {
    fn drop(&mut self) {
        let Some(stop_handle) = self.stop_handle.take() else {
            return tracing::warn!("WhisperHandler::drop() called without stop_handle");
        };
        if stop_handle.send(()).is_err() {
            tracing::warn!("WhisperHandler::drop() failed to send stop signal");
        }
    }
}
