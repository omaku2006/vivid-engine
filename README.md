# Vivid Engine

A low-resource wallpaper engine for Wayland, supporting images and videos (including MP4) as wallpapers.

## Features

- Low CPU, GPU, and RAM utilization
- Supports static images and video wallpapers
- Per-workspace wallpaper support (for compositors like Niri)
- Uses Wayland layer-shell for efficient rendering

## Building

```bash
cargo build --release
```

## Running

```bash
cargo run --release
```

## Configuration

Configuration will be added via command-line arguments or a config file in the future.

## License

MIT