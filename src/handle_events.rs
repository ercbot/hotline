use crate::audio_utils::{convert_audio_from_server, initialize_playback_stream, PlaybackCommand};
use crate::display_transcript::create_transcript_display;
use crossterm::{
    cursor::{MoveTo, RestorePosition, SavePosition},
    execute,
    style::{Color, Print, ResetColor, SetForegroundColor},
    terminal::{Clear, ClearType},
};
use serde_json::Value;
use std::{
    collections::HashMap,
    io::{stdout, Result},
};
use tokio::sync::mpsc;

pub enum Source {
    Server,
    Client,
}

pub struct Event {
    pub event_type: String,
    pub source: Source,
    pub data: Value,
}

// Define the console display mode as a closure with state
pub fn create_console_display() -> impl FnMut(&Event) -> Result<()> {
    let mut previous_event = String::new();
    let mut consecutive_count = 1;
    let mut current_line = 0;

    move |event: &Event| -> Result<()> {
        let mut stdout = stdout();
        let event_type = &event.event_type;

        if event_type != &previous_event {
            consecutive_count = 1;
            current_line += 1;
        } else {
            consecutive_count += 1;
        }

        // Display the event
        execute!(stdout, SavePosition)?;
        execute!(stdout, MoveTo(0, current_line - 1))?;
        execute!(stdout, Clear(ClearType::CurrentLine))?;
        let (color, source_str) = match event.source {
            Source::Server => (Color::Green, "server"),
            Source::Client => (Color::Blue, "client"),
        };
        execute!(
            stdout,
            SetForegroundColor(color),
            Print(source_str),
            ResetColor,
        )?;
        execute!(
            stdout,
            Print(format!(" {} ({})", event_type, consecutive_count))
        )?;
        execute!(stdout, RestorePosition)?;

        previous_event = event_type.clone();

        Ok(())
    }
}

pub async fn handle_events(mut event_receiver: mpsc::Receiver<Event>) {
    // Clear the screen before starting
    execute!(stdout(), Clear(ClearType::All)).unwrap();

    // Initialize the audio stream
    let (audio_sender, output_sample_rate, output_channels) = initialize_playback_stream();

    // Clear the screen before starting
    execute!(stdout(), Clear(ClearType::All)).unwrap();

    // Create display modes
    let mut console_display = create_console_display();
    let mut transcript_display = create_transcript_display();

    // Current display mode (switch as needed)
    let current_display_mode = "transcript";

    while let Some(event) = event_receiver.recv().await {
        // Use the appropriate display mode
        match current_display_mode {
            "console" => console_display(&event).unwrap(),
            "transcript" => transcript_display(&event).unwrap(),
            _ => (),
        }

        match event.event_type.as_str() {
            "response.audio_transcript.delta" => {
                // // Handle audio transcript delta events
                // let transcript = event["delta"].as_str().unwrap();

                // // Print the transcript
                // print!("{}", transcript);
                // io::stdout().flush().unwrap();
            }
            "response.audio.delta" => {
                // Handle audio delta events
                let base64_audio_data = event.data["delta"].as_str().unwrap();

                // Decode and resample the audio data to the output sample rate
                let samples = convert_audio_from_server(
                    base64_audio_data,
                    output_sample_rate,
                    output_channels,
                );

                // Send the resampled samples to the audio thread
                if let Err(e) = audio_sender.send(PlaybackCommand::Play(samples)) {
                    eprintln!("Failed to send audio samples: {}", e);
                }
            }
            "input_audio_buffer.speech_started" => {
                // Handle speech started events
                audio_sender.send(PlaybackCommand::Stop).unwrap();
            }
            "error" => {
                // Handle error events
                println!("error: {:?}", event.data);
            }
            // Add more event types as needed
            _ => {}
        }
    }
}
