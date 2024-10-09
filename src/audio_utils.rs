use base64::prelude::*;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::mpsc;
use std::thread;

use ringbuf::{traits::{Consumer, Observer, Producer, Split}, HeapRb};

pub const SERVER_SAMPLE_RATE: u32 = 24000; // The sample rate of the audio data coming from OpenAI
const RING_BUFFER_CAPACITY: usize = 240_000; // 10 seconds of audio at 24,000 Hz

/// Initializes the audio stream and returns the audio sender and output sample rate.
///
/// This function sets up the audio device, configures the output stream, and starts a separate
/// thread to handle audio playback. It returns a sender for audio samples and the output sample rate.
pub fn initialize_audio_stream() -> (mpsc::Sender<Vec<f32>>, u32) {
    // Initialize audio components
    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .expect("No output device available");
    let config = device.default_output_config().unwrap();
    let output_sample_rate = config.sample_rate().0;

    // Create a standard channel for audio samples
    let (audio_sender, audio_receiver) = mpsc::channel::<Vec<f32>>();

    // Clone the device and config to move into the audio thread
    let device_clone = device.clone();
    let config_clone = config.clone();

    // Start the audio playback thread (synchronous)
    thread::spawn(move || {
        // Use the cloned device and config to build the output stream
        let device = device_clone;
        let config = config_clone;

        // Create the ring buffer
        let audio_buffer = HeapRb::<f32>::new(RING_BUFFER_CAPACITY);
        let (mut producer, mut consumer) = audio_buffer.split();

        let stream = device
            .build_output_stream(
                &config.into(),
                move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                    for sample in data.iter_mut() {
                        *sample = consumer.try_pop().unwrap_or(0.0);
                    }
                },
                |err| eprintln!("An error occurred on the output stream: {}", err),
                None,
            )
            .unwrap();

        stream.play().unwrap();

        // Continuously receive audio samples and push them into the ring buffer
        while let Ok(samples) = audio_receiver.recv() {
            for sample in samples {
                // Handle buffer full situation
                if producer.is_full() {
                    eprintln!("Warning: Audio buffer is full, dropping sample.");
                } else {
                    producer.try_push(sample).unwrap();
                }
            }
        }
    });

    // Return the sender and output sample rate
    (audio_sender, output_sample_rate)
}

// Handling User Input -> Server
// Function to convert f32 audio samples to i16 PCM in base64 format
pub fn base64_encode_audio(samples: &[f32]) -> String {
    let audio_data: Vec<u8> = samples
        .iter()
        .map(|sample| (sample * i16::MAX as f32) as i16)
        .flat_map(|sample| sample.to_le_bytes().to_vec())
        .collect();

    BASE64_STANDARD.encode(&audio_data)
}

// Handling Server -> User Output
// Function to decode base64 audio data to f32 samples
pub fn base64_decode_audio(base64_audio_data: &str) -> Vec<f32> {
    let audio_data = BASE64_STANDARD.decode(base64_audio_data).unwrap();

    audio_data
        .chunks_exact(2)
        .map(|chunk| {
            let sample = i16::from_le_bytes([chunk[0], chunk[1]]);
            f32::from(sample) / i16::MAX as f32
        })
        .collect()
}


// Basic resampling function, may replace this with rubato (https://crates.io/crates/rubato)
pub fn resample_audio(samples: &[f32], current_sample_rate: u32, target_sample_rate: u32) -> Vec<f32> {
    // I have no idea why I need to multiply by 2.0, perhaps it's because the audio data is stereo?
    // but I haven't found any documentation confirming that. I just tried it and it worked.
    // The other possiblity is there is an error in the resampling or encoding/decoding
    let resample_ratio = (target_sample_rate as f32 / current_sample_rate as f32) * 2.0;
   
    let output_length = (samples.len() as f32 * resample_ratio) as usize;
    let mut resampled_audio = Vec::with_capacity(output_length);
    
    // Loop through the output length to generate resampled audio
    for i in 0..output_length {
        // Calculate the corresponding index in the input samples
        let index = i as f32 / resample_ratio;
        let index_floor = index.floor() as usize;
        let index_ceil = index.ceil() as usize;
        
        // If the ceiling index is out of bounds, use the last sample
        if index_ceil >= samples.len() {
            resampled_audio.push(samples[samples.len() - 1]);
        } else {
            // Perform linear interpolation between the floor and ceiling samples
            let t = index - index_floor as f32; // weight for interpolation
            let sample = samples[index_floor] * (1.0 - t) + samples[index_ceil] * t;
            resampled_audio.push(sample);
        }
    }
    
    resampled_audio
}
