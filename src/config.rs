use config::{Config, File, FileFormat};
use serde::{Serialize, Deserialize};
use serde_json::Value;
use anyhow::Result;


/// Represents the configuration for a session with the OpenAI Realtime API
#[derive(Debug, Serialize, Deserialize)]
pub struct SessionConfig {
    modalities: Vec<String>,                                                // Supported modalities (e.g., "text", "audio")
    instructions: String,                                                   // Custom instructions for the AI
    voice: String,                                                          // Voice type for audio responses
    input_audio_format: String,                                             // Format of input audio (e.g., "pcm16")
    output_audio_format: String,                                            // Format of output audio
    #[serde(skip_serializing_if = "Option::is_none")]
    input_audio_transcription: Option<Value>,                               // Configuration for audio transcription
    #[serde(skip_serializing_if = "Option::is_none")]
    turn_detection: Option<Value>,                                          // Configuration for turn detection in conversations
    tools: Vec<Value>,                                                      // Available tools or functions for the AI to use
    tool_choice: String,                                                    // How the AI should choose tools
    temperature: f32,                                                       // Controls randomness in AI responses
    max_response_output_tokens: u32,                                        // Maximum number of tokens in AI responses
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
            turn_detection: Some(serde_json::json!({
                "type": "server_vad"
            })),
            tools: Vec::new(),
            tool_choice: "auto".to_string(),
            temperature: 0.8,
            max_response_output_tokens: 4096,
        }
    }
}

pub fn load_config() -> Result<Config> {
    Config::builder()
        .add_source(File::new("config.yaml", FileFormat::Yaml))
        // You can add more sources here, like environment variables or command line arguments
        .build()
        .map_err(|e| anyhow::anyhow!("Failed to load config: {}", e))
}

pub fn get_session_config() -> Result<SessionConfig> {
    let config = load_config()?;
    config.try_deserialize().map_err(|e| anyhow::anyhow!("Failed to deserialize config: {}", e))
}