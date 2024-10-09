use futures::stream::{SplitSink, SplitStream};
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{connect_async, tungstenite::client::IntoClientRequest, MaybeTlsStream, WebSocketStream};
use futures::{SinkExt, StreamExt};
use serde::{Serialize, Deserialize};
use serde_json::Value;
use async_trait::async_trait;
use uuid::Uuid;
use url::Url;

use tokio::sync::mpsc;
use tokio::task;

// Defaults
const DEFAULT_URL: &str = "wss://api.openai.com/v1/realtime";
const DEFAULT_MODEL: &str = "gpt-4o-realtime-preview-2024-10-01";


// Define structs for various types used in the API

/// Represents the configuration for a session with the OpenAI Realtime API
#[derive(Debug, Serialize, Deserialize)]
struct SessionConfig {
    modalities: Vec<String>,        // Supported modalities (e.g., "text", "audio")
    instructions: String,           // Custom instructions for the AI
    voice: String,                  // Voice type for audio responses
    input_audio_format: String,     // Format of input audio (e.g., "pcm16")
    output_audio_format: String,    // Format of output audio
    input_audio_transcription: Option<Value>,  // Configuration for audio transcription
    turn_detection: Option<Value>,  // Configuration for turn detection in conversations
    tools: Vec<Value>,              // Available tools or functions for the AI to use
    tool_choice: String,            // How the AI should choose tools
    temperature: f32,               // Controls randomness in AI responses
    max_response_output_tokens: u32,  // Maximum number of tokens in AI responses
}

// Default SessionConfig implementation
impl Default for SessionConfig {
    fn default() -> Self {
        Self {
            modalities: vec!["text".to_string(), "audio".to_string()],
            instructions: String::new(),
            voice: "alloy".to_string(),
            input_audio_format: "pcm16".to_string(),
            output_audio_format: "pcm16".to_string(),
            input_audio_transcription: None,
            turn_detection: None,
            tools: Vec::new(),
            tool_choice: "auto".to_string(),
            temperature: 0.8,
            max_response_output_tokens: 4096,
        }
    }
}



/// Represents an item in the conversation (message, function call, etc.)
#[derive(Debug, Serialize, Deserialize)]
struct Item {
    id: String,
    object: String,
    role: Option<String>,  // "user", "assistant", or "system"
    formatted: Value,      // The content of the item, which can vary based on type
}

/// Trait for handling events from the API
#[async_trait]
trait EventHandler {
    async fn on_event(&mut self, event: &str, origin: &str, data: Option<&Value>);
}

/// Main client for interacting with the OpenAI Realtime API
struct RealtimeClient {
    url: String,                                                    // WebSocket URL
    api_key: String,                                                // OpenAI API key

    is_connected: bool,                                             // Connection status

    ws_read: Option<SplitStream<WebSocketStream<MaybeTlsStream<TcpStream>>>>,    // WebSocket read stream
    ws_write: Option<SplitSink<WebSocketStream<MaybeTlsStream<TcpStream>>, Message>>,   // WebSocket write stream

    session_config: SessionConfig,                                  // Current session configuration
    event_sender: mpsc::Sender<Value>,                              // Event sender
}

impl RealtimeClient {
    /// Creates a new RealtimeClient with default configuration
    fn new(url: Option<&str>, api_key: Option<&str>) -> Self {

        let (event_sender, event_receiver) = mpsc::channel(100);
        
        // Spawn a task to handle events
        tokio::spawn(handle_events(event_receiver));
        
        let url = url.unwrap_or(DEFAULT_URL);

        // Get the API key from the argument or environment variable
        let api_key = api_key
            .map(|key| key.to_string())
            .or_else(|| std::env::var("OPENAI_API_KEY").ok())
            .expect("API key must be provided either as an argument or in the environment variable OPENAI_API_KEY");

        Self {
            url: url.to_string(),
            api_key,

            is_connected: false,

            ws_read: None,
            ws_write: None,
            session_config: SessionConfig::default(),
            event_sender
        }
    }

    /// Establishes a WebSocket connection with the OpenAI Realtime API
    async fn connect(&mut self, model: Option<&str>) -> Result<(), Box<dyn std::error::Error>> {
        if self.is_connected {
            return Err("RealtimeClient is already , use .disconnect() first".into());
        }

        // Clone the URL and parse it into a URL object
        let mut url = Url::parse(&self.url)?;

        // Add the model parameter to the URL if provided
        url.query_pairs_mut().append_pair("model", model.unwrap_or(DEFAULT_MODEL));

        // Create a new WebSocket client request from the URL
        let mut request = url.into_client_request()?;
        
        // Add the necessary headers to the request
        let headers = request.headers_mut();
        headers.insert(
            "Authorization",
            format!("Bearer {}", self.api_key).parse().unwrap(),
        );
        headers.insert("OpenAI-Beta", "realtime=v1".parse().unwrap());

        let (ws_stream, _) = connect_async(request).await?;

        // Split the WebSocket stream into read and write halves
        let (ws_write, ws_read) = ws_stream.split();

        self.ws_read = Some(ws_read);
        self.ws_write = Some(ws_write);

        self.is_connected = true;
        
        // self.start_handling_messages().await?;  // Start handling incoming messages

        self.update_session().await?;  // Send session configuration
        Ok(())
    }

    /// Closes the WebSocket connection
    async fn disconnect(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        if self.is_connected {
            if let Some(ws_write) = &mut self.ws_write {
                ws_write.send(Message::Close(None)).await?;
            }
            self.ws_write = None;
            self.ws_read = None;
            self.is_connected = false;
        } 
        else {
            return Err("RealtimeClient is not connected".into());
        }
        Ok(())
    }

    /// Sends the current session configuration to the API
    async fn update_session(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let data = serde_json::to_value(serde_json::json!({"session": self.session_config}))?;
        self.send("session.update", Some(data)).await?;

        Ok(())
    }

    /// Sends a user message to the API
    async fn send_user_message_content(&mut self, content: Value) -> Result<(), Box<dyn std::error::Error>> {
        
        self.send("conversation.item.create", Some(serde_json::json!({
            "item": {
                "type": "message",
                "role": "user",
                "content": content
            }
        }))).await?;
        
        self.create_response().await?;

        Ok(())
    }

    /// Requests the API to generate a response
    async fn create_response(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        self.send("response.create", None).await?;
        
        Ok(())
    }

    /// Starts handling incoming messages in a separate task
    async fn start_handling_messages(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let event_sender = self.event_sender.clone();
        let mut ws_read = self.ws_read.take().expect("WebSocket read stream is not initialized");

        tokio::spawn(async move {
            while let Some(message) = ws_read.next().await {
            match message {
                Ok(Message::Text(text)) => {
                if let Ok(value) = serde_json::from_str::<Value>(&text) {
                    if event_sender.send(value).await.is_err() {
                    eprintln!("Error sending event through channel");
                    break;
                    }
                }
                }
                Err(e) => {
                eprintln!("Error receiving WebSocket message: {}", e);
                break;
                }
                _ => {}
            }
            }
        });

        Ok(())
    }

    /// Sends an event to WebSocket server
    async fn send(&mut self, event_type: &str, data: Option<Value>) -> Result<(), Box<dyn std::error::Error>> {
        let mut event = serde_json::json!({
            "type": event_type,
            "event_id": Uuid::new_v4().to_string(),
        });

        if let Some(data) = data {
            event.as_object_mut().unwrap().extend(data.as_object().unwrap().clone());
        }

        if let Some(ws_write) = &mut self.ws_write {
            ws_write.send(Message::Text(serde_json::to_string(&event)?)).await?;
        } else {
            return Err(format!("Cannot send {} - client is not connected", event_type).into());
        }

        // Also send the event to our local event handler
        self.event_sender.send(event).await
            .map_err(|e| format!("Failed to send event to local handler: {}", e))?;

        Ok(())
    }

}

async fn handle_events(mut event_receiver: mpsc::Receiver<Value>) {
    while let Some(event) = event_receiver.recv().await {
        if let Some(event_type) = event.get("type").and_then(Value::as_str) {
            match event_type {
                "conversation.item.create" => {
                    // Handle conversation item creation
                    println!("New conversation item: {:?}", event);
                },
                "response.create" => {
                    // Handle response creation
                    println!("New response created: {:?}", event);
                },
                "error" => {
                    // Handle error events
                    println!("Error event: {:?}", event);
                },
                // Add more event types as needed
                _ => println!("Unhandled event type: {}", event_type),
            }
        }
    }
}


// Example usage of the RealtimeClient
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Connect to the WebSocket server
    let mut client = RealtimeClient::new(None, None);
    client.connect(None).await?;

    // Start Handling Messages
    client.start_handling_messages().await?;

    // Send a user message
    client.send_user_message_content(serde_json::json!({"text": "Hello, AI!"})).await?;


    // Keep the main function alive to simulate continuous interaction
    loop {
        tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;
    }
}

