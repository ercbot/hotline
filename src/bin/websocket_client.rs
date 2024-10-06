use std::env;
use futures_util::{StreamExt, SinkExt};
use tokio::sync::mpsc;
use tokio_tungstenite::{connect_async, tungstenite::client::IntoClientRequest, tungstenite::protocol::Message};
use serde_json::Value;
use base64::prelude::*;

// mod audio_player;
// use audio_player::AudioPlayer;


async fn response_create(tx : mpsc::UnboundedSender<Message>) {
    let event = serde_json::json!({
        "type": "response.create"
    });
    
    if let Err(e) = tx.send(Message::Text(event.to_string())) {
        eprintln!("Error sending message: {}", e);
    }
}



#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create a new AudioPlayer instance
    // let mut audio_player = AudioPlayer::new()?;

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
    headers.insert("Authorization", format!("Bearer {}", api_key).parse().unwrap());
    headers.insert("OpenAI-Beta", "realtime=v1".parse().unwrap());
    
    // Connect to the WebSocket server
    let (ws_stream, _) = connect_async(request).await?;
    println!("Connected to WebSocket server");

    // Split the WebSocket stream into read and write halves
    let (mut write, mut read) = ws_stream.split();

    // Create an mpsc channel to send events to the write task
    let (tx, mut rx) = mpsc::unbounded_channel();

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
    let read_handle = tokio::spawn(async move {
        read.for_each(|event| async {
            match event {
                Ok(Message::Text(text)) => {
                    // Parse the incoming message as JSON
                    let json: Value = serde_json::from_str(&text).unwrap();

                    // Check if the event is of type "response.audio.delta"
                    if json["type"] == "response.audio.delta" {
                        // Extract the audio data from the message
                        let audio_data = json["delta"].as_str().unwrap();

                        // Decode the base64-encoded audio data
                        let decoded_audio = BASE64_STANDARD.decode(audio_data).unwrap();

                        // Play the audio
                        // audio_player.play_audio(&decoded_audio).unwrap();
                        // println!("Playing audio...");
                    }
                    // Check if the event is of type "response.audio_transcript.delta"
                    else if json["type"] == "response.audio_transcript.delta" {
                        // Extract the transcript from the message
                        let transcript = json["delta"].as_str().unwrap();

                        // Print the transcript
                        print!("{}", transcript);
                    }
                    else {
                        // println!("{}", json);
                    }

                }
                Ok(_) => (),
                Err(e) => eprintln!("Error receiving message: {}", e),
            }
        }).await;
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
                    "text": "Hello OpenAI"
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