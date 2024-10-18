use futures::stream::{SplitSink, SplitStream};
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{connect_async, tungstenite::client::IntoClientRequest, MaybeTlsStream, WebSocketStream};
use futures::{SinkExt, StreamExt};
use serde_json::Value;
use uuid::Uuid;
use url::Url;

use anyhow::{Result, Context, bail};

use tokio::sync::mpsc;

use crate::handle_events::{handle_events, Event, Source};
use crate::config::SessionConfig;

// Defaults
const DEFAULT_URL: &str = "wss://api.openai.com/v1/realtime";
const DEFAULT_MODEL: &str = "gpt-4o-realtime-preview-2024-10-01";

/// Main client for interacting with the OpenAI Realtime API
pub struct RealtimeClient {
    url: String,                                                    // WebSocket URL
    api_key: String,                                                // OpenAI API key

    is_connected: bool,                                             // Connection status

    ws_read: Option<SplitStream<WebSocketStream<MaybeTlsStream<TcpStream>>>>,    // WebSocket read stream
    ws_write: Option<SplitSink<WebSocketStream<MaybeTlsStream<TcpStream>>, Message>>,   // WebSocket write stream

    pub session_config: SessionConfig,                                  // Current session configuration
    event_sender: mpsc::Sender<Event>,                              // Event sender
}

impl RealtimeClient {
    /// Creates a new RealtimeClient with default configuration
    pub fn new(url: Option<&str>, api_key: Option<&str>) -> Self {

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
    pub async fn connect(&mut self, model: Option<&str>) -> Result<()> {
        if self.is_connected {
            bail!("RealtimeClient is already connected, use .disconnect() first");
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
        
        self.start_handling_messages().await.context("Failed to start handling messages")?;

        self.update_session().await.context("Failed to update session")?;
        Ok(())
    }

    /// Closes the WebSocket connection
    pub async fn disconnect(&mut self) -> Result<()> {
        if self.is_connected {
            if let Some(ws_write) = &mut self.ws_write {
                ws_write.send(Message::Close(None)).await
                    .context("Failed to send close message")?;
            }
            self.ws_write = None;
            self.ws_read = None;
            self.is_connected = false;
            Ok(())
        } else {
            bail!("RealtimeClient is not connected")
        }
    }

    /// Sends the current session configuration to the API
    pub async fn update_session(&mut self) -> Result<()> {
        let data = serde_json::to_value(serde_json::json!({"session": self.session_config}))
            .context("Failed to serialize session config")?;
        self.send("session.update", Some(data)).await
            .context("Failed to send session update")?;
        Ok(())
    }

    /// Sends a message with the specified content to the API
    pub async fn send_user_message_content(&mut self, content: Vec<Value>) -> Result<()> {
        self.send("conversation.item.create", Some(serde_json::json!({
            "item": {
                "type": "message",
                "role": "user",
                "content": content
            }
        }))).await.context("Failed to send user message content")?;
       
        self.create_response().await.context("Failed to create response after sending user message")?;
        Ok(())
    }

    /// Requests the API to generate a response
    pub async fn create_response(&mut self) -> Result<()> {
        self.send("response.create", None).await
            .context("Failed to create response")?;
       
        Ok(())
    }

    /// Input audio buffer append
    pub async fn input_audio_buffer_append(&mut self, base64_audio_data: &str) -> Result<()> {
        self.send("input_audio_buffer.append", Some(serde_json::json!({
            "audio": base64_audio_data
        }))).await.context("Failed to append to input audio buffer")?;
        
        Ok(())
    }

    /// Input audio buffer commit
    pub async fn input_audio_buffer_commit(&mut self) -> Result<()> {
        self.send("input_audio_buffer.commit", None).await
            .context("Failed to commit input audio buffer")?;
        
        Ok(())
    }

    // Private methods

    /// Starts handling incoming messages in a separate task
    async fn start_handling_messages(&mut self) -> Result<()> {
        let event_sender = self.event_sender.clone();
        let mut ws_read = self.ws_read.take()
            .context("WebSocket read stream is not initialized")?;

        tokio::spawn(async move {
            while let Some(message) = ws_read.next().await {
                match message {
                    Ok(Message::Text(text)) => {
                        if let Ok(value) = serde_json::from_str::<Value>(&text) {
                            let event = Event {
                                event_type: value["type"].as_str()
                                    .unwrap_or("unknown")
                                    .to_string(),
                                source: Source::Server,
                                data: value.clone(),
                            };
                            if event_sender.send(event).await.is_err() {
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

    async fn send(&mut self, event_type: &str, data: Option<Value>) -> Result<()> {
        let mut event_data = serde_json::json!({
            "type": event_type,
            "event_id": Uuid::new_v4().to_string(),
        });

        if let Some(data) = data {
            event_data.as_object_mut()
                .context("Failed to mutate event_data as object")?
                .extend(data.as_object()
                    .context("Provided data is not a valid JSON object")?
                    .clone());
        }

        if let Some(ws_write) = &mut self.ws_write {
            let message = serde_json::to_string(&event_data)
                .context("Failed to serialize event data")?;
            ws_write.send(Message::Text(message)).await
                .context("Failed to send message through WebSocket")?;
        } else {
            bail!("Cannot send {} - client is not connected", event_type);
        }

        // Send the event to the local event handler
        let event = Event {
            event_type: event_type.to_string(),
            source: Source::Client,
            data: event_data,
        };

        // Also send the event to our local event handler
        self.event_sender.send(event).await
            .context("Failed to send event to local handler")?;

        Ok(())
    }

}