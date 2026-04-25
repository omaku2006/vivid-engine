# 🌌 Vivid Engine — Ultra Lightweight Wayland Wallpaper Engine

> ⚡ *“Fully vibe-coded (AI-built), performance-focused, minimal-resource wallpaper engine for modern Wayland setups.”*

---

## 🚀 Overview

**Vivid Engine** is a high-performance, low-resource wallpaper engine built in **Rust** for **Wayland compositors**.
It supports both **static images** and **video wallpapers**, with **smooth animations**, **IPC control**, and **optimized CPU/RAM usage**.

This project is designed with one core philosophy:

> 🔥 *Maximum visual experience with minimum system load.*

- Contributions & PRs are welcome to harden the codebase!
---

## ✨ Key Features

### 🎬 Visual Experience

* 🖼️ Static image wallpapers (PNG, JPEG, WebP)
* 🎥 Video wallpapers (MP4, MKV, WebM, AVI, etc.)
* 🌈 15+ smooth transition animations
* 🔁 Looping video playback
* ⚡ Real-time animation switching

---
## Demo


---
### ⚙️ Performance Optimizations

#### 🧠 CPU Optimization

* 🚀 **Rayon parallel rendering** (multi-core CPU utilization)
* 🎯 Row-wise parallel pixel processing
* ⏱️ Frame timing control (~60 FPS)
* ❌ No unnecessary allocations (buffer reuse)

#### 💾 RAM Optimization

* 🧩 Shared memory buffers (Wayland SHM)
* 🔁 Buffer reuse (no repeated allocations)
* 📉 Minimal memory footprint (~low MB usage)
* 🧹 Proper cleanup of old buffers

#### 🎥 FFmpeg Optimization

* 📦 Raw video streaming (no heavy decoding layer)
* 🎯 Direct BGRA frame output
* 🔄 Streaming via stdout (zero disk overhead)
* 🧵 Dedicated decoding thread

---

### 🔌 IPC System (Advanced Control)

* 🧠 Unix socket-based IPC (`/tmp/vivid-engine.sock`)
* ⚡ Instant wallpaper switching
* 🎛️ Live animation control
* 📊 Runtime status monitoring

---

## 🧱 Architecture

```
User Command
     ↓
IPC (Unix Socket)
     ↓
Wallpaper Engine Core
     ↓
Wayland Layer-Shell
     ↓
Display Output
```

### Core Modules

| Module            | Description                              |
| ----------------- | ---------------------------------------- |
| `engine.rs`       | Core rendering + Wayland integration     |
| `animations.rs`   | All animation logic (parallel optimized) |
| `video_player.rs` | FFmpeg-based video streaming             |
| `ipc.rs`          | Communication between processes          |
| `config.rs`       | Persistent configuration                 |
| `args.rs`         | CLI argument parsing                     |

---

## 📦 Installation

### 🔧 Requirements

* Rust (latest stable)
* Wayland compositor (Hyprland, Niri, Sway, etc.)
* FFmpeg

### 📥 Install Dependencies (Arch Linux example)

```bash
sudo pacman -S rust ffmpeg
```

### 🏗️ Build

```bash
git clone https://github.com/omaku2006/vivid-engine.git
cd vivid-engine
cargo build --release
```

---

## ▶️ Usage

### 🚀 Start Engine (Daemon Mode)

```bash
./target/release/vivid-engine
```

---

### 🖼️ Set Image Wallpaper

```bash
vivid-engine ~/wallpapers/image.jpg
```

---

### 🎥 Set Video Wallpaper

```bash
vivid-engine ~/videos/wallpaper.mp4
```

---

### 🎬 Set Animation

```bash
vivid-engine -a fade
```

---

### ⏱️ Set Animation Duration

```bash
vivid-engine -a 1.5
```

---

### 📋 List All Animations

```bash
vivid-engine -a list
```

---

## 🎞️ Available Animations

```
fade, wipe, split, center, outer,
pixel, dissolve, glitch,
slide_up, slide_down,
zoom, blinds, diagonal, wave, random
```

---

## ⚡ Performance Breakdown

### 🧠 CPU Usage

* Idle (image): ~0–2%
* Animation: ~5–15%
* Video: ~10–25% (depends on resolution)

---

### 💾 RAM Usage

* Base engine: ~5–15 MB
* Video buffer: ~10–30 MB (depends on resolution)

---

### 🎥 FFmpeg Usage

* Runs as subprocess
* Streams raw frames directly
* Automatically killed when switching wallpapers

---

## 🔄 How Video Pipeline Works

```
FFmpeg → Raw BGRA Frames → Shared Buffer → Wayland Surface → Display
```

* No GUI player involved
* No extra decoding layers
* Pure pipeline → high efficiency

---

## 🧠 Smart Features

### 🛑 Safe Video Switching

* Old FFmpeg process is **force killed**
* Thread cleanup ensured
* No zombie processes

---

### 🔁 Buffer Reuse

* Same SHM buffer reused for frames
* Prevents memory leaks

---

### 🎯 Frame Sync

* ~16ms delay → ~60 FPS rendering
* Smooth playback

---

## 📂 Config System

Location:

```
~/.config/vivid-engine/state.conf
```

Example:

```
LAST_WALLPAPER=/home/user/wall.jpg
ANIMATION=fade
DURATION=0.50
```

---

## 🔌 IPC Commands

| Command       | Description       |
| ------------- | ----------------- |
| SET_WALLPAPER | Change wallpaper  |
| SET_ANIMATION | Change animation  |
| SET_DURATION  | Change speed      |
| GET_STATUS    | Get current state |

---

## 🧪 Advanced Notes

* Uses **Wayland Layer Shell (wlr-layer-shell)**
* Runs as background daemon
* Supports multi-workspace setups
* Designed for compositors like:

  * Hyprland
  * Niri
  * Sway

---

## ⚠️ Limitations

* ❌ X11 not supported
* ❌ GPU acceleration not used (CPU optimized instead)
* ❌ No GUI (CLI only)

---

## 🛠️ Future Plans

* 🎛️ GUI control panel
* ⚙️ Config file improvements
* 🧠 Smart auto-wallpaper switching
* 🎵 Audio support (optional)

---

## 📜 License

MIT License

---

## ❤️ Credits

* Built with Rust 🦀
* Powered by Wayland
* Video decoding via FFmpeg
* Performance boosted with Rayon

---

## 🔥 Final Words

> This project is not just a wallpaper engine —
> it's a **performance experiment**, a **system-level tool**, and a **fully vibe-coded AI-assisted creation**.

---

💡 *If you care about performance + aesthetics — this is for you.*

---

