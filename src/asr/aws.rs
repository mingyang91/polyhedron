use async_trait::async_trait;
use tokio::sync::broadcast::Receiver;
use crate::asr::{ASR, Event};

struct AWS_ASR {
    aws: aws_sdk_transcribestreaming::Client,
}
#[async_trait]
impl ASR for AWS_ASR {
    async fn frame(&mut self, frame: &[i16]) -> anyhow::Result<()> {
        todo!()
    }

    fn subscribe(&mut self) -> Receiver<Event> {
        todo!()
    }
}
