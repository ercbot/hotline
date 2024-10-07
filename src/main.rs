use base64::prelude::*;
use cpal::traits::StreamTrait;
use futures_util::{SinkExt, StreamExt};
use serde_json::Value;
use std::collections::VecDeque;
use std::env;
use std::io::{self, Write};
use tokio::sync::mpsc;
use tokio_tungstenite::{
    connect_async, tungstenite::client::IntoClientRequest, tungstenite::protocol::Message,
};

use std::sync::{Arc, Mutex};

mod handle_server_event;
use handle_server_event::handle_server_event;

mod audio_playback;
use audio_playback::initialize_audio_playback;

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

    // Initialize audio playback stream
    let (stream, output_sample_rate) = initialize_audio_playback(Arc::clone(&audio_buffer))?;

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
            handle_server_event(event, &buffer_for_ws, output_sample_rate).await;
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
