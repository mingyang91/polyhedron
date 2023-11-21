use std::{env, net::IpAddr};

use config::{Config, Environment, File};
use once_cell::sync::Lazy;
use serde::Deserialize;
use tracing::debug;
#[cfg(feature = "whisper")]
use crate::whisper;

pub(crate) static SETTINGS: Lazy<Settings> =
    Lazy::new(|| Settings::new().expect("Failed to initialize settings"));


#[derive(Debug, Deserialize)]
pub(crate) struct Server {
    pub(crate) port: u16,
    pub(crate) host: IpAddr,
}

#[derive(Debug, Deserialize)]
pub struct Settings {
    #[cfg(feature = "whisper")]
    pub(crate) whisper: whisper::config::WhisperConfig,
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
