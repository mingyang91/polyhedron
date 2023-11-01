use serde::Deserialize;
use std::ffi::c_int;
use std::fs;
use std::net::IpAddr;
use lazy_static::lazy_static;
use whisper_rs::FullParams;

#[derive(Debug)]
pub enum Error {
    IoError(std::io::Error),
    ConfigError(serde_yaml::Error),
}

pub(crate) fn load_config() -> Result<Config, Error> {
    let config_str = std::fs::read_to_string("config.yaml").map_err(|e| Error::IoError(e))?;
    let config: Config = serde_yaml::from_str(config_str.as_str())
        .map_err(|e| Error::ConfigError(e))?;
    return Ok(config)
}

lazy_static! {
    pub static ref CONFIG: Config = load_config().expect("failed to load config");
}

#[derive(Debug, Deserialize, Clone)]
pub(crate) struct WhisperParams {
    pub(crate) n_threads: Option<usize>,
    pub(crate) step_ms: u32,
    pub(crate) length_ms: u32,
    pub(crate) keep_ms: u32,
    pub(crate) max_tokens: u32,
    pub(crate) audio_ctx: u32,
    // pub(crate) vad_thold: f32,
    // pub(crate) freq_thold: f32,
    pub(crate) speed_up: bool,
    pub(crate) translate: bool,
    pub(crate) no_fallback: bool,
    pub(crate) print_special: bool,
    pub(crate) no_context: bool,
    pub(crate) no_timestamps: bool,
    // pub(crate) tinydiarize: bool,
    pub(crate) language: Option<String>,
    pub(crate) model: String,
}

const NONE: [c_int; 0] = [];

impl WhisperParams {
    pub(crate) fn to_full_params<'a, 'b>(&'a self, tokens: &'b [c_int]) -> FullParams<'a, 'b> {
        let mut param = FullParams::new(Default::default());
        param.set_print_progress(false);
        param.set_print_special(self.print_special);
        param.set_print_realtime(false);
        param.set_print_timestamps(!self.no_timestamps);
        param.set_translate(self.translate);
        param.set_single_segment(true);
        param.set_max_tokens(self.max_tokens as i32);
        let lang = self.language.as_ref().map(|s| s.as_str());
        param.set_language(lang);
        let num_cpus = std::thread::available_parallelism()
            .map(|c| c.get())
            .unwrap_or(4);
        param.set_n_threads(self.n_threads.unwrap_or(num_cpus) as c_int);
        param.set_audio_ctx(self.audio_ctx as i32);
        param.set_speed_up(self.speed_up);
        // param.set_tdrz_enable(self.tinydiarize);
        if self.no_fallback {
            param.set_temperature_inc(-1.0);
        }
        if self.no_context {
            param.set_tokens(&NONE);
        } else {
            param.set_tokens(&tokens);
        }

        param
    }
}

#[derive(Debug, Deserialize)]
pub(crate) struct Server {
    pub(crate) port: u16,
    pub(crate) host: IpAddr,
}

#[derive(Debug, Deserialize)]
pub struct Config {
    pub(crate) whisper: WhisperParams,
    pub(crate) server: Server,
    pub(crate) postgres_url: String,
}

mod tests {
    #[tokio::test]
    async fn load() {
        let config_str = tokio::fs::read_to_string("config.yaml").await.expect("failed to read config file");
        let params: crate::config::Config = serde_yaml::from_str(config_str.as_str()).expect("failed to parse config file");
        println!("{:?}", params);
    }
}
#[tokio::test]
async fn load() {
    let config_str = fs::read_to_string("config.yaml").expect("failed to read config file");
    let params: Config =
        serde_yaml::from_str(config_str.as_str()).expect("failed to parse config file");
    println!("{:?}", params);
}
