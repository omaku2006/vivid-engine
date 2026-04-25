use clap::Parser;
use std::thread;
use std::time::Duration;
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::os::unix::net::UnixListener;

mod args;
mod animations;
mod config;
mod engine;
mod ipc;

use args::Args;
use animations::AnimationType;
use engine::WallpaperEngine;
use ipc::{IpcCommand, try_connect, start_listener};

fn print_animation_list() {
    println!("🎬 Available Animations:");
    println!("  • fade • wipe • split • center • outer • pixel • dissolve • glitch");
    println!("  • slide_up • slide_down • zoom • blinds • diagonal • wave • random");
    println!("\n⏱️  Duration: -a 0.1 to 3.0 (seconds)");
    println!("💡 Usage:\n   vivid-engine                    # Start daemon\n   vivid-engine /path.jpg        # Change wallpaper\n   vivid-engine -a center -a 1.0 # Set anim + duration");
}

fn main() {
    let args = Args::parse();
    if let Some(arg) = &args.animation {
        if arg.to_lowercase() == "list" {
            print_animation_list();
            return;
        }
    }

    if args.file.is_some() || args.animation.is_some() {
        if try_connect().is_some() {
            let cfg = config::load();
            let cmd = if let Some(path) = &args.file {
                let anim = args.animation.as_ref()
                    .filter(|a| !config::is_duration_str(a))
                    .cloned().unwrap_or(cfg.animation);
                let dur = args.animation.as_ref()
                    .and_then(|a| if config::is_duration_str(a) { a.parse().ok() } else { None })
                    .unwrap_or(cfg.duration);
                IpcCommand::SetWallpaper { path: path.clone(), animation: anim, duration: dur }
            } else if let Some(anim_arg) = &args.animation {
                if config::is_duration_str(anim_arg) {
                    IpcCommand::SetDuration { seconds: anim_arg.parse().unwrap() }
                } else {
                    IpcCommand::SetAnimation { name: anim_arg.clone() }
                }
            } else {
                IpcCommand::GetStatus
            };

            match ipc::send_command(&cmd) {
                Ok(resp) => { println!("{}", resp); return; }
                Err(e) => { eprintln!("❌ IPC error: {}", e); std::process::exit(1); }
            }
        }
    }

    println!("🚀 Starting Vivid Engine daemon...");
    let mut engine = match WallpaperEngine::new() {
        Ok(e) => e,
        Err(e) => { eprintln!("❌ Wayland init failed: {}", e); std::process::exit(1); }
    };

    let cfg = config::load();
    if let Some(path) = &cfg.last_wallpaper {
        if std::path::Path::new(path).exists() {
            let anim = AnimationType::from_name(&cfg.animation);
            
            // ✅ FIX: Video start thayya pachhi transition show thay!
            if WallpaperEngine::is_video(path) {
                engine.start_video_with_transition(path, anim, cfg.duration);
            } else {
                let _ = engine.display_with_animation(path, anim, cfg.duration);
            }
        }
    }

    let listener: Option<UnixListener> = match start_listener() {
        Ok(l) => { println!("🔌 IPC socket ready"); Some(l) }
        Err(e) => { eprintln!("⚠️ IPC socket error: {}", e); None }
    };

    println!("🌟 Daemon running. Press Ctrl+C to exit.");

    loop {
        engine.render_video_frame();

        if let Some(ref listener) = listener {
            match listener.accept() {
                Ok((stream, _)) => {
                    stream.set_nonblocking(false).ok();
                    stream.set_read_timeout(Some(Duration::from_secs(5))).ok();
                    let mut reader = BufReader::new(&stream);
                    let mut req = String::new();
                    if reader.read_line(&mut req).is_ok() && !req.trim().is_empty() {
                        if let Some(cmd) = IpcCommand::parse(&req) {
                            let resp = engine.handle_ipc_command(cmd);
                            let mut writer = BufWriter::new(&stream);
                            let _ = writer.write_all(resp.as_bytes());
                            let _ = writer.write_all(b"\n");
                            let _ = writer.flush();
                        }
                    }
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
                _ => {}
            }
        }

        engine.dispatch();
        thread::sleep(Duration::from_millis(16));
    }
}
