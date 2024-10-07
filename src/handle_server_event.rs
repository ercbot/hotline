use base64::prelude::*;
use serde_json::Value;
use std::collections::VecDeque;
use std::io::{self, Write};

use tokio_tungstenite::tungstenite::protocol::Message;

use std::sync::{Arc, Mutex};


use crate::audio_utils::{base64_decode_audio, resample_audio, SERVER_SAMPLE_RATE};


pub async fn handle_server_event(event: Result<Message, tokio_tungstenite::tungstenite::Error>, buffer_for_ws: &Arc<Mutex<VecDeque<f32>>>, output_sample_rate: u32) {
    match event {
        Ok(Message::Text(text)) => {
            let json: Value = serde_json::from_str(&text).unwrap();
            if json["type"] == "response.audio.delta" {
                
                let base64_audio_data = json["delta"].as_str().unwrap();
                
                // Decode the base64 audio data
                let samples = base64_decode_audio(base64_audio_data);

                // Basic resampling
                let resampled_audio = resample_audio(&samples, SERVER_SAMPLE_RATE, output_sample_rate);
            
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
}