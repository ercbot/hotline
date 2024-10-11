mod client;
mod handle_events;
mod audio_utils;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::{Arc, Mutex};

use client::RealtimeClient;
use audio_utils::convert_audio_to_server;
use tokio::sync::mpsc;


// Example usage of the RealtimeClient
#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    // Connect to the WebSocket server
    let mut client = RealtimeClient::new(None, None);
    client.connect(None).await.unwrap();

    // Get the default audio host
    let host = cpal::default_host();
    // Get default input and output devices, handling errors if they don't exist
    let input_device = host
        .default_input_device()
        .expect("no input device available");

    
    // Get the default input and output configuration and convert it to a StreamConfig
    let input_config = input_device.default_input_config()?.config();
    
    // From the Input device, get sample rate, channel count
    let input_sample_rate = input_config.sample_rate.0;
    let input_channels = input_config.channels;

    // Create a buffer that can store 200ms of audio data
    let local_buffer_size = (input_sample_rate as f32 * input_channels as f32 * 0.4) as usize; // 2 bytes per sample (pcm-16)
    let local_buffer = Arc::new(Mutex::new(Vec::with_capacity(local_buffer_size)));

    // Create a channel for sending audio data
    let (sender, mut receiver) = mpsc::channel::<Vec<f32>>(10);

    // Spawn a task to process and send audio data to the server
    tokio::spawn(async move {
        while let Some(buffer) = receiver.recv().await {
            let base64_audio = convert_audio_to_server(&buffer, input_sample_rate, input_channels);
            if let Err(e) = client.input_audio_buffer_append(&base64_audio).await {
                eprintln!("Failed to send audio data to server: {}", e);
            }
        }
    });

    // Set up the audio input stream
    let stream = input_device.build_input_stream(
        &input_config.into(),
        move |data: &[f32], _: &cpal::InputCallbackInfo| {
            let mut buffer = local_buffer.lock().unwrap();
            buffer.extend_from_slice(data);
            
            if buffer.len() >= local_buffer_size {
                let full_buffer = std::mem::replace(&mut *buffer, Vec::with_capacity(local_buffer_size));
                if sender.blocking_send(full_buffer).is_err() {
                    eprintln!("Failed to send audio data through channel");
                }
            }
        },
        |err| eprintln!("An error occurred on the input stream: {}", err),
        None,
    )?;

    // Start the audio stream
    print!("Starting Recording...");
    stream.play()?;

    // Keep the main task running and manage the stream
    loop {}

}
