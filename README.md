# Hotline: Terminal Client for Multimodal AI in Realtime

Hotline is a Rust-based terminal client for interacting with realtime AI, supporting both text and audio modalities.

## Features

- ✅ Real-time communication with AI API using WebSockets
- ✅ Efficient cross platform audio input and output
- ✅ Audio resampling and channel conversion to match server requirements
- 🚧 Controllable audio input modes:
  - ✅ Continous with server-side VAD (Voice Activity Detection)
  - 🔜 Push-to-talk
- 🚧 Two display modes:
  - 🔜 Transcription: Shows a transcript of the user's conversation with the assistant (with roles tagged)
  - ✅ Console: Shows a live feed of events being sent and received (useful for debugging)
- 🔜 Configuration management:
  - 🔜 Read Session Config from YAML file
  - 🔜 CLI interface for easy configuration and control

## Prerequisites

- Rust and Cargo (latest stable version recommended)
- OpenAI Key (currently only supports OpenAI API)

## Installation

1. Clone the repository:
   ```
   git clone https://github.com/ercbot/hotline.git
   cd hotline
   ```

2. Set up your API key:
   ```
   export OPENAI_API_KEY=your_api_key_here
   ```

## Usage

Run the client with:

```
cargo run
```

The client will automatically connect to the AI API and start recording audio from your default input device.

## Project Structure

- `main.rs`: Entry point of the application
- `client.rs`: Contains the `RealtimeClient` struct for WebSocket communication
- `audio_utils.rs`: Handles audio processing, including playback and data coversion
- `handle_events.rs`: Manages incoming events from the WebSocket connection

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

## License

See LICENSE.md

## Disclaimer

This project is not officially associated with any AI service provider. Use it responsibly and in accordance with the API provider's use policies.