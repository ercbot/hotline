use tokio::net::TcpStream;
use tokio_tungstenite::{connect_async, tungstenite::client::IntoClientRequest, MaybeTlsStream, WebSocketStream};
use futures::{SinkExt, StreamExt};
use serde::{Serialize, Deserialize};
use serde_json::Value;
use async_trait::async_trait;
use uuid::Uuid;
use url::Url;


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

    ws_stream: Option<WebSocketStream<MaybeTlsStream<TcpStream>>>,  // WebSocket connection
    session_config: SessionConfig,                                  // Current session configuration
    event_handlers: Vec<Box<dyn EventHandler>>,                     // Registered event handlers
}

impl RealtimeClient {
    /// Creates a new RealtimeClient with default configuration
    fn new(url: Option<&str>, api_key: Option<&str>) -> Self {
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

            ws_stream: None,
            session_config: SessionConfig {
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
            },
            event_handlers: Vec::new(),
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
        self.ws_stream = Some(ws_stream);
        self.is_connected = true;
        
        self.update_session().await?;  // Send session configuration
        Ok(())
    }

    /// Closes the WebSocket connection
    async fn disconnect(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(ws_stream) = &mut self.ws_stream {
            ws_stream.close(None).await?;
            self.ws_stream = None;
            self.is_connected = false;
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

    /// Registers a new event handler
    fn add_event_handler(&mut self, handler: Box<dyn EventHandler>) {
        self.event_handlers.push(handler);
    }

    /// Listens for and processes incoming messages from the API
    async fn handle_incoming_messages(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(ws_stream) = &mut self.ws_stream {
            while let Some(message) = ws_stream.next().await {
                let message = message?;
                if let tokio_tungstenite::tungstenite::Message::Text(text) = message {
                    let value: Value = serde_json::from_str(&text)?;
                    if let Some(event_type) = value["type"].as_str() {
                        // Call all registered event handlers
                        for handler in &mut self.event_handlers {
                            handler.on_event(event_type, 
                                "server", Some(&value)).await;
                        }
                    }
                }
            }
        }
        Ok(())
    }

    /// Sends an event to WebSocket server
    async fn send(&mut self, event_type: &str, data: Option<Value>) -> Result<bool, Box<dyn std::error::Error>> {
        if self.ws_stream.is_none() {
            return Err("RealtimeClient is not connected".into());
        }

        let mut event = serde_json::Map::new();
        event.insert("event_id".to_string(), serde_json::json!(Uuid::new_v4().to_string()));
        event.insert("type".to_string(), serde_json::json!(event_type));
        
        // send value to event handler
        for handler in &mut self.event_handlers {
            handler.on_event(event_type, "client", data.as_ref()).await;
        }

        if let Some(data) = data {
            if let Value::Object(map) = data {
                for (k, v) in map {
                    event.insert(k, v);
                }
            } else {
                return Err("data must be an object".into());
            }
        }

        let event_value = Value::Object(event);
        let event_string = serde_json::to_string(&event_value)?;

        println!("Sending event: {}", event_string);

        if let Some(ws_stream) = &mut self.ws_stream {
            ws_stream.send(tokio_tungstenite::tungstenite::Message::Text(event_string)).await?;
        }

        Ok(true)
    }

}

// Example usage of the RealtimeClient
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut client = RealtimeClient::new(None, None);
    
    // Connect to the OpenAI Realtime API
    client.connect(None).await?;

    // Example event handler implementation
    struct ConsoleHandler;
    #[async_trait]
    impl EventHandler for ConsoleHandler {
        async fn on_event(&mut self, event: &str, origin: &str, data: Option<&Value>) {
            println!("{}: {}", origin, event);
            if event == "error" {
                if let Some(data) = data {
                    println!("Error data: {}", data);
                }
            }
            if origin == "client" {
                if let Some(data) = data {
                    println!("Data: {}", data);
                }
            }
        }
    }

    // Register the event handler
    client.add_event_handler(Box::new(ConsoleHandler));

    // Send a user message
    client.send_user_message_content(serde_json::json!({"text": "Hello, AI!"})).await?;

    // Start handling incoming messages (this will run indefinitely)
    client.handle_incoming_messages().await?;

    Ok(())
}

