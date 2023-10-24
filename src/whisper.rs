use whisper_rs::WhisperContext;
use crate::config::Config;

pub(crate) async fn run_whisper(config: &Config) {
    let ctx = WhisperContext::new(&*config.whisper.model).expect("failed to load whisper context");
    let mut _state = ctx.create_state().expect("failed to create state");
    let params = (&config.whisper).to_full_params();
    _state.full(params, &[]).expect("TODO: panic message");
}

async fn pcm_i16_to_f32(input: &Vec<u8>) -> Vec<f32> {
    let pcm_i16 = input
        .chunks_exact(2)
        .map(|chunk| {
            let mut buf = [0u8; 2];
            buf.copy_from_slice(chunk);
            i16::from_le_bytes(buf)
        })
        .collect::<Vec<i16>>();
    let pcm_f32 = pcm_i16
        .iter()
        .map(|i| *i as f32 / i16::MAX as f32)
        .collect::<Vec<f32>>();
    pcm_f32
}

struct WhisperHandler {

}