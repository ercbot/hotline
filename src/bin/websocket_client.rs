use base64::prelude::*;
use futures_util::{SinkExt, StreamExt};
use serde_json::Value;
use std::collections::VecDeque;
use std::env;
use std::io::{self, Write};
use tokio::sync::mpsc;
use tokio_tungstenite::{
    connect_async, tungstenite::client::IntoClientRequest, tungstenite::protocol::Message,
};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::{Arc, Mutex};

const SERVER_SAMPLE_RATE: u32 = 24000; // The sample rate of the audio data coming from OpenAI

async fn response_create(tx: mpsc::UnboundedSender<Message>) {
    let event = serde_json::json!({
        "type": "response.create"
    });

    if let Err(e) = tx.send(Message::Text(event.to_string())) {
        eprintln!("Error sending message: {}", e);
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Define the URL for the WebSocket connection
    let url: &str = "wss://api.openai.com/v1/realtime?model=gpt-4o-realtime-preview-2024-10-01";

    // Read the OpenAI API key from the environment
    let api_key: String = env::var("OPENAI_API_KEY")
        .expect("OPENAI_API_KEY must be set")
        .to_string();

    // Create a new WebSocket client request from the URL
    let mut request = url.into_client_request()?;
    let headers = request.headers_mut();

    // Add the necessary headers to the request
    headers.insert(
        "Authorization",
        format!("Bearer {}", api_key).parse().unwrap(),
    );
    headers.insert("OpenAI-Beta", "realtime=v1".parse().unwrap());

    // Connect to the WebSocket server
    let (ws_stream, _) = connect_async(request).await?;
    println!("Connected to WebSocket server");

    // Split the WebSocket stream into read and write halves
    let (mut write, mut read) = ws_stream.split();

    // Create an mpsc channel to send events to the write task
    let (tx, mut rx) = mpsc::unbounded_channel();

    // Create a thread-safe, growable buffer for output audio samples
    let audio_buffer = Arc::new(Mutex::new(VecDeque::new()));

    // Clone the buffer for use in the audio playback thread
    let playback_buffer = Arc::clone(&audio_buffer);

    // Initialize audio playback stream
    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .expect("no output device available");
    let config = device.default_output_config().unwrap();
    let output_sample_rate = config.sample_rate().0; // Get the output sample rate
    let channels = config.channels(); // Get the number of channels

    println!("Output Channels: {}", channels);

    let stream = device
        .build_output_stream(
            &config.into(),
            move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                let mut buffer = playback_buffer.lock().unwrap();
                for sample in data.iter_mut() {
                    *sample = buffer.pop_front().unwrap_or(0.0);
                }
            },
            |err| eprintln!("An error occurred on the output stream: {}", err),
            None,
        )
        .unwrap();

    stream.play().unwrap();

    // Spawn a task to handle sending events
    tokio::spawn(async move {
        while let Some(event) = rx.recv().await {
            if let Err(e) = write.send(event).await {
                eprintln!("Error sending event: {}", e);
                break;
            }
        }
    });

    // Spawn a task to handle reading incoming events
    let buffer_for_ws = Arc::clone(&audio_buffer);
    let read_handle = tokio::spawn(async move {
        read.for_each(|event| async {

            match event {
                Ok(Message::Text(text)) => {
                    let json: Value = serde_json::from_str(&text).unwrap();
                    if json["type"] == "response.audio.delta" {
                        
                        let base64_audio_data = json["delta"].as_str().unwrap();
                        let audio_data = BASE64_STANDARD.decode(base64_audio_data).unwrap();

                        // Convert audio data to f32 samples
                        let samples: Vec<f32> = audio_data
                            .chunks_exact(2)
                            .map(|chunk| {
                                let sample = i16::from_le_bytes([chunk[0], chunk[1]]);
                                f32::from(sample) / i16::MAX as f32
                            })
                            .collect();

                        // Basic resampling

                        // I have no idea why I need to multiply by 2.0, perhaps it's because the audio data is stereo?
                        // but I haven't found any documentation confirming that. I just tried it and it worked.
                        let resample_ratio = (output_sample_rate as f32 / SERVER_SAMPLE_RATE as f32) * 2.0; 
                        
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
                                let t = index - index_floor as f32;
                                let sample = samples[index_floor] * (1.0 - t) + samples[index_ceil] * t;
                                resampled_audio.push(sample);
                            }
                        }
                    
                        // Add the new samples to the buffer
                        let mut buffer = buffer_for_ws.lock().unwrap();
                        buffer.extend(resampled_audio);
                    }
                    // Check if the event is of type "response.audio_transcript.delta"
                    else if json["type"] == "response.audio_transcript.delta" {
                        // Extract the transcript from the message
                        let transcript = json["delta"].as_str().unwrap();

                        // Print the transcript
                        print!("{}", transcript);
                        io::stdout().flush().unwrap();
                    } else {
                        // println!("{}", json);
                    }
                }
                Ok(_) => (),
                Err(e) => eprintln!("Error receiving message: {}", e),
            }
        })
        .await;
    });

    // Example of sending a message (you can replace this with actual audio input handling)
    let test_message = serde_json::json!({
        "type": "conversation.item.create",
        "item": {
            "type": "message",
            "role": "user",
            "content": [
                {
                    "type": "input_text",
                    "text": "Make up a poem about the birth of the universe."
                }
            ]
        }
    });

    tx.send(Message::Text(test_message.to_string())).unwrap();
    response_create(tx).await;

    // Wait for the read task to complete
    read_handle.await?;

    Ok(())
}
