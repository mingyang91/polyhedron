use std::{
    collections::VecDeque,
    fmt::{Debug, Display, Formatter},
    thread::sleep,
    time::Duration,
};
use fvad::SampleRate;

use once_cell::sync::Lazy;
use tokio::sync::{broadcast, mpsc, oneshot};
use tokio::time::Instant;
use tracing::{debug, trace, warn};
use whisper_rs::{convert_integer_to_float_audio, WhisperContext, WhisperState, WhisperToken, WhisperTokenData};

use crate::config::{Settings, SETTINGS};
use crate::{config::WhisperConfig, group::GroupedWithin};

const WHISPER_SAMPLE_RATE: usize = whisper_rs_sys::WHISPER_SAMPLE_RATE as usize;

static WHISPER_CONTEXT: Lazy<WhisperContext> = Lazy::new(|| {
    let settings = Settings::new().expect("Failed to initialize settings.");
    if tracing::enabled!(tracing::Level::DEBUG) {
        let info = whisper_rs::print_system_info();
        debug!("system_info: n_threads = {} / {} | {}\n",
            settings.whisper.params.n_threads.unwrap_or(0),
            std::thread::available_parallelism().map(|c| c.get()).unwrap_or(0),
            info);
    }
    WhisperContext::new(&settings.whisper.model).expect("failed to create WhisperContext")
});


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

fn u8_to_i16(input: &[u8]) -> Vec<i16> {
    input
        .chunks_exact(2)
        .map(|chunk| {
            let mut buf = [0u8; 2];
            buf.copy_from_slice(chunk);
            i16::from_le_bytes(buf)
        })
        .collect::<Vec<i16>>()
}

#[derive(Clone, Debug)]
pub enum Output {
    Unstable(Segment),
    Stable(Segment),
}

#[derive(Clone, Debug)]
pub struct Segment {
    pub start_timestamp: i64,
    pub end_timestamp: i64,
    pub text: String,
    tokens: Vec<WhisperTokenData>,
}

pub struct WhisperHandler {
    tx: mpsc::Sender<Vec<i16>>,
    transcription_tx: broadcast::Sender<Vec<Output>>,
    stop_handle: Option<oneshot::Sender<()>>,
}

impl WhisperHandler {
    pub(crate) fn new(config: WhisperConfig, prompt: String) -> Result<Self, Error> {
        let vad_slice_size = WHISPER_SAMPLE_RATE / 100 * 3;
        let (stop_handle, mut stop_signal) = oneshot::channel();
        let (pcm_tx, pcm_rx) = mpsc::channel::<Vec<i16>>(128);
        let (transcription_tx, _) = broadcast::channel::<Vec<Output>>(128);
        let shared_transcription_tx = transcription_tx.clone();
        let state = WHISPER_CONTEXT
            .create_state()
            .map_err(|e| Error::whisper_error("failed to create WhisperState", e))?;
        let preset_prompt_tokens = WHISPER_CONTEXT
            .tokenize(prompt.as_str(), SETTINGS.whisper.max_prompt_tokens)
            .map_err(|e| Error::whisper_error("failed to tokenize prompt", e))?;
        tokio::task::spawn_blocking(move || {
            let mut vad = fvad::Fvad::new().expect("failed to create VAD")
                .set_sample_rate(SampleRate::Rate16kHz);
            let mut detector = Detector::new(state, &SETTINGS.whisper, preset_prompt_tokens);
            let mut grouped = GroupedWithin::new(
                detector.n_samples_step,
                Duration::from_millis(config.step_ms as u64),
                pcm_rx,
                u16::MAX as usize,
            );
            while let Err(oneshot::error::TryRecvError::Empty) = stop_signal.try_recv() {
                if detector.has_crossed_next_line() {
                    if let Some(segment) = detector.next_line() {
                        let segments = vec![Output::Stable(segment)];
                        if let Err(e) = shared_transcription_tx.send(segments) {
                            tracing::error!("failed to send transcription: {}", e);
                            break;
                        };
                    }
                }
                let new_pcm_f32 = match grouped.next() {
                    Err(mpsc::error::TryRecvError::Disconnected) => break,
                    Err(mpsc::error::TryRecvError::Empty) => {
                        sleep(Duration::from_millis(10));
                        continue;
                    }
                    Ok(data) => {
                        let active_voice = data
                            .chunks(vad_slice_size)
                            .filter(|frame| {
                                if frame.len() != vad_slice_size {
                                    true
                                } else {
                                    vad.is_voice_frame(frame).unwrap_or(true)
                                }
                                // true
                            })
                            .collect::<Vec<_>>()
                            .concat();
                        convert_integer_to_float_audio(&active_voice)
                    },
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
                        warn!("failed to inference: {}", err);
                        continue;
                    }
                };

                let outputs = segments
                    .iter()
                    .map(|segment| Output::Unstable(segment.clone()))
                    .collect::<Vec<_>>();
                if let Err(e) = shared_transcription_tx.send(outputs) {
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

    pub fn subscribe(&self) -> broadcast::Receiver<Vec<Output>> {
        self.transcription_tx.subscribe()
    }

    pub async fn send_i16(&mut self, data: Vec<i16>) -> Result<(), mpsc::error::SendError<Vec<i16>>> {
        self.tx.send(data).await
    }

    pub async fn send_bytes(&mut self, data: Vec<u8>) -> Result<(), mpsc::error::SendError<Vec<i16>>> {
        let i16_data = u8_to_i16(&data);
        self.send_i16(i16_data).await
    }
}

#[allow(dead_code)]
struct Detector {
    state: WhisperState<'static>,
    config: &'static WhisperConfig,
    start_time: Instant,
    segment: Option<Segment>,
    line_num: usize,
    preset_prompt_tokens: Vec<WhisperToken>,
    n_samples_keep: usize,
    n_samples_step: usize,
    n_samples_len: usize,
    prompt_tokens: Vec<WhisperToken>,
    pcm_f32: VecDeque<f32>,
    offset: usize,
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
            start_time: Instant::now(),
            segment: None,
            line_num: 0,
            preset_prompt_tokens,
            n_samples_keep: config.keep_ms * WHISPER_SAMPLE_RATE / 1000,
            n_samples_step: config.step_ms * WHISPER_SAMPLE_RATE / 1000,
            n_samples_len: config.length_ms * WHISPER_SAMPLE_RATE / 1000,
            prompt_tokens: Default::default(),
            pcm_f32: VecDeque::with_capacity(config.length_ms * WHISPER_SAMPLE_RATE / 1000),
            offset: 0,
        }
    }

    fn feed(&mut self, new_pcm_f32: Vec<f32>) {
        self.pcm_f32.extend(new_pcm_f32);
        if self.pcm_f32.len() < self.n_samples_len {
            return;
        }
        // let len_to_drain = self
        //     .pcm_f32
        //     .drain(0..(self.pcm_f32.len() - self.n_samples_len))
        //     .len();
        // warn!("ASR too slow, drain {} samples", len_to_drain);
        // self.offset += len_to_drain;
    }

    fn inference(&mut self) -> Result<Vec<Segment>, Error> {
        let params = self.config.params.to_full_params(self.prompt_tokens.as_slice());
        let start = std::time::Instant::now();
        let _ = self
            .state
            .full(params, self.pcm_f32.make_contiguous())
            .map_err(|e| Error::whisper_error("failed to initialize WhisperState", e))?;
        let end = std::time::Instant::now();
        if end - start > Duration::from_millis(self.config.step_ms as u64) {
            // warn!(
            //     "full([{}]) took {} ms too slow",
            //     self.pcm_f32.len(),
            //     (end - start).as_millis()
            // );
        }

        let timestamp_offset: i64 = (self.offset * 1000 / WHISPER_SAMPLE_RATE) as i64;
        let num_segments = self
            .state
            .full_n_segments()
            .map_err(|e| Error::whisper_error("failed to get number of segments", e))?;
        let mut segments: Vec<Segment> = Vec::with_capacity(num_segments as usize);
        for i in 0..num_segments {
            let start_timestamp: i64 = timestamp_offset
                + 10 * self
                    .state
                    .full_get_segment_t0(i)
                    .map_err(|e| Error::whisper_error("failed to get start timestamp", e))?;

            let end_timestamp: i64 = timestamp_offset
                + 10 * self
                .state
                .full_get_segment_t1(i)
                .map_err(|e| Error::whisper_error("failed to get end timestamp", e))?;

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
                let token_data = self.state.full_get_token_data(i, j)
                    .map_err(|e| Error::whisper_error("failed to get token data", e))?;
                segment_tokens.push(token_data);
            }

            segments.push(Segment {
                start_timestamp,
                end_timestamp,
                text: segment.trim().to_string(),
                tokens: segment_tokens,
            });
        }

        self.segment = segments.first().cloned();
        Ok(segments.to_vec())
    }

    fn remember_prompt(&mut self) {
        let Some(segment) = self.segment.as_ref() else {
            return
        };

        let tokens = segment
            .tokens
            .iter()
            .map(|td| td.tid)
            .collect::<Vec<WhisperToken>>();

        self.prompt_tokens.extend(tokens);
        if self.prompt_tokens.len() > self.config.max_prompt_tokens {
            let _ = self.prompt_tokens.drain(0..(self.prompt_tokens.len() - self.config.max_prompt_tokens)).len();
        }
    }

    fn has_crossed_next_line(&self) -> bool {
        let now = Instant::now();
        let elapsed = now - self.start_time;
        let line_number: usize = (elapsed.as_millis() / self.config.length_ms as u128) as usize;
        line_number > self.line_num
    }

    fn next_line(&mut self) -> Option<Segment> {
        if self.pcm_f32.len() > self.n_samples_keep {
            let drain_size = self.pcm_f32.drain(0..(self.pcm_f32.len() - self.n_samples_keep)).len();
            self.offset += drain_size;
        } else {
            let size_will_clear = self.pcm_f32.len();
            self.pcm_f32.clear();
            self.offset += size_will_clear;
        }

        self.line_num += 1;
        self.remember_prompt();
        self.segment.take()
    }
}

impl Drop for WhisperHandler {
    fn drop(&mut self) {
        let Some(stop_handle) = self.stop_handle.take() else {
            return warn!("WhisperHandler::drop() called without stop_handle");
        };
        if stop_handle.send(()).is_err() {
            warn!("WhisperHandler::drop() failed to send stop signal");
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use std::io::{stdout, Write};
    use hound;
    use tracing_test;
    use tracing::info;

    async fn print_output(output: Output) {
        match output {
            Output::Stable(stable) => {
                print!("\x1b[2K\r");
                print!("{}\n", stable.text);
            },
            Output::Unstable(unstable) => {
                // back to previous line of console
                print!("\x1b[2K\r");
                print!("{}", " ".repeat(100));
                print!("\x1b[2K\r");
                print!("{} ...", unstable.text);
            }
        }
        stdout().flush().unwrap();
    }
    #[tokio::test]
    #[tracing_test::traced_test]
    async fn test_whisper_handler() {
        let mut whisper_handler = WhisperHandler::new(
            SETTINGS.whisper.clone(),
            "Harry Potter and the Philosopher's Stone".to_string(),
        ).expect("failed to create WhisperHandler");

        let wav = hound::WavReader::open("samples/ADHD_1A.wav")
            .expect("failed to open wav");
        let spec = wav.spec();
        println!("{:?}", spec);
        let samples = wav
            .into_samples::<i16>()
            .map(|s| s.unwrap())
            .collect::<Vec<i16>>();
        let chunks = samples.chunks(1600)
            .map(|chunk| chunk.to_vec())
            .into_iter();

        let mut rx = whisper_handler.subscribe();
        let send_fut = async {
            // tokio::time::sleep(Duration::from_secs(5)).await;
            for chunk in chunks {
                let _ = whisper_handler.send_i16(chunk).await.expect("failed to send sample");
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        };

        let recv_fut = async {
            while let Ok(outputs) = rx.recv().await {
                let Some(output) = outputs.first() else {
                    continue
                };

                match output {
                    Output::Stable(stable) => {
                        println!("{}", stable.text);
                    },
                    Output::Unstable(unstable) => {

                    }
                }

            }
        };

        tokio::join!(send_fut, recv_fut);
    }
}