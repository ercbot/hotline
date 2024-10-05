use std::env;
use futures_util::StreamExt;
use tokio_tungstenite::{connect_async, tungstenite::client::IntoClientRequest, tungstenite::protocol::Message};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Define the URL for the WebSocket connection
    let url: &str = "wss://api.openai.com/v1/realtime?model=gpt-4o-realtime-preview-2024-10-01";

    // Read the OpenAI API key from the environment
    let api_key: String = env::var("OPENAI_API_KEY")
        .expect("OPENAI_API_KEY must be set")
        .to_string();

    // Create a new WebSocket client request from the URL
    let mut request: tungstenite::http::Request<()> = url.into_client_request()?;
    let headers: &mut tungstenite::http::HeaderMap = request.headers_mut();

    // Add the necessary headers to the request
    headers.insert("Authorization", format!("Bearer {}", api_key).parse().unwrap());
    headers.insert("OpenAI-Beta", "realtime=v1".parse().unwrap());
    
    // Connect to the WebSocket server
    let (ws_stream, _) = connect_async(request).await?;
    
    println!("Connected to WebSocket server");

    // Get the read and write halves of the WebSocket stream
    let (_write, read) = ws_stream.split();

    read.for_each(|message| async {
        match message {
            Ok(Message::Text(text)) => println!("Received message: {}", text),
            Ok(Message::Binary(bin)) => println!("Received binary message: {:?}", bin),
            Ok(_) => (),
            Err(e) => println!("Error receiving message: {}", e),
        }
    })
    .await;

    Ok(())
}
