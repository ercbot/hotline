use base64::prelude::*;

pub const SERVER_SAMPLE_RATE: u32 = 24000; // The sample rate of the audio data coming from OpenAI


// Handling User Input -> Server
// Function to convert f32 audio samples to i16 PCM in base64 format
pub fn base64_encode_audio(samples: &[f32]) -> String {
    let audio_data: Vec<u8> = samples
        .iter()
        .map(|sample| (sample * i16::MAX as f32) as i16)
        .flat_map(|sample| sample.to_le_bytes().to_vec())
        .collect();

    BASE64_STANDARD.encode(&audio_data)
}

// Handling Server -> User Output
// Function to decode base64 audio data to f32 samples
pub fn base64_decode_audio(base64_audio_data: &str) -> Vec<f32> {
    let audio_data = BASE64_STANDARD.decode(base64_audio_data).unwrap();

    audio_data
        .chunks_exact(2)
        .map(|chunk| {
            let sample = i16::from_le_bytes([chunk[0], chunk[1]]);
            f32::from(sample) / i16::MAX as f32
        })
        .collect()
}


// Basic resampling function, may replace this with rubato (https://crates.io/crates/rubato)
pub fn resample_audio(samples: &[f32], current_sample_rate: u32, target_sample_rate: u32) -> Vec<f32> {
    // I have no idea why I need to multiply by 2.0, perhaps it's because the audio data is stereo?
    // but I haven't found any documentation confirming that. I just tried it and it worked.
    // The other possiblity is there is an error in the resampling or encoding/decoding
    let resample_ratio = (target_sample_rate as f32 / current_sample_rate as f32) * 2.0;
   
    let output_length = (samples.len() as f32 * resample_ratio) as usize;
    let mut resampled_audio = Vec::with_capacity(output_length);
    
    // Loop through the output length to generate resampled audio
    for i in 0..output_length {
        // Calculate the corresponding index in the input samples
        let index = i as f32 / resample_ratio;
        let index_floor = index.floor() as usize;
        let index_ceil = index.ceil() as usize;
        
        // If the ceiling index is out of bounds, use the last sample
        if index_ceil >= samples.len() {
            resampled_audio.push(samples[samples.len() - 1]);
        } else {
            // Perform linear interpolation between the floor and ceiling samples
            let t = index - index_floor as f32; // weight for interpolation
            let sample = samples[index_floor] * (1.0 - t) + samples[index_ceil] * t;
            resampled_audio.push(sample);
        }
    }
    
    resampled_audio
}
