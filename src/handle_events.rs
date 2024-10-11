use tokio::sync::mpsc;
use serde_json::Value;
use crossterm::{
    execute,
    style::{Color, Print, ResetColor, SetForegroundColor},
    terminal::{Clear, ClearType},
    cursor::{MoveTo, SavePosition, RestorePosition},
};
use std::io::{stdout, Result};

use crate::audio_utils::{convert_audio_from_server, initialize_playback_stream, PlaybackCommand};

pub enum Source {
    Server,
    Client,
}

pub struct Event {
    pub event_type: String,
    pub source: Source,
    pub data: Value,
}

pub async fn handle_events(mut event_receiver: mpsc::Receiver<Event>) {
    // Initialize the audio stream
    let (audio_sender, output_sample_rate, output_channels) = initialize_playback_stream();

    // Clear the screen before starting
    execute!(stdout(), Clear(ClearType::All)).unwrap();

    // Initalizing the console dispaly vars
    let mut previous_event = String::new();
    let mut consecutive_count = 1;
    let mut current_line = 0;

    while let Some(event) = event_receiver.recv().await {
        // Display the event in the console
        let event_type = event.event_type.clone();

        if event_type != previous_event {
            // If the event type is different from the previous event, display the new event
            display_event_console(&event_type, event.source, 1, current_line).unwrap();
            consecutive_count = 1;
            current_line += 1;
        } else {
            // If the event type is the same as the previous event, increment the count
            consecutive_count += 1;
            display_event_console(&event.event_type, event.source, consecutive_count, current_line).unwrap();
        }

        previous_event = event_type.clone();

        match event.event_type.as_str() {
            "response.audio_transcript.delta" => {
                // // Handle audio transcript delta events
                // let transcript = event["delta"].as_str().unwrap();

                // // Print the transcript
                // print!("{}", transcript);
                // io::stdout().flush().unwrap();
            },
            "response.audio.delta" => {
                // Handle audio delta events
                let base64_audio_data = event.data["delta"].as_str().unwrap();

                // Decode and resample the audio data to the output sample rate
                let samples = convert_audio_from_server(base64_audio_data, output_sample_rate, output_channels);

                // Send the resampled samples to the audio thread
                if let Err(e) = audio_sender.send(PlaybackCommand::Play(samples)) {
                    eprintln!("Failed to send audio samples: {}", e);
                }
            },
            "input_audio_buffer.speech_started" => {
                // Handle speech started events
                audio_sender.send(PlaybackCommand::Stop).unwrap();
            },
            "error" => {
                // Handle error events
                println!("error: {:?}", event.data);
            },
            // Add more event types as needed
            _ => {}
        }
    }
}


fn display_event_console(event_type: &str, source: Source, count: usize, line: u16) -> Result<()> {
    let mut stdout = stdout();
    
    // Save cursor position
    execute!(stdout, SavePosition)?;
    
    // Move to the specific line
    execute!(stdout, MoveTo(0, line))?;
    
    // Clear the current line
    execute!(stdout, Clear(ClearType::CurrentLine))?;
    
    // Set color based on event source
    let (color, source_str) = match source {
        Source::Server => (Color::Green, "server"),
        Source::Client => (Color::Blue, "client"),
    };

    // Print the colored source
    execute!(
        stdout,
        SetForegroundColor(color),
        Print(source_str),
        ResetColor,
    )?;

    // Print the rest of the message in default color
    execute!(
        stdout,
        Print(format!(" {} ({})", event_type, count)),
    )?;
    
    // Restore cursor position
    execute!(stdout, RestorePosition)?;
    
    Ok(())
}