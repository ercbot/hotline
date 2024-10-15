mod client;
mod handle_events;
mod audio_utils;
mod config;

use client::RealtimeClient;
use audio_utils::{convert_audio_to_server, initialize_recording_stream};



// Example usage of the RealtimeClient
#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    // Connect to the WebSocket server
    let mut client = RealtimeClient::new(None, None);

    // Load the configuration
    let session_config = config::get_session_config()?;

    // Update the client with the session configuration
    client.session_config = session_config;

    client.connect(None).await.unwrap();

    // Initialize the recording stream
    let (
        mut recording_rx, 
        input_sample_rate, 
        input_channels, 
        _stream
    ) = initialize_recording_stream().unwrap();

    // Spawn a task to process and send audio data to the server
    tokio::spawn(async move {
        while let Some(buffer) = recording_rx.recv().await {
            let base64_audio = convert_audio_to_server(&buffer, input_sample_rate, input_channels);
            if let Err(e) = client.input_audio_buffer_append(&base64_audio).await {
                eprintln!("Failed to send audio data to server: {}", e);
            }
        }
    });

    
    // Keep the main task running and manage the stream
    loop {}

}
