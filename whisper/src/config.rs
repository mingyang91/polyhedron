use serde::Deserialize;
use whisper_rs::{FullParams, WhisperToken};

#[derive(Debug, Deserialize, Clone)]
pub struct WhisperConfig {
    pub(crate) params: WhisperParams,
    pub(crate) step_ms: usize,
    pub(crate) length_ms: usize,
    pub(crate) keep_ms: usize,
    pub model: String,
    pub(crate) max_prompt_tokens: usize,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize, Clone)]
pub(crate) struct WhisperParams {
    pub(crate) n_threads: Option<usize>,
    pub(crate) max_tokens: Option<u32>,
    pub(crate) audio_ctx: Option<u32>,
    pub(crate) speed_up: Option<bool>,
    pub(crate) translate: Option<bool>,
    pub(crate) no_context: Option<bool>,
    pub(crate) print_special: Option<bool>,
    pub(crate) print_realtime: Option<bool>,
    pub(crate) print_progress: Option<bool>,
    pub(crate) token_timestamps: Option<bool>,
    pub(crate) no_timestamps: Option<bool>,
    pub(crate) temperature_inc: Option<f32>,
    pub(crate) entropy_threshold: Option<f32>,
    pub(crate) single_segment: Option<bool>,
    pub(crate) suppress_non_speech_tokens: Option<bool>,
    pub(crate) n_max_text_ctx: Option<usize>,
    // pub(crate) tinydiarize: bool,
    pub(crate) language: Option<String>,
}

impl WhisperParams {
    pub(crate) fn to_full_params<'a, 'b>(&'a self, tokens: &'b [WhisperToken]) -> FullParams<'a, 'b> {
        let mut param = FullParams::new(Default::default());
        if let Some(print_progress) = self.print_progress.as_ref() {
            param.set_print_progress(*print_progress);
        }
        if let Some(print_special) = self.print_special.as_ref() {
            param.set_print_special(*print_special);
        }
        if let Some(print_realtime) = self.print_realtime.as_ref() {
            param.set_print_realtime(*print_realtime);
        }
        if let Some(single_segment) = self.single_segment.as_ref() {
            param.set_single_segment(*single_segment);
        }
        if let Some(no_timestamps) = self.no_timestamps.as_ref() {
            param.set_print_timestamps(!no_timestamps);
        }
        if let Some(token_timestamps) = self.token_timestamps.as_ref() {
            param.set_token_timestamps(*token_timestamps);
        }
        if let Some(translate) = self.translate.as_ref() {
            param.set_translate(*translate);
        }
        if let Some(max_tokens) = self.max_tokens.as_ref() {
            param.set_max_tokens(*max_tokens as i32);
        }
        param.set_language(self.language.as_deref());
        if let Some(n_threads) = self.n_threads.as_ref() {
            param.set_n_threads(*n_threads as i32);
        }
        if let Some(audio_ctx) = self.audio_ctx.as_ref() {
            param.set_audio_ctx(*audio_ctx as i32);
        }
        if let Some(speed_up) = self.speed_up.as_ref() {
            param.set_speed_up(*speed_up);
        }
        // param.set_tdrz_enable(self.tinydiarize);
        if let Some(temperature_inc) = self.temperature_inc.as_ref() {
            param.set_temperature_inc(*temperature_inc);
        }
        if let Some(suppress_non_speech_tokens) = self.suppress_non_speech_tokens.as_ref() {
            param.set_suppress_non_speech_tokens(*suppress_non_speech_tokens);
        }
        if let Some(no_context) = self.no_context.as_ref() {
            param.set_no_context(*no_context);
        }
        if let Some(entropy_threshold) = self.entropy_threshold.as_ref() {
            param.set_entropy_thold(*entropy_threshold);
        }
        if let Some(n_max_text_ctx) = self.n_max_text_ctx.as_ref() {
            param.set_n_max_text_ctx(*n_max_text_ctx as i32);
        }

        param.set_tokens(tokens);
        param
    }
}
