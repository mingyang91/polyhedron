pub(crate) mod aws;
#[cfg(feature = "whisper")]
pub(crate) mod whisper;

use async_trait::async_trait;
use tokio::sync::broadcast::Receiver;

#[derive(Debug, Clone)]
pub(crate) struct Event {
    pub(crate) transcript: String,
    pub(crate) is_final: bool,
}

#[async_trait]
pub(crate) trait ASR {
    async fn frame(&mut self, frame: Vec<i16>) -> anyhow::Result<()>;
    fn subscribe(&mut self) -> Receiver<Event>;
}
