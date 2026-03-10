# Batchi - EchoScope Web Interface

Batchi is the web-based graphical user interface for the EchoScope bioacoustics platform. It is a Rust-based WebAssembly application built with [Leptos](https://leptos.dev).

## Prerequisites

- [Rust](https://www.rust-lang.org/tools/install) (latest stable)
- `trunk` (WASM bundler)

## Setup

Run `setup.bat` (Windows) or execute:

```bash
cargo install trunk
rustup target add wasm32-unknown-unknown
```

## Running

Run `run.bat` (Windows) or execute:

```bash
trunk serve --open
```

This will start a local development server at `http://127.0.0.1:8080`.

## Architecture

Batchi runs entirely in the browser using WebAssembly. It uses WebGL for high-performance spectrogram rendering.

## Relationship to EchoScope

Batchi serves as the frontend analysis tool, while the core DSP algorithms (in `src/dsp/`) are shared or derived from the EchoScope CLI/Library.
