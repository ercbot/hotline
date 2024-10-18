use config::{Config, File, FileFormat};
use serde::{Serialize, Deserialize};
use serde_json::Value;
use anyhow::Result;


/// Represents the configuration for a session with the OpenAI Realtime API
/// Represents the configuration for a session with the OpenAI Realtime API
#[derive(Debug, Serialize, Deserialize)]
pub struct SessionConfig {
    pub modalities: Vec<String>,                                                // Supported modalities (e.g., "text", "audio")
    pub instructions: String,                                                   // Custom instructions for the AI
    pub voice: String,                                                          // Voice type for audio responses
    pub input_audio_format: String,                                             // Format of input audio (e.g., "pcm16")
    pub output_audio_format: String,                                            // Format of output audio
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_audio_transcription: Option<Value>,                               // Configuration for audio transcription
    #[serde(skip_serializing_if = "Option::is_none")]
    pub turn_detection: Option<Value>,                                          // Configuration for turn detection in conversations
    pub tools: Vec<Value>,                                                      // Available tools or functions for the AI to use
    pub tool_choice: String,                                                    // How the AI should choose tools
    pub temperature: f32,                                                       // Controls randomness in AI responses
    pub max_response_output_tokens: u32,                                        // Maximum number of tokens in AI responses
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

/// Load the session configuration from a file
pub fn load_config_from_file(config_path: &str) -> Result<SessionConfig> {
    // Check if the file exists
    if !std::path::Path::new(config_path).exists() {
        return Err(anyhow::anyhow!("Config file does not exist"));
    }

    // Load the configuration file
    let config = Config::builder()
        .add_source(File::new(config_path, FileFormat::Yaml))
        // You can add more sources here, like environment variables or command line arguments
        .build()
        .map_err(|e| anyhow::anyhow!("Failed to load config: {}", e)).unwrap_or(Config::default());

    config.try_deserialize().map_err(|e| anyhow::anyhow!("Failed to deserialize config: {}", e))
}