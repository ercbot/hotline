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


pub async fn handle_server_event(event: Result<Message, tokio_tungstenite::tungstenite::Error>, buffer_for_ws: &Arc<Mutex<VecDeque<f32>>>, output_sample_rate: u32) {
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
}