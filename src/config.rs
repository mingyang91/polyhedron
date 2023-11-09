use std::{env, ffi::c_int, net::IpAddr};

use config::{Config, Environment, File};
use once_cell::sync::Lazy;
use serde::Deserialize;
use whisper_rs::FullParams;
use tracing::debug;

pub(crate) static SETTINGS: Lazy<Settings> =
    Lazy::new(|| Settings::new().expect("Failed to initialize settings"));

#[derive(Debug, Deserialize, Clone)]
pub(crate) struct WhisperConfig {
    pub(crate) params: WhisperParams,
    pub(crate) step_ms: u32,
    pub(crate) length_ms: u32,
    pub(crate) keep_ms: u32,
    pub(crate) model: String,
    pub(crate) max_prompt_tokens: usize,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize, Clone)]
pub(crate) struct WhisperParams {
    pub(crate) n_threads: Option<usize>,
    pub(crate) max_tokens: u32,
    pub(crate) audio_ctx: u32,
    pub(crate) speed_up: bool,
    pub(crate) translate: bool,
    pub(crate) no_fallback: bool,
    pub(crate) print_special: bool,
    pub(crate) print_realtime: bool,
    pub(crate) print_progress: bool,
    pub(crate) no_timestamps: bool,
    pub(crate) temperature_inc: f32,
    pub(crate) single_segment: bool,
    // pub(crate) tinydiarize: bool,
    pub(crate) language: Option<String>,
}

const _NONE: [c_int; 0] = [];

impl WhisperParams {
    pub(crate) fn to_full_params<'a, 'b>(&'a self, _tokens: &'b [c_int]) -> FullParams<'a, 'b> {
        let mut param = FullParams::new(Default::default());
        param.set_print_progress(self.print_progress);
        param.set_print_special(self.print_special);
        param.set_print_realtime(self.print_realtime);
        param.set_print_timestamps(!self.no_timestamps);
        param.set_translate(self.translate);
        param.set_single_segment(false);
        param.set_max_tokens(self.max_tokens as i32);
        let lang = self.language.as_deref();
        param.set_language(lang);
        let num_cpus = std::thread::available_parallelism()
            .map(|c| c.get())
            .unwrap_or(4);
        param.set_n_threads(self.n_threads.unwrap_or(num_cpus) as c_int);
        param.set_audio_ctx(self.audio_ctx as i32);
        param.set_speed_up(self.speed_up);
        // param.set_tdrz_enable(self.tinydiarize);
        param.set_temperature_inc(self.temperature_inc);

        param
    }
}

#[derive(Debug, Deserialize)]
pub(crate) struct Server {
    pub(crate) port: u16,
    pub(crate) host: IpAddr,
}

#[derive(Debug, Deserialize)]
pub struct Settings {
    pub(crate) whisper: WhisperConfig,
    pub(crate) server: Server,
}

impl Settings {
    pub(crate) fn new() -> Result<Self, anyhow::Error> {
        let run_mode = env::var("APP_RUN_MODE").unwrap_or("dev".into());
        let config = Config::builder()
            .add_source(File::with_name(&format!("config/{run_mode}.yaml")).required(false))
            .add_source(Environment::with_prefix("APP").separator("-"))
            .build()
            .map_err(anyhow::Error::from)?;

        config.try_deserialize::<Self>().map_err(Into::into)
            .map(|settings| {
                debug!("Settings: {settings:?}");
                settings
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_dev_settings_should_success() {
        let settings = Settings::new().unwrap();
        println!("{:?}", settings);
    }
}
