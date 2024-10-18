mod client;
mod handle_events;
mod audio_utils;
mod config;

use clap::Parser;
use client::RealtimeClient;
use audio_utils::{convert_audio_to_server, initialize_recording_stream};
use config::{SessionConfig, load_config_from_file};


#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Parser)]
enum Commands {
    Dial(DialArgs),
}

#[derive(Parser)]
struct DialArgs {
    /// Sets the voice type
    #[arg(long)]
    voice: Option<String>,

    /// Sets a custom config file
    #[arg(short = 'f', long)]
    config: Option<String>,
}


// Example usage of the RealtimeClient
#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let cli = Cli::parse();

    match &cli.command {
        Some(Commands::Dial(args)) => {
            // Load configuration
            let mut session_config = if let Some(config_path) = &args.config {
                load_config_from_file(config_path)?
            } else {
                SessionConfig::default()
            };

            // Override with CLI arguments
            if let Some(voice) = &args.voice {
                session_config.voice = voice.to_string();
            }

            // Connect to the WebSocket server
            let mut client = RealtimeClient::new(None, None);
            client.session_config = session_config;
            client.connect(None).await?;

            // Initialize the recording stream
            let (mut recording_rx, input_sample_rate, input_channels, _stream) = initialize_recording_stream()?;

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
        None => {
            println!("Please use the 'dial' subcommand to start a conversation.");
        }
    }

    Ok(())

}
