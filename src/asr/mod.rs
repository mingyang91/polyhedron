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


pub fn slice_i16_to_u8_le(slice: &[i16]) -> Vec<u8> {
    slice
        .iter()
        .flat_map(|&sample| {
            [sample as u8, (sample >> 8) as u8]
        })
        .collect()
}

pub fn slice_i16_to_u8_be(slice: &[i16]) -> Vec<u8> {
    slice
        .iter()
        .flat_map(|&sample| {
            [(sample >> 8) as u8, sample as u8]
        })
        .collect()
}


