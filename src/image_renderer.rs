use wayland_client::Main;
use wayland_client::protocol::wl_surface::WlSurface;
use crate::wayland_state::WaylandState;

pub struct ImageRenderer;

impl ImageRenderer {
    pub fn render(
        state: &mut WaylandState,
        surface: Main<WlSurface>,
        file_path: &str,
        width: u32,
        height: u32,
    ) {
        let (mut data, buffer) = state.create_buffer(width, height);

        let img = image::open(file_path).expect("Failed to load image");
        let img = img.resize_exact(width, height, image::imageops::FilterType::Lanczos3);
        let rgba = img.to_rgba8();

        // Xrgb8888 format: bytes are B,G,R,A but we ignore alpha (set to 0xff)
        for (i, pixel) in rgba.pixels().enumerate() {
            let idx = i * 4;
            data[idx] = pixel[2];     // B
            data[idx + 1] = pixel[1]; // G
            data[idx + 2] = pixel[0]; // R
            data[idx + 3] = 0xff;     // A (ignored by Xrgb)
        }

        state.attach_buffer(&surface, &buffer);
        println!("✅ Image displayed: {}", file_path);
    }
}
