use std::fs::File;
use std::io::Read;
use std::os::unix::io::AsRawFd;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{sync_channel, Receiver, SyncSender};
use std::thread;
use std::time::Duration;

use wayland_client::{
    protocol::{wl_buffer, wl_compositor, wl_output, wl_registry, wl_shm, wl_shm_pool, wl_surface},
    Connection, Dispatch, Proxy, QueueHandle,
};
use wayland_protocols_wlr::layer_shell::v1::client::{
    zwlr_layer_shell_v1::{self, ZwlrLayerShellV1},
    zwlr_layer_surface_v1::{self, ZwlrLayerSurfaceV1},
};

use crate::animations::{render_frame, AnimationType};

// =============================================================================
// Output & Surface structs
// =============================================================================

pub struct OutputInfo {
    pub output: wl_output::WlOutput,
    pub width: u32,
    pub height: u32,
}

pub struct SurfaceInfo {
    pub surface: wl_surface::WlSurface,
    pub layer_surface: ZwlrLayerSurfaceV1,
    pub width: u32,
    pub height: u32,
    pub configured: bool,
    pub shm_buffer: Option<(memmap2::MmapMut, wl_buffer::WlBuffer)>,
    pub current_frame: Option<Vec<u8>>,
}

pub struct AppState {
    pub compositor: Option<wl_compositor::WlCompositor>,
    pub shm: Option<wl_shm::WlShm>,
    pub layer_shell: Option<ZwlrLayerShellV1>,
    pub outputs: Vec<OutputInfo>,
    pub surfaces: Vec<SurfaceInfo>,
    pub pending_configures: usize,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            compositor: None,
            shm: None,
            layer_shell: None,
            outputs: Vec::new(),
            surfaces: Vec::new(),
            pending_configures: 0,
        }
    }
}

// =============================================================================
// Wayland Dispatch implementations
// =============================================================================

type RegEvent = <wl_registry::WlRegistry as Proxy>::Event;
type OutEvent = <wl_output::WlOutput as Proxy>::Event;
type LayEvent = <ZwlrLayerSurfaceV1 as Proxy>::Event;

impl Dispatch<wl_registry::WlRegistry, ()> for AppState {
    fn event(
        state: &mut Self,
        registry: &wl_registry::WlRegistry,
        event: RegEvent,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let RegEvent::Global {
            name,
            interface,
            version,
        } = event
        {
            match interface.as_str() {
                "wl_compositor" => {
                    state.compositor = Some(registry.bind(name, version.min(6), qh, ()));
                }
                "wl_shm" => {
                    state.shm = Some(registry.bind(name, 1, qh, ()));
                }
                "zwlr_layer_shell_v1" => {
                    state.layer_shell = Some(registry.bind(name, version.min(4), qh, ()));
                }
                "wl_output" => {
                    let idx = state.outputs.len();
                    let output = registry.bind(name, version.min(4), qh, idx);
                    state.outputs.push(OutputInfo {
                        output,
                        width: 1920,
                        height: 1080,
                    });
                }
                _ => {}
            }
        }
    }
}

impl Dispatch<wl_output::WlOutput, usize> for AppState {
    fn event(
        state: &mut Self,
        _: &wl_output::WlOutput,
        event: OutEvent,
        idx: &usize,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let OutEvent::Mode { width, height, .. } = event {
            if let Some(info) = state.outputs.get_mut(*idx) {
                info.width = width as u32;
                info.height = height as u32;
            }
        }
    }
}

impl Dispatch<ZwlrLayerSurfaceV1, usize> for AppState {
    fn event(
        state: &mut Self,
        proxy: &ZwlrLayerSurfaceV1,
        event: LayEvent,
        idx: &usize,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let LayEvent::Configure {
            serial,
            width: w,
            height: h,
        } = event
        {
            if let Some(surf) = state.surfaces.get_mut(*idx) {
                if w > 0 {
                    surf.width = w as u32;
                }
                if h > 0 {
                    surf.height = h as u32;
                }
                surf.configured = true;
            }
            state.pending_configures = state.pending_configures.saturating_sub(1);
            proxy.ack_configure(serial);
        }
    }
}

macro_rules! stub_dispatch {
    ($type:ty) => {
        impl Dispatch<$type, ()> for AppState {
            fn event(
                _: &mut Self,
                _: &$type,
                _: <$type as Proxy>::Event,
                _: &(),
                _: &Connection,
                _: &QueueHandle<Self>,
            ) {
            }
        }
    };
}

stub_dispatch!(wl_compositor::WlCompositor);
stub_dispatch!(wl_shm::WlShm);
stub_dispatch!(wl_shm_pool::WlShmPool);
stub_dispatch!(wl_surface::WlSurface);
stub_dispatch!(wl_buffer::WlBuffer);
stub_dispatch!(ZwlrLayerShellV1);

// =============================================================================
// WallpaperEngine
// =============================================================================

pub struct WallpaperEngine {
    pub conn: Connection,
    pub eq: wayland_client::EventQueue<AppState>,
    pub state: AppState,
    _shm_files: Vec<File>,

    // Video pipeline
    video_width: u32,
    video_height: u32,
    video_rx: Option<Receiver<Vec<u8>>>,
    video_return_tx: Option<SyncSender<Vec<u8>>>,
    video_child: Option<Child>,
    video_thread: Option<thread::JoinHandle<()>>,
    // Reusable scratch buffer for resizing frames (zero-allocation path)
    video_resize_buf: Vec<u8>,
}

impl WallpaperEngine {
    pub fn is_video(path: &str) -> bool {
        let ext = std::path::Path::new(path)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();
        matches!(
            ext.as_str(),
            "mp4" | "mkv" | "webm" | "avi" | "mov" | "wmv" | "flv"
        )
    }

    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let conn = Connection::connect_to_env()?;
        let mut eq = conn.new_event_queue::<AppState>();
        let qh = eq.handle();
        let mut state = AppState::new();
        let _reg = conn.display().get_registry(&qh, ());
        eq.roundtrip(&mut state)?;
        eq.roundtrip(&mut state)?;

        if state.compositor.is_none() || state.shm.is_none() || state.layer_shell.is_none() {
            return Err("❌ Missing Wayland protocols".into());
        }

        for _ in 0..10 {
            eq.roundtrip(&mut state)?;
            if !state.outputs.is_empty() {
                break;
            }
        }

        println!("🖥️  Found {} output(s)", state.outputs.len());

        Ok(Self {
            conn,
            eq,
            state,
            _shm_files: Vec::new(),
            video_width: 1920,
            video_height: 1080,
            video_rx: None,
            video_return_tx: None,
            video_child: None,
            video_thread: None,
            video_resize_buf: Vec::new(),
        })
    }

    pub fn setup_surfaces(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        if !self.state.surfaces.is_empty() {
            return Ok(());
        }
        let qh = self.eq.handle();
        let comp = self.state.compositor.as_ref().unwrap();
        let shell = self.state.layer_shell.as_ref().unwrap();

        self.state.pending_configures = 0;
        for (idx, info) in self.state.outputs.iter().enumerate() {
            let surf = comp.create_surface(&qh, ());
            let layer = shell.get_layer_surface(
                &surf,
                Some(&info.output),
                zwlr_layer_shell_v1::Layer::Background,
                "vivid-engine".to_string(),
                &qh,
                idx,
            );
            layer.set_anchor(
                zwlr_layer_surface_v1::Anchor::Top
                    | zwlr_layer_surface_v1::Anchor::Bottom
                    | zwlr_layer_surface_v1::Anchor::Left
                    | zwlr_layer_surface_v1::Anchor::Right,
            );
            layer.set_exclusive_zone(-1);
            layer.set_keyboard_interactivity(zwlr_layer_surface_v1::KeyboardInteractivity::None);
            surf.commit();

            self.state.surfaces.push(SurfaceInfo {
                surface: surf,
                layer_surface: layer,
                width: info.width,
                height: info.height,
                configured: false,
                shm_buffer: None,
                current_frame: None,
            });
            self.state.pending_configures += 1;
        }

        for _ in 0..50 {
            self.eq.roundtrip(&mut self.state)?;
            if self.state.pending_configures == 0 {
                break;
            }
        }

        if let Some(first) = self.state.surfaces.first() {
            self.video_width = first.width;
            self.video_height = first.height;
        }

        println!(
            "✅ Surfaces ready on {} monitor(s)",
            self.state.surfaces.len()
        );
        Ok(())
    }

    fn create_buffer(
        &mut self,
        w: u32,
        h: u32,
    ) -> Result<(memmap2::MmapMut, wl_buffer::WlBuffer), Box<dyn std::error::Error>> {
        let qh = self.eq.handle();
        let shm = self.state.shm.as_ref().unwrap();
        let stride = w * 4;
        let size = (stride * h) as usize;
        let file = tempfile::tempfile()?;
        file.set_len(size as u64)?;
        let fd = unsafe { std::os::fd::BorrowedFd::borrow_raw(file.as_raw_fd()) };
        let pool = shm.create_pool(fd, size as i32, &qh, ());
        let buf = pool.create_buffer(
            0,
            w as i32,
            h as i32,
            stride as i32,
            wl_shm::Format::Argb8888,
            &qh,
            (),
        );
        let mmap = unsafe { memmap2::MmapMut::map_mut(&file)? };
        self._shm_files.push(file);
        Ok((mmap, buf))
    }

    fn load_image_data(path: &str, w: u32, h: u32) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        let img = image::open(path)?;
        let img = img.resize_to_fill(w, h, image::imageops::FilterType::Lanczos3);
        let rgba = img.to_rgba8();
        let mut bgra = vec![0u8; (w * h * 4) as usize];
        for (i, pixel) in rgba.pixels().enumerate() {
            let idx = i * 4;
            bgra[idx] = pixel[2];
            bgra[idx + 1] = pixel[1];
            bgra[idx + 2] = pixel[0];
            bgra[idx + 3] = 255;
        }
        Ok(bgra)
    }

    fn extract_first_frame(&mut self, path: &str, w: u32, h: u32) -> Option<Vec<u8>> {
        let mut cmd = Command::new("ffmpeg");
        cmd.args([
            "-i", path,
            "-vf",
            &format!(
                "scale={}:{}:force_original_aspect_ratio=decrease,pad={}:{}:(ow-iw)/2:(oh-ih)/2,format=bgra",
                w, h, w, h
            ),
            "-vframes", "1",
            "-f", "rawvideo",
            "-pix_fmt", "bgra",
            "-an",
            "-threads", "2",
            "-",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::null());

        let mut child = cmd.spawn().ok()?;
        let mut out = child.stdout.take()?;
        let mut buf = vec![0u8; (w * h * 4) as usize];
        if out.read_exact(&mut buf).is_err() {
            return None;
        }
        let _ = child.wait();
        Some(buf)
    }

    /// Resize raw BGRA into a pre-allocated destination (zero-allocation hot path)
    fn resize_raw_bgra_into(
        src: &[u8],
        src_w: u32,
        src_h: u32,
        dst_w: u32,
        dst_h: u32,
        dst: &mut [u8],
    ) {
        if src_w == dst_w && src_h == dst_h {
            dst.copy_from_slice(src);
            return;
        }
        let sx_ratio = src_w as f32 / dst_w as f32;
        let sy_ratio = src_h as f32 / dst_h as f32;

        for y in 0..dst_h {
            for x in 0..dst_w {
                let sx = ((x as f32 + 0.5) * sx_ratio - 0.5).round() as i32;
                let sy = ((y as f32 + 0.5) * sy_ratio - 0.5).round() as i32;
                let sx = sx.clamp(0, src_w as i32 - 1) as u32;
                let sy = sy.clamp(0, src_h as i32 - 1) as u32;
                let s_idx = ((sy * src_w + sx) * 4) as usize;
                let d_idx = ((y * dst_w + x) * 4) as usize;
                dst[d_idx..d_idx + 4].copy_from_slice(&src[s_idx..s_idx + 4]);
            }
        }
    }

    fn build_ffmpeg_cmd(path: &str, w: u32, h: u32, fps: u32) -> Command {
        let mut cmd = Command::new("ffmpeg");

        // Auto-detect VAAPI (Intel/AMD iGPU). Decodes on GPU, outputs system RAM for our SHM path.
        let vaapi_dev = "/dev/dri/renderD128";
        if std::path::Path::new(vaapi_dev).exists() {
            cmd.args(["-hwaccel", "vaapi", "-hwaccel_device", vaapi_dev]);
        }

        cmd.args([
            "-stream_loop", "-1",
            "-re",
            "-i", path,
            "-vf",
            &format!(
                "scale={}:{}:force_original_aspect_ratio=decrease,pad={}:{}:(ow-iw)/2:(oh-ih)/2,format=bgra",
                w, h, w, h
            ),
            "-r", &fps.to_string(),
            "-f", "rawvideo",
            "-pix_fmt", "bgra",
            "-an",
            "-threads", "2", // Cap software-decode threads
            "-",
        ]);

        cmd
    }

    pub fn animate_transition(
        &mut self,
        new_data: Vec<u8>,
        anim: AnimationType,
        duration_seconds: f32,
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.setup_surfaces()?;

        let anim_resolved = anim.resolve();
        let total_frames = (duration_seconds * 60.0).clamp(3.0, 180.0) as usize;

        let mut per_surface: Vec<(Vec<u8>, Vec<u8>, u32, u32)> =
            Vec::with_capacity(self.state.surfaces.len());
        for surf in &self.state.surfaces {
            let w = surf.width;
            let h = surf.height;
            let old = surf
                .current_frame
                .as_ref()
                .map(|f| {
                    let mut v = vec![0u8; (w * h * 4) as usize];
                    Self::resize_raw_bgra_into(
                        f,
                        self.video_width,
                        self.video_height,
                        w,
                        h,
                        &mut v,
                    );
                    v
                })
                .unwrap_or_else(|| vec![0u8; (w * h * 4) as usize]);
            let mut new = vec![0u8; (w * h * 4) as usize];
            Self::resize_raw_bgra_into(
                &new_data,
                self.video_width,
                self.video_height,
                w,
                h,
                &mut new,
            );
            per_surface.push((old, new, w, h));
        }

        let dims: Vec<(usize, u32, u32)> = self
            .state
            .surfaces
            .iter()
            .enumerate()
            .filter(|(_, s)| s.shm_buffer.is_none())
            .map(|(i, s)| (i, s.width, s.height))
            .collect();
        for (idx, w, h) in dims {
            if let Ok(b) = self.create_buffer(w, h) {
                self.state.surfaces[idx].shm_buffer = Some(b);
            }
        }

        for f in 0..=total_frames {
            let t = f as f32 / total_frames as f32;

            for (idx, surf) in self.state.surfaces.iter_mut().enumerate() {
                let (ref old, ref new, w, h) = per_surface[idx];
                let Some((ref mut mmap, ref buf)) = surf.shm_buffer else {
                    continue;
                };

                render_frame(old, new, t, w, h, mmap, &anim_resolved);

                surf.surface.attach(Some(buf), 0, 0);
                surf.surface.damage_buffer(0, 0, w as i32, h as i32);
                surf.surface.commit();
            }

            self.conn.flush().ok();
            thread::sleep(Duration::from_secs_f32(
                duration_seconds / total_frames as f32,
            ));
        }

        for (idx, surf) in self.state.surfaces.iter_mut().enumerate() {
            let (_, ref new, w, h) = per_surface[idx];
            if let Some((ref mut mmap, ref buf)) = surf.shm_buffer {
                mmap.copy_from_slice(new);
                surf.surface.attach(Some(buf), 0, 0);
                surf.surface.damage_buffer(0, 0, w as i32, h as i32);
                surf.surface.commit();
            }
            surf.current_frame = Some(new.clone());
        }
        self.conn.flush().ok();

        if let Some(first) = self.state.surfaces.first() {
            self.video_width = first.width;
            self.video_height = first.height;
        }

        Ok(())
    }

    pub fn display_with_animation(
        &mut self,
        path: &str,
        anim: AnimationType,
        duration_seconds: f32,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let (pw, ph) = if let Some(s) = self.state.surfaces.first() {
            (s.width, s.height)
        } else {
            (1920, 1080)
        };
        let new_data = Self::load_image_data(path, pw, ph)?;
        self.animate_transition(new_data, anim, duration_seconds)
    }

    pub fn start_video_with_transition(&mut self, path: &str, anim: AnimationType, duration: f32) {
        let (pw, ph) = if let Some(s) = self.state.surfaces.first() {
            (s.width, s.height)
        } else {
            (1920, 1080)
        };
        self.video_width = pw;
        self.video_height = ph;

        if let Some(first_frame) = self.extract_first_frame(path, pw, ph) {
            let _ = self.animate_transition(first_frame, anim, duration);
        }

        // 🗑️ CRITICAL: don't keep the first frame in RAM — video streams fresh frames
        for surf in &mut self.state.surfaces {
            surf.current_frame = None;
        }

        self.start_video_thread(path, pw, ph);
    }

    pub fn start_video_thread(&mut self, path: &str, w: u32, h: u32) {
        self.kill_video();

        let path = path.to_string();
        let (target_w, target_h, fps) = if is_on_battery() {
            let tw = ((w / 2) | 1).max(640);
            let th = ((h / 2) | 1).max(360);
            (tw, th, 24)
        } else {
            (w, h, 60)
        };

        self.video_width = target_w;
        self.video_height = target_h;
        let frame_sz = (target_w * target_h * 4) as usize;

        let (frame_tx, frame_rx) = sync_channel::<Vec<u8>>(1);
        let (return_tx, return_rx) = sync_channel::<Vec<u8>>(2);

        for _ in 0..2 {
            let _ = return_tx.send(vec![0u8; frame_sz]);
        }

        self.video_rx = Some(frame_rx);
        self.video_return_tx = Some(return_tx.clone());

        let mut cmd = Self::build_ffmpeg_cmd(&path, target_w, target_h, fps);
        cmd.stdout(Stdio::piped()).stderr(Stdio::null());

        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => {
                eprintln!("❌ Failed to start ffmpeg: {}", e);
                return;
            }
        };

        let out = child.stdout.take().expect("Failed to capture stdout");
        self.video_child = Some(child);

        self.video_thread = Some(std::thread::spawn(move || {
            let mut out = out;
            loop {
                let mut frame = match return_rx.recv() {
                    Ok(b) => b,
                    Err(_) => break,
                };
                if out.read_exact(&mut frame).is_err() {
                    break;
                }
                if frame_tx.send(frame).is_err() {
                    break;
                }
            }
        }));
    }

    pub fn render_video_frame(&mut self) {
        let frame_data = match self.video_rx.as_ref().and_then(|rx| rx.try_recv().ok()) {
            Some(f) => f,
            None => return,
        };

        let missing: Vec<(usize, u32, u32)> = self
            .state
            .surfaces
            .iter()
            .enumerate()
            .filter(|(_, s)| s.shm_buffer.is_none())
            .map(|(i, s)| (i, s.width, s.height))
            .collect();
        for (idx, w, h) in missing {
            if let Ok(b) = self.create_buffer(w, h) {
                self.state.surfaces[idx].shm_buffer = Some(b);
            }
        }

        // Take scratch buffer out of self to avoid borrow issues in the loop
        let mut scratch = std::mem::take(&mut self.video_resize_buf);

        for idx in 0..self.state.surfaces.len() {
            let surf = &mut self.state.surfaces[idx];
            let Some((ref mut mmap, ref buf)) = surf.shm_buffer else {
                continue;
            };

            if surf.width == self.video_width && surf.height == self.video_height {
                mmap.copy_from_slice(&frame_data);
            } else {
                let needed = (surf.width * surf.height * 4) as usize;
                if scratch.len() != needed {
                    scratch.resize(needed, 0);
                }
                Self::resize_raw_bgra_into(
                    &frame_data,
                    self.video_width,
                    self.video_height,
                    surf.width,
                    surf.height,
                    &mut scratch,
                );
                let len = mmap.len().min(scratch.len());
                mmap[..len].copy_from_slice(&scratch[..len]);
            }

            surf.surface.attach(Some(buf), 0, 0);
            surf.surface
                .damage_buffer(0, 0, surf.width as i32, surf.height as i32);
            surf.surface.commit();
        }

        self.conn.flush().ok();
        let _ = self.eq.dispatch_pending(&mut self.state);

        // Put scratch back for next frame
        self.video_resize_buf = scratch;

        if let Some(tx) = &self.video_return_tx {
            let _ = tx.send(frame_data);
        }
    }

    pub fn kill_video(&mut self) {
        self.video_rx = None;
        self.video_return_tx = None;

        if let Some(mut child) = self.video_child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        if let Some(t) = self.video_thread.take() {
            let _ = t.join();
        }
    }

    pub fn handle_ipc_command(&mut self, cmd: crate::ipc::IpcCommand) -> String {
        use crate::ipc::IpcCommand;
        match cmd {
            IpcCommand::SetWallpaper {
                path,
                animation,
                duration,
            } => {
                if !std::path::Path::new(&path).exists() {
                    return "❌ File not found".to_string();
                }

                self.kill_video();

                for surf in &mut self.state.surfaces {
                    surf.current_frame = None;
                    surf.shm_buffer = None;
                }
                self._shm_files.clear();
                self.video_resize_buf.clear();

                let anim = AnimationType::from_name(&animation);
                if Self::is_video(&path) {
                    self.start_video_with_transition(&path, anim, duration);
                    crate::config::save(&path, &animation, duration);
                    format!("✅ Video wallpaper started: {}", path)
                } else {
                    match self.display_with_animation(&path, anim, duration) {
                        Ok(_) => {
                            crate::config::save(&path, &animation, duration);
                            format!("✅ Image changed: {}", path)
                        }
                        Err(e) => format!("❌ Error: {}", e),
                    }
                }
            }
            IpcCommand::SetAnimation { name } => {
                let cfg = crate::config::load();
                crate::config::save(
                    cfg.last_wallpaper.as_deref().unwrap_or(""),
                    &name,
                    cfg.duration,
                );
                format!("✅ Animation set: {}", name)
            }
            IpcCommand::SetDuration { seconds } => {
                let cfg = crate::config::load();
                crate::config::save(
                    cfg.last_wallpaper.as_deref().unwrap_or(""),
                    &cfg.animation,
                    seconds,
                );
                format!("✅ Duration set: {:.2}s", seconds)
            }
            IpcCommand::GetStatus => {
                let cfg = crate::config::load();
                format!(
                    "📊 Status:\n  Wallpaper: {:?}\n  Animation: {}\n  Duration: {:.2}s\n  Monitors: {}",
                    cfg.last_wallpaper,
                    cfg.animation,
                    cfg.duration,
                    self.state.surfaces.len()
                )
            }
        }
    }

    pub fn dispatch(&mut self) {
        let _ = self.eq.dispatch_pending(&mut self.state);
        let _ = self.conn.flush();
    }
}

pub fn is_on_battery() -> bool {
    std::fs::read_to_string("/sys/class/power_supply/BAT0/status")
        .or_else(|_| std::fs::read_to_string("/sys/class/power_supply/BAT1/status"))
        .map(|s| s.trim() == "Discharging")
        .unwrap_or(false)
}
