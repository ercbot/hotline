use base64::prelude::*;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use crossterm::terminal::enable_raw_mode;
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use futures_util::{SinkExt, StreamExt};
use serde_json::Value;
use std::collections::VecDeque;
use std::env;
use std::io::{self, Write};
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tokio_tungstenite::{
    connect_async, tungstenite::client::IntoClientRequest, tungstenite::protocol::Message,
};

use std::sync::{Arc, Mutex};

mod handle_server_event;
use handle_server_event::handle_server_event;

mod audio_playback;
use audio_playback::initialize_audio_playback;

mod audio_utils;
use audio_utils::{base64_encode_audio, resample_audio, SERVER_SAMPLE_RATE};

async fn response_create(tx: mpsc::UnboundedSender<Message>) {
    let event = serde_json::json!({
        "type": "response.create"
    });

    if let Err(e) = tx.send(Message::Text(event.to_string())) {
        eprintln!("Error sending message: {}", e);
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Define the URL for the WebSocket connection
    let url: &str = "wss://api.openai.com/v1/realtime?model=gpt-4o-realtime-preview-2024-10-01";

    // Read the OpenAI API key from the environment
    let api_key: String = env::var("OPENAI_API_KEY")
        .expect("OPENAI_API_KEY must be set")
        .to_string();

    // Create a new WebSocket client request from the URL
    let mut request = url.into_client_request()?;
    let headers = request.headers_mut();

    // Add the necessary headers to the request
    headers.insert(
        "Authorization",
        format!("Bearer {}", api_key).parse().unwrap(),
    );
    headers.insert("OpenAI-Beta", "realtime=v1".parse().unwrap());

    // Connect to the WebSocket server
    let (ws_stream, _) = connect_async(request).await?;
    println!("Connected to WebSocket server");

    // Split the WebSocket stream into read and write halves
    let (mut write, mut read) = ws_stream.split();

    // Create an mpsc channel to send events to the write task
    let (tx, mut rx) = mpsc::unbounded_channel();

    // Create a thread-safe, growable buffer for output audio samples
    let audio_buffer = Arc::new(Mutex::new(VecDeque::new()));

    // Initialize audio playback stream
    let (stream, output_sample_rate) = initialize_audio_playback(Arc::clone(&audio_buffer))?;

    stream.play().unwrap();

    // Spawn a task to handle sending events
    tokio::spawn(async move {
        while let Some(event) = rx.recv().await {
            if let Err(e) = write.send(event).await {
                eprintln!("Error sending event: {}", e);
                break;
            }
        }
    });

    // Spawn a task to handle reading incoming events
    let buffer_for_ws = Arc::clone(&audio_buffer);
    let read_handle = tokio::spawn(async move {
        read.for_each(|event| async {
            handle_server_event(event, &buffer_for_ws, output_sample_rate).await;
        })
        .await;
    });

    // Set up audio input
    let host = cpal::default_host();
    let input_device = host.default_input_device().expect("No input device available");
    let input_config = input_device.default_input_config().unwrap();

    let input_sample_rate = input_config.sample_rate().0;

    // Create a flag to indicate whether we're currently recording
    let is_recording = Arc::new(Mutex::new(false));

    let tx_clone = tx.clone();
    let is_recording_clone = Arc::clone(&is_recording);
    let input_data_fn = move |data: &[f32], _: &cpal::InputCallbackInfo| {
        let is_rec = *is_recording_clone.lock().unwrap();
        if is_rec {
            let resampled_data = resample_audio(data, input_sample_rate, SERVER_SAMPLE_RATE);
            let base64_audio = base64_encode_audio(&resampled_data);

            let audio_event = serde_json::json!({
                "type": "input_audio_buffer.append",
                "audio": base64_audio
            });

            if let Err(e) = tx_clone.send(Message::Text(audio_event.to_string())) {
                eprintln!("Error sending audio data: {}", e);
            }
        }
    };

    let input_stream = input_device.build_input_stream(
        &input_config.into(),
        input_data_fn,
        |err| eprintln!("Error in input stream: {}", err),
        None,
    )?;

    input_stream.play()?;

    // Enable raw mode for terminal input
    enable_raw_mode()?;

    println!("Push-to-Talk enabled. Hold SPACE to record.");

    // Main event loop
    let mut last_key_time = Instant::now();

    // Set a timeout for the key press
    let key_timeout = Duration::from_millis(500);

    loop {
        if event::poll(Duration::from_millis(10))? {
            if let Event::Key(key_event) = event::read()? {
                match key_event.code {
                    KeyCode::Char(' ') => {
                        last_key_time = Instant::now();
                        let mut is_rec = is_recording.lock().unwrap();
                        if !*is_rec {
                            *is_rec = true;
                            println!("Recording started");
                        }
                    }
                    KeyCode::Esc => {
                        println!("Exiting...");
                        break;
                    }
                    KeyCode::Char('c') if key_event.modifiers.contains(KeyModifiers::CONTROL) => {
                        println!("Exiting...");
                        break;
                    }
                    _ => {}
                }
            }
        } else if *is_recording.lock().unwrap() && last_key_time.elapsed() > key_timeout {
            let mut is_rec = is_recording.lock().unwrap();
            *is_rec = false;
            println!("Recording stopped");
        }

        // Add a small delay to prevent the loop from running too fast
        std::thread::sleep(Duration::from_millis(10));
    }



    Ok(())
}
