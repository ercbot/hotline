use tokio::sync::mpsc;
use std::io::{self, Write};
use serde_json::Value;

use crate::audio_utils::{base64_decode_audio, initialize_audio_stream, resample_audio, SERVER_SAMPLE_RATE};


pub async fn handle_events(mut event_receiver: mpsc::Receiver<Value>) {
    // Initialize the audio stream
    let (audio_sender, output_sample_rate) = initialize_audio_stream();

    while let Some(event) = event_receiver.recv().await {
        if let Some(event_type) = event.get("type").and_then(Value::as_str) {
            match event_type {
                "conversation.item.create" => {
                    // Handle conversation item creation
                },
                "response.create" => {
                    // Handle response creation
                },
                "response.audio_transcript.delta" => {
                    // Handle audio transcript delta events
                    let transcript = event["delta"].as_str().unwrap();

                    // Print the transcript
                    print!("{}", transcript);
                    io::stdout().flush().unwrap();
                },
                "response.audio.delta" => {
                    // Handle audio delta events
                    let base64_audio_data = event["delta"].as_str().unwrap();

                    // Decode the base64 audio data
                    let samples = base64_decode_audio(base64_audio_data);

                    // Resample the audio data to the output sample rate
                    let resampled_samples = resample_audio(&samples, SERVER_SAMPLE_RATE, output_sample_rate);

                    // Send the resampled samples to the audio thread
                    if let Err(e) = audio_sender.send(resampled_samples) {
                        eprintln!("Failed to send audio samples: {}", e);
                    }
                }
                "error" => {
                    // Handle error events
                    println!("Error event: {:?}", event);
                },
                // Add more event types as needed
                _ => println!("Unhandled event type: {}", event_type),
                // _ => (),
            }
        }
    }
}