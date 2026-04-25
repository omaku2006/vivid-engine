use wayland_client::{
    protocol::{wl_buffer, wl_compositor, wl_shm, wl_surface::WlSurface},
    Display, EventQueue, GlobalManager, Main,
};
use wayland_protocols::wlr::unstable::layer_shell::v1::client::{
    zwlr_layer_shell_v1::{ZwlrLayerShellV1, Layer},
    zwlr_layer_surface_v1::{Anchor, KeyboardInteractivity, ZwlrLayerSurfaceV1},
};
use std::fs::File;
use std::os::unix::io::AsRawFd;

// Wayland ne batavava mate ek empty struct banavi che
struct LayerSurfaceHandler;

impl wayland_client::Dispatch<ZwlrLayerSurfaceV1, ()> for LayerSurfaceHandler {
    fn event(
        _state: &mut Self,
        _proxy: &ZwlrLayerSurfaceV1,
        event: <ZwlrLayerSurfaceV1 as wayland_client::Protocol>::Event,
        _data: &(),
        _conn: &wayland_client::Connection,
        _qhandle: &wayland_client::QueueHandle<Self>,
    ) {
        if let wayland_protocols::wlr::unstable::layer_shell::v1::client::zwlr_layer_surface_v1::Event::Configure { serial, .. } = event {
            _proxy.ack_configure(serial);
        }
    }
}

pub struct WaylandState {
    _display: Display,
    event_queue: EventQueue,
    compositor: Main<wl_compositor::WlCompositor>,
    shm: Main<wl_shm::WlShm>,
    shell: Main<ZwlrLayerShellV1>,
    pub width: u32,
    pub height: u32,
    // FIX: Aa cheeze zinda rakhvi pade, nahi to compositor memory access nahi kari sake!
    _shm_pool: Option<Main<wl_shm::WlShmPool>>,
    _shm_file: Option<File>,
    _layer_surface: Option<Main<ZwlrLayerSurfaceV1>>,
}

impl WaylandState {
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let display = Display::connect_to_env()?;
        let mut event_queue = display.create_event_queue();
        let attached_display = display.attach(event_queue.token());
        let globals = GlobalManager::new(&attached_display);
        
        // Two roundtrips to get all globals
        event_queue.sync_roundtrip(&mut (), |_, _, _| {})?;
        event_queue.sync_roundtrip(&mut (), |_, _, _| {})?;

        let compositor = globals.instantiate_exact::<wl_compositor::WlCompositor>(4)
            .expect("Compositor not found");
        let shm = globals.instantiate_exact::<wl_shm::WlShm>(1)
            .expect("Shm not found");
        let shell = globals.instantiate_exact::<ZwlrLayerShellV1>(2)
            .expect("Layer shell version 2 not found");

        Ok(Self {
            _display: display,
            event_queue,
            compositor,
            shm,
            shell,
            width: 1920,
            height: 1080,
            _shm_pool: None,
            _shm_file: None,
            _layer_surface: None,
        })
    }

    pub fn set_resolution(&mut self, width: u32, height: u32) {
        self.width = width;
        self.height = height;
    }

    pub fn create_surface(&mut self, _output_name: &str, width: u32, height: u32) -> Main<WlSurface> {
        let surface = self.compositor.create_surface();
        
        let layer_surface = self.shell.get_layer_surface(
            &surface,
            None, // let compositor choose output
            Layer::Background,
            "vivid-engine".to_string(),
        );
        
        layer_surface.set_anchor(Anchor::Top | Anchor::Bottom | Anchor::Left | Anchor::Right);
        layer_surface.set_exclusive_zone(-1);  // -1 for background
        layer_surface.set_keyboard_interactivity(KeyboardInteractivity::None);
        layer_surface.set_size(width, height);
        
        // FIX 1: v0.29 ma assign_handler no use thai che configure handle karva!
        layer_surface.assign_handler(LayerSurfaceHandler);

        // Layer surface ne store kari rakhvo pade
        self._layer_surface = Some(layer_surface);

        // Commit the surface
        surface.commit();
        
        // Compositor ne events process karva de ane configure no wait kar!
        self.event_queue.sync_roundtrip(&mut (), |_, _,_| {}).unwrap();
        self.event_queue.sync_roundtrip(&mut (), |_, _,_| {}).unwrap();
        
        surface
    }

    pub fn create_buffer(&mut self, width: u32, height: u32) -> (memmap2::MmapMut, wl_buffer::WlBuffer) {
        let stride = width * 4;
        let size = (stride * height) as usize;
        
        let file = tempfile::tempfile().expect("Failed to create shm file");
        file.set_len(size as u64).unwrap();
        let mmap = unsafe { memmap2::MmapMut::map_mut(&file).expect("Failed to mmap") };
        
        let pool = self.shm.create_pool(file.as_raw_fd(), size as i32);
        let buffer = pool.create_buffer(
            0,
            width as i32,
            height as i32,
            stride as i32,
            wl_shm::Format::Xrgb8888, // Use Xrgb for simplicity
        ).detach();
        
        // FIX 2: File & Pool ne drop nathi karva dvta, struct ma store kari rakhie chie
        self._shm_pool = Some(pool);
        self._shm_file = Some(file);
        
        (mmap, buffer)
    }

    pub fn attach_buffer(&mut self, surface: &WlSurface, buffer: &wl_buffer::WlBuffer) {
        surface.attach(Some(buffer), 0, 0);
        surface.damage(0, 0, i32::MAX, i32::MAX);
        surface.commit();
        self.dispatch();
    }

    pub fn dispatch(&mut self) {
        // v0.29 ma flush separately nathi, e dispatch andar j thai jaay
        let _ = self.event_queue.dispatch_pending(&mut (), |_, _, _| {});
        let _ = self._display.flush();
    }
}
