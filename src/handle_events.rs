use tokio::sync::{mpsc, Mutex};
use std::{io::{self, Write}, sync::Arc};
use serde_json::Value;

use ringbuf::{storage::Heap, traits::Split, HeapRb};

use cpal::traits::{DeviceTrait, HostTrait};

const RING_BUFFER_CAPACITY: usize = 1024;

pub async fn handle_events(mut event_receiver: mpsc::Receiver<Value>) {
    // // Initialize audio playback stream
    // // Create a thread-safe, ring buffer for output audio samples
    // let audio_buffer = HeapRb::<f32>::new(RING_BUFFER_CAPACITY);

    // // Split the buffer into producer and consumer
    // let (producer, consumer) = audio_buffer.split();

    // // Wrap the consumer in a Mutex
    // let consumer = Arc::new(Mutex::new(consumer));

    // // Initialize audio playback stream
    // let host = cpal::default_host();
    // let device = host
    //     .default_output_device()
    //     .expect("No output device available");
    // let config = device.default_output_config().unwrap();
    // let output_sample_rate = config.sample_rate().0;

    // let stream = device
    //     .build_output_stream(
    //         &config.into(),
    //         move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
    //             let mut consumer = consumer_clone.lock().unwrap();
    //             for sample in data.iter_mut() {
    //                 *sample = match consumer.pop() {
    //                     Some(s) => s,
    //                     None => 0.0, // Output silence if the buffer is empty
    //                 };
    //             }
    //         },
    //         |err| eprintln!("An error occurred on the output stream: {}", err),
    //         None,
    //     )?;
    
    while let Some(event) = event_receiver.recv().await {
        if let Some(event_type) = event.get("type").and_then(Value::as_str) {
            match event_type {
                "conversation.item.create" => {
                    // Handle conversation item creation
                },
                "response.create" => {
                    // Handle response creation
                },
                "response.audio_transcript.delta" => {
                    // Handle audio transcript delta events
                    let transcript = event["delta"].as_str().unwrap();

                    // Print the transcript
                    print!("{}", transcript);
                    io::stdout().flush().unwrap();
                },
                "error" => {
                    // Handle error events
                    println!("Error event: {:?}", event);
                },
                // Add more event types as needed
                // _ => println!("Unhandled event type: {}", event_type),
                _ => (),
            }
        }
    }
}