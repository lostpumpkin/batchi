@echo off
echo Setting up Rust environment for Batchi...
cargo install trunk
rustup target add wasm32-unknown-unknown
echo Setup complete. (Note: venv is not required for this Rust project)
pause
