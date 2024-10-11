use tokio::sync::mpsc;
use serde_json::Value;

use crate::audio_utils::{convert_audio_from_server, initialize_playback_stream, PlaybackCommand};


pub async fn handle_events(mut event_receiver: mpsc::Receiver<Value>) {
    // Initialize the audio stream
    let (audio_sender, output_sample_rate, output_channels) = initialize_playback_stream();

    while let Some(event) = event_receiver.recv().await {
        if let Some(event_type) = event.get("type").and_then(Value::as_str) {
            println!("{}", event_type);
            match event_type {
                "conversation.item.create" => {
                    // Handle conversation item creation
                },
                "response.create" => {
                    // Handle response creation
                },
                "response.audio_transcript.delta" => {
                    // // Handle audio transcript delta events
                    // let transcript = event["delta"].as_str().unwrap();

                    // // Print the transcript
                    // print!("{}", transcript);
                    // io::stdout().flush().unwrap();
                },
                "response.audio.delta" => {
                    // Handle audio delta events
                    let base64_audio_data = event["delta"].as_str().unwrap();

                    // Decode and resample the audio data to the output sample rate
                    let samples = convert_audio_from_server(base64_audio_data, output_sample_rate, output_channels);

                    // Send the resampled samples to the audio thread
                    if let Err(e) = audio_sender.send(PlaybackCommand::Play(samples)) {
                        eprintln!("Failed to send audio samples: {}", e);
                    }
                },
                "response.audio.done" => {
                    // Handle audio complete events
                    

                },
                "input_audio_buffer.speech_started" => {
                    // Handle speech started events
                    audio_sender.send(PlaybackCommand::Stop).unwrap();
                }

                "error" => {
                    // Handle error events
                    println!("Error event: {:?}", event);
                },
                "input_audio_buffer.append" => {
                    // Handle input audio buffer append events
                },
                "session.created" => {
                    // Handle session created events
                },
                // Add more event types as needed
                // _ => println!("Unhandled event type: {}", event_type),
                _ => (),
            }
        }
    }
}