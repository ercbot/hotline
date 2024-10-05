use anyhow::Result;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::{Arc, Mutex};
use std::time::Duration;


// Resample the input audio to match the output sample rate
fn resample(input: &[f32], input_rate: usize, output_rate: usize) -> Vec<f32> {
    let ratio = output_rate as f32 / input_rate as f32;
    let output_len = (input.len() as f32 * ratio).ceil() as usize;
    let mut output = Vec::with_capacity(output_len);

    for i in 0..output_len {
        let index = i as f32 / ratio;
        let index_floor = index.floor() as usize;
        let index_ceil = index.ceil() as usize;

        if index_ceil >= input.len() {
            output.push(input[input.len() - 1]);
        } else {
            let t = index - index_floor as f32;
            let sample = input[index_floor] * (1.0 - t) + input[index_ceil] * t;
            output.push(sample);
        }
    }

    output
}

fn main() -> Result<()> {
    // Get the default audio host
    let host = cpal::default_host();

    // Get default input and output devices, handling errors if they don't exist
    let input_device = host
        .default_input_device()
        .expect("no input device available");
    let output_device = host
        .default_output_device()
        .expect("no output device available");

    // Print the names of the input and output devices
    println!("Input device: {}", input_device.name()?);
    println!("Output device: {}", output_device.name()?);

    // Get the default input and output configuration and convert it to a StreamConfig
    let input_config: cpal::StreamConfig = input_device.default_input_config()?.into();
    let output_config: cpal::StreamConfig = output_device.default_output_config()?.into();

    // Define the desired length of the recording and playback in seconds
    const SECONDS: u64 = 5;
    let latency = Duration::from_secs(SECONDS);

    // From the Input device, get sample rate, channel count
    let input_sample_rate = input_config.sample_rate.0 as usize;
    // let channel_count = input_config.channels as usize;

    // From the Output device, get sample rate, channel count
    let output_sample_rate = output_config.sample_rate.0 as usize;

    // Create a vector to store the recorded audio
    let recorded_samples = Arc::new(Mutex::new(Vec::new()));

    // Record audio
    println!("Recording for 5 seconds...");
    {
        let recorded_samples = recorded_samples.clone();
        let input_stream = input_device.build_input_stream(
            &input_config,
            move |data: &[f32], _: &cpal::InputCallbackInfo| {
                // Append the recorded audio data into the vector
                let mut samples = recorded_samples.lock().unwrap();
                samples.extend_from_slice(data);
            },
            |err| eprintln!("An error occurred on the input stream: {}", err),
            None,
        )?;

        input_stream.play()?; // Start recording
                              // Wait for 5 seconds, counting down the seconds
        for i in (0..5).rev() {
            println!("{}...", i);
            std::thread::sleep(Duration::from_secs(1));
        }
        drop(input_stream); // Stop recording
    }

    // Resample the recorded audio
    let resampled_samples = {
        let samples = recorded_samples.lock().unwrap();
        resample(&samples, input_sample_rate, output_sample_rate)
    };

    // Prepare for playback
    println!("Playing back the recorded audio...");
    let playback_data = Arc::new(Mutex::new((resampled_samples, 0))); // (samples, position)

    {
        let playback_data = playback_data.clone();
        let output_stream = output_device.build_output_stream(
            &output_config,
            move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                let mut playback = playback_data.lock().unwrap();
                let samples = &playback.0;
                let position = playback.1;
                let len = data.len();

                if position + len <= samples.len() {
                    data.copy_from_slice(&samples[position..position + len]);
                    playback.1 += len;
                } else {
                    // Not enough samples left, fill with remaining samples and silence
                    let remaining = samples.len() - position;
                    if remaining > 0 {
                        data[..remaining].copy_from_slice(&samples[position..]);
                    }
                    data[remaining..].iter_mut().for_each(|s| *s = 0.0);
                    playback.1 = samples.len();
                }
            },
            |err| eprintln!("An error occurred on the output stream: {}", err),
            None,
        )?;

        output_stream.play()?; // Start playback
        std::thread::sleep(latency); // Wait for 5 seconds
    }

    Ok(()) // Return Ok if everything went well
}
