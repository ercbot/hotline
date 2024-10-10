use base64::prelude::*;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::mpsc;
use crossbeam_channel::unbounded;
use std::thread;

use ringbuf::{traits::{Consumer, Observer, Producer, Split}, HeapRb};

const SERVER_SAMPLE_RATE: u32 = 24000; // The sample rate coming from/going to the server
const SERVER_CHANNELS: u16 = 1; // The number of channels coming from/going to the server

// Ring buffer needs to be large as API generates audio way faster than it can be played
// TODO: create a ringbuffer for the audio before resampling as sample rate of the server is likely lower than that of the output device
const RING_BUFFER_CAPACITY: usize = 2_400_000;

/// Initializes the audio stream and returns the audio sender and output sample rate.
///
/// This function sets up the audio device, configures the output stream, and starts a separate
/// thread to handle audio playback. It returns a sender for audio samples and the output sample rate.
pub fn initialize_playback_stream() -> (mpsc::Sender<Vec<f32>>, u32, u16) {
    // Initialize audio components
    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .expect("No output device available");
    let config = device.default_output_config().unwrap();
    let output_sample_rate = config.sample_rate().0;
    let output_channels = config.channels();

    // Create a standard channel for audio samples
    let (playback_tx, playback_rx) = mpsc::channel::<Vec<f32>>();

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

        // Playback stream - continously pop samples from the ring buffer to play them
        let playback_stream = device
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

        playback_stream.play().unwrap();

        // Continuously receive audio samples and push them into the ring buffer
        while let Ok(samples) = playback_rx.recv() {
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
    (playback_tx, output_sample_rate, output_channels)
}


const CHUNK_SIZE: usize = 2_400_000; // Adjust as needed

pub fn initialize_recording_stream() -> (crossbeam_channel::Receiver<Vec<f32>>, u32) {
    // Initialize audio components
    let host = cpal::default_host();
    let device = host
        .default_input_device()
        .expect("No output device available");
    let config = device.default_input_config().unwrap();
    let input_sample_rate = config.sample_rate().0;

    // Create a crossbeam channel for audio samples
    let (recording_tx, recording_rx) = unbounded();

    // Clone the device and config to move into the audio thread
    let device_clone = device.clone();

    // Start the audio recording thread
    thread::spawn(move || {
        // Use the cloned device and config to build the input stream
        let device = device_clone;

        // Create the ring buffer
        let audio_buffer = HeapRb::<f32>::new(RING_BUFFER_CAPACITY);
        let (mut producer, mut consumer) = audio_buffer.split();

        // Recording stream - continuously push samples into the ring buffer
        let recording_stream = device
            .build_input_stream(
                &config.into(),
                move |data: &[f32], _: &cpal::InputCallbackInfo| {
                    for &sample in data.iter() {
                        if producer.try_push(sample).is_err() {
                            eprintln!("Warning: Audio Input buffer is full, dropping sample.");
                        }
                    }
                },
                |err| eprintln!("An error occurred on the input stream: {}", err),
                None,
            )
            .unwrap();

        recording_stream.play().unwrap();

        // Continuously read from the ring buffer and send to the main thread
        let mut buffer = Vec::with_capacity(CHUNK_SIZE);
        loop {
            buffer.clear();
            for _ in 0..CHUNK_SIZE {
                if let Some(sample) = consumer.try_pop() {
                    buffer.push(sample);
                } else {
                    break;
                }
            }
            if !buffer.is_empty() {
                if recording_tx.send(buffer.clone()).is_err() {
                    eprintln!("Main thread receiver disconnected. Stopping recording.");
                    break;
                }
            }
            thread::sleep(std::time::Duration::from_millis(10)); // Adjust sleep time as needed
        }
    });

    // Return the receiver and input sample rate
    (recording_rx, input_sample_rate)
}



/// Handling User Input -> Server
/// Function to convert f32 audio samples to i16 PCM in base64 format
fn base64_encode_audio(samples: &[f32]) -> String {
    let audio_data: Vec<u8> = samples
        .iter()
        .map(|sample| (sample * i16::MAX as f32) as i16)
        .flat_map(|sample| sample.to_le_bytes().to_vec())
        .collect();

    BASE64_STANDARD.encode(&audio_data)
}

/// Handling Server -> User Output
/// Function to decode base64 audio data to f32 samples
fn base64_decode_audio(base64_audio_data: &str) -> Vec<f32> {
    let audio_data = BASE64_STANDARD.decode(base64_audio_data).unwrap();

    audio_data
        .chunks_exact(2)
        .map(|chunk| {
            let sample = i16::from_le_bytes([chunk[0], chunk[1]]);
            f32::from(sample) / i16::MAX as f32
        })
        .collect()
}


/// Basic resample and channel conversion 
/// 
/// Resamples audio data from one sample rate and number of channels.
/// cpal uses interleaved samples by default, so stereo is actually one big channel [L, R, L, R, ...].
fn resample_and_convert_channels(
    samples: &[f32],
    current_sample_rate: u32,
    current_num_channels: u16,
    target_sample_rate: u32,
    target_num_channels: u16
) -> Result<Vec<f32>, &'static str> {
    // Validate input
    if current_num_channels != 1 && current_num_channels != 2 {
        return Err("Input must be mono or stereo");
    }
    if target_num_channels != 1 && target_num_channels != 2 {
        return Err("Output must be mono or stereo");
    }
    if current_sample_rate == 0 || target_sample_rate == 0 {
        return Err("Sample rates must be greater than zero");
    }

    // Calculate the resample ratio
    let resample_ratio = target_sample_rate as f32 / current_sample_rate as f32;
    
    // Calculate the output length (before channel conversion)
    let resampled_length = (samples.len() as f32 * resample_ratio) as usize;
    
    // Perform resampling
    let mut resampled_audio = Vec::with_capacity(resampled_length);
    for i in 0..resampled_length {
        let index = i as f32 / resample_ratio;
        let index_floor = index.floor() as usize;
        let index_ceil = (index_floor + 1).min(samples.len() - 1);
        
        // Perform linear interpolation between the floor and ceiling samples
        let t = index.fract(); // weight for interpolation
        let sample = samples[index_floor] * (1.0 - t) + samples[index_ceil] * t;
        resampled_audio.push(sample);
    }

    // Perform channel conversion if necessary
    let converted_audio = match (current_num_channels, target_num_channels) {
        (1, 2) => {
            // Mono to stereo: duplicate each sample
            resampled_audio.iter().flat_map(|&s| vec![s, s]).collect()
        },
        (2, 1) => {
            // Stereo to mono: average each pair of samples
            resampled_audio.chunks(2).map(|chunk| chunk.iter().sum::<f32>() / 2.0).collect()
        },
        _ => resampled_audio, // No conversion needed (mono to mono or stereo to stereo)
    };

    Ok(converted_audio)
}


pub fn convert_audio_to_server(samples: &[f32], sample_rate: u32, channels: u16) -> String {
    // Resample and convert channels to the server format
    let samples = resample_and_convert_channels(samples, sample_rate, channels, SERVER_SAMPLE_RATE, SERVER_CHANNELS).unwrap()

    // Encode the audio data in base64 format
    base64_encode_audio(&samples)
}

pub fn convert_audio_from_server(base64_audio_data: &str, sample_rate: u32, channels: u16) -> Vec<f32> {
    // Decode the base64 audio data
    let samples = base64_decode_audio(base64_audio_data);

    // Resample and convert channels from the server format
    resample_and_convert_channels(&samples, SERVER_SAMPLE_RATE, SERVER_CHANNELS, sample_rate, channels).unwrap()
}