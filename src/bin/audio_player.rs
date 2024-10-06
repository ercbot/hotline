use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Sample, SampleFormat, Stream};
use std::sync::{Arc, Mutex};


const OPENAI_SAMPLE_RATE: u32 = 24000; // The sample rate of the audio data coming from OpenAI

// Resample the input audio to match the output sample rate
fn resample(input: &[f32], input_rate: usize, output_rate: usize) -> Vec<f32> {
    let ratio = output_rate as f32 / input_rate as f32;
    let output_len = (input.len() as f32 * ratio).ceil() as usize;
    let mut output = Vec::with_capacity(output_len);

    for i in 0..output_len {
        let index = i as f32 / ratio;
        let index_floor = index.floor() as usize;
        let index_ceil = index.ceil() as usize;

        if index_ceil >= input.len() {
            output.push(input[input.len() - 1]);
        } else {
            let t = index - index_floor as f32;
            let sample = input[index_floor] * (1.0 - t) + input[index_ceil] * t;
            output.push(sample);
        }
    }

    output
}


pub struct AudioPlayer {
}

impl AudioPlayer {
    // Create a new AudioPlayer instance with the default output device
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        Ok(Self {})
    }

    // Play the audio data using the AudioPlayer instance
    pub fn play_audio(&mut self, audio_data: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
        // Initalize host, get default output device, and default output configuration
        let host = cpal::default_host();
        let device = host.default_output_device().expect("no output device available");
        let config = device.default_output_config()?.into();

        // Convert the audio data to f32 samples
        let audio_data = audio_data.chunks(2)
            .map(|chunk| {
                let sample = i16::from_le_bytes([chunk[0], chunk[1]]);
                f32::from(sample) / i16::MAX as f32
            })
            .collect::<Vec<f32>>();

        // Shared, thread-safe buffer of audio samples and the current playback position
        let shared_audio_state = Arc::new(Mutex::new((audio_data, 0))); // (samples, position)

        {
            let playback_data = shared_audio_state.clone();
            let output_stream = device.build_output_stream(
                &config,
                move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                    let mut playback = playback_data.lock().unwrap();
                    let samples = &playback.0;
                    let position = playback.1;
                    let len = data.len();
    
                    if position + len <= samples.len() {
                        data.copy_from_slice(&samples[position..position + len]);
                        playback.1 += len;
                    } else {
                        // Not enough samples left, fill with remaining samples and silence
                        let remaining = samples.len() - position;
                        if remaining > 0 {
                            data[..remaining].copy_from_slice(&samples[position..]);
                        }
                        data[remaining..].iter_mut().for_each(|s| *s = 0.0);
                        playback.1 = samples.len();
                    }
                },
                |err| eprintln!("An error occurred on the output stream: {}", err),
                None,
            )?;
    
            output_stream.play()?; // Start playback

            // Wait for the audio to finish playing
            std::thread::sleep(std::time::Duration::from_secs_f32(
                shared_audio_state.lock().unwrap().0.len() as f32 / config.sample_rate.0 as f32
            ));
            
        }

        Ok(())
    }

}