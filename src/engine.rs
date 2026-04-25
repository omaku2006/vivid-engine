use std::fs::File;
use std::io::Read;
use std::os::unix::io::AsRawFd;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use wayland_client::{
    protocol::{
        wl_buffer, wl_compositor, wl_output, wl_registry, wl_shm, wl_shm_pool, wl_surface,
    },
    Connection, Dispatch, QueueHandle, Proxy,
};
use wayland_protocols_wlr::layer_shell::v1::client::{
    zwlr_layer_shell_v1::{self, ZwlrLayerShellV1},
    zwlr_layer_surface_v1::{self, ZwlrLayerSurfaceV1},
};

use crate::animations::{render_frame, AnimationType};

pub struct AppState {
    pub compositor: Option<wl_compositor::WlCompositor>,
    pub shm: Option<wl_shm::WlShm>,
    pub layer_shell: Option<ZwlrLayerShellV1>,
    pub output: Option<wl_output::WlOutput>,
    pub width: u32,
    pub height: u32,
    pub surface: Option<wl_surface::WlSurface>,
    pub layer_surface: Option<ZwlrLayerSurfaceV1>,
    pub configured: bool,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            compositor: None, shm: None, layer_shell: None, output: None,
            width: 1920, height: 1080, surface: None, layer_surface: None, configured: false,
        }
    }
}

type RegEvent = <wl_registry::WlRegistry as Proxy>::Event;
type OutEvent = <wl_output::WlOutput as Proxy>::Event;
type LayEvent = <ZwlrLayerSurfaceV1 as Proxy>::Event;

impl Dispatch<wl_registry::WlRegistry, ()> for AppState {
    fn event(s: &mut Self, p: &wl_registry::WlRegistry, e: RegEvent, _: &(), _: &Connection, qh: &QueueHandle<Self>) {
        if let RegEvent::Global { name, interface, version } = e {
            match interface.as_str() {
                "wl_compositor" => s.compositor = Some(p.bind(name, version.min(6), qh, ())),
                "wl_shm"        => s.shm = Some(p.bind(name, 1, qh, ())),
                "zwlr_layer_shell_v1" => s.layer_shell = Some(p.bind(name, version.min(4), qh, ())),
                "wl_output"     if s.output.is_none() => s.output = Some(p.bind(name, version.min(4), qh, ())),
                _ => {}
            }
        }
    }
}
impl Dispatch<wl_output::WlOutput, ()> for AppState {
    fn event(s: &mut Self, _: &wl_output::WlOutput, e: OutEvent, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        if let OutEvent::Mode { width, height, .. } = e { s.width = width as u32; s.height = height as u32; }
    }
}
impl Dispatch<ZwlrLayerSurfaceV1, ()> for AppState {
    fn event(s: &mut Self, p: &ZwlrLayerSurfaceV1, e: LayEvent, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        if let LayEvent::Configure { serial, width: w, height: h } = e {
            if w > 0 { s.width = w; } if h > 0 { s.height = h; }
            s.configured = true;
            p.ack_configure(serial);
        }
    }
}

impl Dispatch<wl_compositor::WlCompositor, ()> for AppState { fn event(_: &mut Self, _: &wl_compositor::WlCompositor, _: <wl_compositor::WlCompositor as Proxy>::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {} }
impl Dispatch<wl_shm::WlShm, ()> for AppState { fn event(_: &mut Self, _: &wl_shm::WlShm, _: <wl_shm::WlShm as Proxy>::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {} }
impl Dispatch<wl_shm_pool::WlShmPool, ()> for AppState { fn event(_: &mut Self, _: &wl_shm_pool::WlShmPool, _: <wl_shm_pool::WlShmPool as Proxy>::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {} }
impl Dispatch<wl_surface::WlSurface, ()> for AppState { fn event(_: &mut Self, _: &wl_surface::WlSurface, _: <wl_surface::WlSurface as Proxy>::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {} }
impl Dispatch<wl_buffer::WlBuffer, ()> for AppState { fn event(_: &mut Self, _: &wl_buffer::WlBuffer, _: <wl_buffer::WlBuffer as Proxy>::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {} }
impl Dispatch<ZwlrLayerShellV1, ()> for AppState { fn event(_: &mut Self, _: &ZwlrLayerShellV1, _: <ZwlrLayerShellV1 as Proxy>::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {} }

pub struct WallpaperEngine {
    pub conn: Connection,
    pub eq: wayland_client::EventQueue<AppState>,
    pub state: AppState,
    _shm_file: Option<File>,
    current_frame: Option<Vec<u8>>,
    width: u32,
    height: u32,
    pub video_buffer: Arc<Mutex<Option<Vec<u8>>>>,
    video_thread: Option<thread::JoinHandle<()>>,
    video_shm_buffer: Option<(memmap2::MmapMut, wl_buffer::WlBuffer)>,
    video_child: Option<Child>,
}

impl WallpaperEngine {
    pub fn is_video(path: &str) -> bool {
        let ext = std::path::Path::new(path).extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
        matches!(ext.as_str(), "mp4" | "mkv" | "webm" | "avi" | "mov" | "wmv" | "flv")
    }

    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let conn = Connection::connect_to_env()?;
        let mut eq = conn.new_event_queue::<AppState>();
        let qh = eq.handle();
        let mut state = AppState::new();
        let _reg = conn.display().get_registry(&qh, ());
        eq.roundtrip(&mut state)?; eq.roundtrip(&mut state)?;
        if state.compositor.is_none() || state.shm.is_none() || state.layer_shell.is_none() {
            return Err("❌ Missing Wayland protocols".into());
        }
        println!("🖥️  Connected: {}x{}", state.width, state.height);
        Ok(Self {
            conn, eq, state, _shm_file: None, current_frame: None,
            width: 1920, height: 1080,
            video_buffer: Arc::new(Mutex::new(None)),
            video_thread: None,
            video_shm_buffer: None,
            video_child: None,
        })
    }

    fn capture_current_frame(&mut self) -> Option<Vec<u8>> { self.current_frame.as_ref().map(|f| f.clone()) }

    pub fn setup_surface(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        if self.state.configured { return Ok(()); }
        let qh = self.eq.handle();
        let comp = self.state.compositor.as_ref().unwrap();
        let shell = self.state.layer_shell.as_ref().unwrap();
        let surf = comp.create_surface(&qh, ());
        let layer = shell.get_layer_surface(&surf, self.state.output.as_ref(), zwlr_layer_shell_v1::Layer::Background, "vivid-engine".to_string(), &qh, ());
        layer.set_anchor(zwlr_layer_surface_v1::Anchor::Top | zwlr_layer_surface_v1::Anchor::Bottom | zwlr_layer_surface_v1::Anchor::Left | zwlr_layer_surface_v1::Anchor::Right);
        layer.set_exclusive_zone(-1);
        layer.set_keyboard_interactivity(zwlr_layer_surface_v1::KeyboardInteractivity::None);
        self.state.surface = Some(surf);
        self.state.layer_surface = Some(layer);
        self.state.surface.as_ref().unwrap().commit();
        for _ in 0..10 {
            self.eq.roundtrip(&mut self.state)?;
            if self.state.configured { return Ok(()); }
        }
        Err("⚠️ Configure timeout".into())
    }

    fn create_buffer(&mut self, w: u32, h: u32) -> Result<(memmap2::MmapMut, wl_buffer::WlBuffer), Box<dyn std::error::Error>> {
        let qh = self.eq.handle();
        let shm = self.state.shm.as_ref().unwrap();
        let stride = w * 4; let size = (stride * h) as usize;
        let file = tempfile::tempfile()?; file.set_len(size as u64)?;
        let fd = unsafe { std::os::fd::BorrowedFd::borrow_raw(file.as_raw_fd()) };
        let pool = shm.create_pool(fd, size as i32, &qh, ());
        let buf = pool.create_buffer(0, w as i32, h as i32, stride as i32, wl_shm::Format::Argb8888, &qh, ());
        let mmap = unsafe { memmap2::MmapMut::map_mut(&file)? };
        self._shm_file = Some(file);
        Ok((mmap, buf))
    }

    fn load_image_data(path: &str, w: u32, h: u32) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        let img = image::open(path)?;
        let img = img.resize_to_fill(w, h, image::imageops::FilterType::Lanczos3);
        let rgba = img.to_rgba8();
        let mut bgra = vec![0u8; (w * h * 4) as usize];
        for (i, pixel) in rgba.pixels().enumerate() {
            let idx = i * 4;
            bgra[idx] = pixel[2]; bgra[idx + 1] = pixel[1]; bgra[idx + 2] = pixel[0]; bgra[idx + 3] = 255;
        }
        Ok(bgra)
    }

    fn extract_first_frame(&mut self, path: &str) -> Option<Vec<u8>> {
        let w = self.state.width;
        let h = self.state.height;
        let mut cmd = Command::new("ffmpeg");
        cmd.args([
            "-i", path,
            "-vf", &format!("scale={}:{}:force_original_aspect_ratio=decrease,pad={}:{}:(ow-iw)/2:(oh-ih)/2,format=bgra", w, h, w, h),
            "-vframes", "1",
            "-f", "rawvideo", "-pix_fmt", "bgra", "-an", "-"
        ]).stdout(Stdio::piped()).stderr(Stdio::null());

        let mut child = cmd.spawn().ok()?;
        let mut out = child.stdout.take()?;
        let mut buf = vec![0u8; (w * h * 4) as usize];
        if out.read_exact(&mut buf).is_err() { return None; }
        let _ = child.wait();
        Some(buf)
    }

    pub fn animate_transition(&mut self, new_data: Vec<u8>, anim: AnimationType, duration_seconds: f32) -> Result<(), Box<dyn std::error::Error>> {
        self.setup_surface()?;
        let (w, h) = (self.state.width, self.state.height);
        self.width = w; self.height = h;
        let old_data = self.capture_current_frame().unwrap_or_else(|| vec![0u8; (w * h * 4) as usize]);
        let (mut mmap, buf) = self.create_buffer(w, h)?;
        let anim_resolved = anim.resolve();
        let total_frames = (duration_seconds * 60.0).clamp(3.0, 180.0) as usize;

        for f in 0..=total_frames {
            let t = f as f32 / total_frames as f32;
            render_frame(&old_data, &new_data, t, w, h, &mut mmap, &anim_resolved);
            let surf = self.state.surface.as_ref().unwrap();
            surf.attach(Some(&buf), 0, 0);
            surf.damage_buffer(0, 0, w as i32, h as i32);
            surf.commit();
            self.conn.flush().ok();
            let frame_time = Duration::from_secs_f32(duration_seconds / total_frames as f32);
            thread::sleep(frame_time);
        }
        
        self.current_frame = Some(new_data.clone());
        mmap.copy_from_slice(&new_data);
        let surf = self.state.surface.as_ref().unwrap();
        surf.attach(Some(&buf), 0, 0);
        surf.damage_buffer(0, 0, w as i32, h as i32);
        surf.commit();
        self.conn.flush().ok();
        Ok(())
    }

    pub fn display_with_animation(&mut self, path: &str, anim: AnimationType, duration_seconds: f32) -> Result<(), Box<dyn std::error::Error>> {
        let new_data = Self::load_image_data(path, self.state.width, self.state.height)?;
        self.animate_transition(new_data, anim, duration_seconds)
    }

    pub fn start_video_with_transition(&mut self, path: &str, anim: AnimationType, duration: f32) {
        if let Some(first_frame) = self.extract_first_frame(path) {
            let _ = self.animate_transition(first_frame, anim, duration);
        }
        self.start_video_thread(path);
    }

    pub fn start_video_thread(&mut self, path: &str) {
        let path = path.to_string();
        let buf = self.video_buffer.clone();
        let w = self.state.width;
        let h = self.state.height;

        let mut cmd = Command::new("ffmpeg");
        cmd.args([
            "-stream_loop", "-1", "-re",
            "-i", &path,
            "-vf", &format!("scale={}:{}:force_original_aspect_ratio=decrease,pad={}:{}:(ow-iw)/2:(oh-ih)/2,format=bgra", w, h, w, h),
            "-f", "rawvideo", "-pix_fmt", "bgra", "-an", "-"
        ]).stdout(Stdio::piped()).stderr(Stdio::inherit());

        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => { eprintln!("❌ Failed to start ffmpeg: {}", e); return; }
        };

        let out = child.stdout.take().expect("Failed to capture stdout");
        self.video_child = Some(child);

        self.video_thread = Some(thread::spawn(move || {
            let mut out = out;
            let frame_sz = (w * h * 4) as usize;
            let mut frame = vec![0u8; frame_sz];

            loop {
                if out.read_exact(&mut frame).is_err() { break; }
                *buf.lock().unwrap() = Some(frame.clone());
                // ✅ FIX: Sleep REMOVED! FFmpeg -re handles pacing smoothly natively.
                // Extra sleep was causing micro-jerk at loop points!
            }
        }));
    }

    pub fn render_video_frame(&mut self) {
        // ✅ FIX: Simple, blind render. No VSync deadlock!
        let frame_opt = self.video_buffer.lock().unwrap().take();
        if frame_opt.is_none() { return; }
        
        let frame_data = frame_opt.unwrap();
        if self.video_shm_buffer.is_none() {
            if let Ok(buf) = self.create_buffer(self.state.width, self.state.height) {
                self.video_shm_buffer = Some(buf);
            } else { return; }
        }
        
        if let Some((ref mut mmap, ref buf)) = self.video_shm_buffer {
            mmap.copy_from_slice(&frame_data);
            if let Some(surf) = self.state.surface.as_ref() {
                surf.attach(Some(buf), 0, 0);
                surf.damage_buffer(0, 0, self.state.width as i32, self.state.height as i32);
                surf.commit();
                self.conn.flush().ok();
                let _ = self.eq.dispatch_pending(&mut self.state);
            }
        }
    }

    pub fn handle_ipc_command(&mut self, cmd: crate::ipc::IpcCommand) -> String {
        use crate::ipc::IpcCommand;
        match cmd {
            IpcCommand::SetWallpaper { path, animation, duration } => {
                if !std::path::Path::new(&path).exists() { return "❌ File not found".to_string(); }
                
                if let Some(mut child) = self.video_child.take() {
                    let _ = child.kill();
                    let _ = child.wait();
                    if let Some(handle) = self.video_thread.take() {
                        let _ = handle.join();
                    }
                }
                
                *self.video_buffer.lock().unwrap() = None;
                self.video_shm_buffer = None;

                let anim = AnimationType::from_name(&animation);
                if Self::is_video(&path) {
                    self.start_video_with_transition(&path, anim, duration);
                    crate::config::save(&path, &animation, duration);
                    format!("✅ Video wallpaper started: {}", path)
                } else {
                    match self.display_with_animation(&path, anim, duration) {
                        Ok(_) => { crate::config::save(&path, &animation, duration); format!("✅ Image changed: {}", path) }
                        Err(e) => format!("❌ Error: {}", e),
                    }
                }
            }
            IpcCommand::SetAnimation { name } => {
                let cfg = crate::config::load();
                crate::config::save(cfg.last_wallpaper.as_deref().unwrap_or(""), &name, cfg.duration);
                format!("✅ Animation set: {}", name)
            }
            IpcCommand::SetDuration { seconds } => {
                let cfg = crate::config::load();
                crate::config::save(cfg.last_wallpaper.as_deref().unwrap_or(""), &cfg.animation, seconds);
                format!("✅ Duration set: {:.2}s", seconds)
            }
            IpcCommand::GetStatus => {
                let cfg = crate::config::load();
                format!("📊 Status:\n  Wallpaper: {:?}\n  Animation: {}\n  Duration: {:.2}s", cfg.last_wallpaper, cfg.animation, cfg.duration)
            }
        }
    }

    pub fn dispatch(&mut self) {
        let _ = self.eq.dispatch_pending(&mut self.state);
        let _ = self.conn.flush();
    }
}
