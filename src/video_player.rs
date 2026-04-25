use std::process::{Command, Stdio};
use std::io::Read;
use wayland_client::Main;
use wayland_client::protocol::wl_surface::WlSurface;
use crate::wayland_state::WaylandState;

pub struct VideoPlayer;

impl VideoPlayer {
    pub fn play(
        state: &mut WaylandState,
        surface: Main<WlSurface>,
        file_path: &str,
        width: u32,
        height: u32,
    ) {
        println!("🎬 Playing video: {} ({}x{})", file_path, width, height);

        // Check ffmpeg
        if Command::new("ffmpeg").arg("-version").output().is_err() {
            eprintln!("❌ FFmpeg not found! Install: sudo pacman -S ffmpeg");
            return;
        }
        
        let mut ffmpeg = match Command::new("ffmpeg")
            .args([
                "-i", file_path,
                "-vf", &format!("scale={}:{}:force_original_aspect_ratio=decrease,format=bgra", width, height),
                "-f", "rawvideo",
                "-pix_fmt", "bgra",
                "-an",
                "-"
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn() {
                Ok(cmd) => cmd,
                Err(e) => {
                    eprintln!("❌ Failed to start ffmpeg: {}", e);
                    return;
                }
            };
        
        let mut output = ffmpeg.stdout.take().expect("Failed to capture stdout");
        let frame_size = (width * height * 4) as usize;
        let mut buffer = vec![0u8; frame_size];
        let mut frame_count = 0;
        
        // Pre-create a buffer to reuse (avoid leak)
        let (mut shm_data, wl_buffer) = state.create_buffer(width, height);
        
        loop {
            // Read one frame
            let mut bytes_read = 0;
            while bytes_read < frame_size {
                match output.read(&mut buffer[bytes_read..frame_size]) {
                    Ok(0) => break,
                    Ok(n) => bytes_read += n,
                    Err(_) => break,
                }
            }
            
            if bytes_read == 0 {
                println!("✅ Video finished: {} frames", frame_count);
                break;
            }
            
            frame_count += 1;
            
            // Copy frame data to shm buffer
            shm_data.copy_from_slice(&buffer);
            
            // Re-attach same buffer (no new allocation)
            surface.attach(Some(&wl_buffer), 0, 0);
            surface.damage(0, 0, i32::MAX, i32::MAX);
            surface.commit();
            state.dispatch();
            
            // ~60 FPS
            std::thread::sleep(std::time::Duration::from_millis(16));
            
            if frame_count % 60 == 0 {
                print!("\r🎬 Frame: {}", frame_count);
                use std::io::Write;
                std::io::stdout().flush().unwrap();
            }
        }
        
        let _ = ffmpeg.kill();
        println!("\n✅ Video playback done. Last frame remains.");
    }
}
