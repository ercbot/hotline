mod client;
mod handle_events;
// mod audio_playback;

use client::RealtimeClient;


// Example usage of the RealtimeClient
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Connect to the WebSocket server
    let mut client = RealtimeClient::new(None, None);
    client.connect(None).await?;

    // Send a user message
    client.send_user_message_content(serde_json::json!({"text": "Hello, AI!"})).await?;


    // Keep the main function alive to simulate continuous interaction
    loop {
        tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;
    }
}
