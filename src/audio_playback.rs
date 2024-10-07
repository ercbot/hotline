use cpal::traits::{DeviceTrait, HostTrait};
use std::sync::{Arc, Mutex};
use std::collections::VecDeque;

pub fn initialize_audio_playback(audio_buffer: Arc<Mutex<VecDeque<f32>>>) -> Result<(cpal::Stream, u32), Box<dyn std::error::Error>> {
    // Initialize audio playback stream
    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .expect("no output device available");
    let config = device.default_output_config().unwrap();
    let output_sample_rate = config.sample_rate().0; // Get the output sample rate

    let stream = device
        .build_output_stream(
            &config.into(),
            move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                let mut buffer = audio_buffer.lock().unwrap();
                for sample in data.iter_mut() {
                    *sample = buffer.pop_front().unwrap_or(0.0);
                }
            },
            |err| eprintln!("An error occurred on the output stream: {}", err),
            None,
        )
        .unwrap();

    // Return the stream and output sample rate
    Ok((stream, output_sample_rate))
}