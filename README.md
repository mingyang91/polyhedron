---
title: Polyhedron
emoji: 🚀
colorFrom: yellow
colorTo: yellow
sdk: docker
pinned: false
license: apache-2.0
app_port: 8000
---

# Polyhedron

Polyhedron is a voice chat application designed to enable real-time transcription and translation for training across language barriers.

## Overview
The app allows a trainer to conduct lessons in their native language, while trainees can receive instructions translated into their own languages.

## Key features:

- Real-time voice transcription of the trainer's speech using Amazon Transcribe
- Translates speech into the trainee's language using Amazon Translate
- Displays translated text to trainees in real-time
- Allows trainer to see transcription and repeat unclear sections
- Facilitates training in multilingual organizations
- Polyhedron uses WebSockets to stream audio and text between clients. The frontend is built with React and Vite.

The backend is developed in Rust using the Poem web framework with WebSockets support. It interfaces with AWS services for transcription, translation and text-to-speech.

Configuration like AWS credentials and models are specified in config.yaml.

## Getting Started
To run Polyhedron locally:

1. Config AWS account via https://docs.aws.amazon.com/cli/latest/userguide/cli-configure-files.html
2. Clone the repository, Run `docker compose up`
3. Open http://localhost:8080 in the browser
## Architecture

![Completed Architecture](./docs/HR%20Training-Completed.drawio.svg)

Polyhedron uses a broadcast model to share transcription, translation, and speech synthesis work between clients.

- A single transcription is generated for the speaker and shared with all language clients.

- The transcript is translated once per language and shared with clients of that language.

- Speech is synthesized once per voice and shared with clients selecting that voice.

This optimized architecture minimizes redundant work and cost:

- Automatic speech recognition (ASR) is done only once for the speaker and broadcast.

- Translation is done once per language from the shared transcript and broadcast.

- Text-to-speech (TTS) synthesis is done once per voice and broadcast.

By sharing the intermediate outputs, the system avoids duplicating work across clients. This allows serving many users efficiently and cost effectively.

The components communicate using WebSockets and channels to distribute the shared outputs.

![Simply Architecture](./docs/HR%20Training-Simple.drawio.svg)

The system architecture with a single listener can be summarized as:

- Speaker voice input ->
- ASR Transcription (English) ->
- Translation to Listener language ->
- TTS Synthesis in Listener language ->
- Voice output in Listener language
The speaker's voice is transcribed to text using ASR in the speaker's language (e.g. English).

The transcript is then translated to the listener's language.

Text-to-speech synthesis converts the translated text into a voice audio in the listener's language.

This synthesized voice audio is played out as output to the listener.

The architecture forms a linear pipeline from speaker voice input to listener voice output, with transcription, translation and synthesis steps in between.

## Directory Structure

- `src/`: Main Rust backend source code
    - `main.rs`: Entry point and server definition
    - `config.rs`: Configuration loading
    - `lesson.rs`: Lesson management and audio streaming
    - `whisper.rs`: Whisper ASR integration
    - `group.rs`: Group management
- `static/`: Frontend JavaScript and assets
    - `index.html`: Main HTML page
    - `index.js`: React frontend code
    - `recorderWorkletProcessor.js`: Audio recorder WebWorker
- `models/`: Whisper speech recognition models
- `config.yaml`: Server configuration
- `Cargo.toml`: Rust crate dependencies

## Contributing
Contributions welcome! Please open an issue or PR.
