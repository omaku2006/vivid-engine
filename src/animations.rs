use rand::seq::SliceRandom;
use rayon::prelude::*; // 🚀 Parallel processing

#[derive(Clone, Debug)]
pub enum AnimationType {
    Fade, Wipe, Split, Center, Outer, Pixel, Glitch, Dissolve,
    SlideUp, SlideDown, Zoom, Blinds, Diagonal, Wave, Random,
}

impl AnimationType {
    pub fn from_name(name: &str) -> Self {
        match name.to_lowercase().as_str() {
            "wipe" => Self::Wipe, "split" => Self::Split,
            "center" | "circle" => Self::Center, "outer" => Self::Outer,
            "pixel" => Self::Pixel, "glitch" => Self::Glitch,
            "dissolve" => Self::Dissolve, 
            "slide_up" | "slideup" => Self::SlideUp,
            "slide_down" | "slidedown" => Self::SlideDown,
            "zoom" | "ken_burns" => Self::Zoom,
            "blinds" | "venetian" => Self::Blinds,
            "diagonal" => Self::Diagonal,
            "wave" | "ripple" => Self::Wave,
            "random" => Self::Random,
            _ => Self::Fade,
        }
    }
    pub fn resolve(&self) -> Self {
        if matches!(self, Self::Random) {
            [Self::Fade, Self::Wipe, Self::Center, Self::Zoom, Self::Wave, Self::Glitch, Self::Diagonal]
                .choose(&mut rand::thread_rng()).cloned().unwrap_or(Self::Fade)
        } else { self.clone() }
    }
}

#[inline]
fn hash_coord(x: u32, y: u32) -> u8 {
    let h = x.wrapping_mul(12345).wrapping_add(y.wrapping_mul(67890));
    (h ^ (h >> 8)) as u8
}

// ✅ ULTRA-OPTIMIZED: Rayon Par Rows + Smootherstep Math = Butter 60FPS
pub fn render_frame(old: &[u8], new: &[u8], progress: f32, width: u32, height: u32, out: &mut [u8], anim: &AnimationType) {
    let w = width as usize;
    let h = height as usize;
    
    // 🎯 Smootherstep (Smoother than standard smoothstep)
    let p = progress * progress * progress * (progress * (progress * 6.0 - 15.0) + 10.0);
    let inv_p = 1.0 - p;

    // Precompute heavy math outside the loop
    let half_w = width as f32 * 0.5;
    let half_h = height as f32 * 0.5;
    let max_radius_sq = half_w * half_w + half_h * half_h;
    let threshold_sq = max_radius_sq * p * p;
    
    let slide_up_off = (h as f32 * p) as isize;
    let slide_down_off = (h as f32 * (1.0 - p)) as isize;
    
    let zoom_scale = 1.0 + (1.0 - p) * 2.0;
    let cx = w as f32 * 0.5;
    let cy = h as f32 * 0.5;
    
    let num_blinds = 12;
    let blind_h = h as f32 / num_blinds as f32;

    // 🚀 RAYON MAGIC: Process rows in parallel across CPU cores!
    out.par_chunks_mut(w * 4).enumerate().for_each(|(y, row_out)| {
        let row_off = y * w * 4;
        let dy = y as f32 - half_h;
        let dy_sq = dy * dy;
        let y_isize = y as isize;

        for x in 0..w {
            let i = x * 4;
            match anim {
                AnimationType::Fade => {
                    row_out[i]     = (old[row_off + i] as f32 * inv_p + new[row_off + i] as f32 * p) as u8;
                    row_out[i + 1] = (old[row_off + i + 1] as f32 * inv_p + new[row_off + i + 1] as f32 * p) as u8;
                    row_out[i + 2] = (old[row_off + i + 2] as f32 * inv_p + new[row_off + i + 2] as f32 * p) as u8;
                    row_out[i + 3] = 255;
                }
                AnimationType::Wipe => {
                    if (x as f32 / width as f32) < p {
                        row_out[i..i+4].copy_from_slice(&new[row_off + i .. row_off + i + 4]);
                    } else {
                        row_out[i..i+4].copy_from_slice(&old[row_off + i .. row_off + i + 4]);
                    }
                }
                AnimationType::Split => {
                    let dx = (x as f32 - half_w).abs();
                    if dx < half_w * p {
                        row_out[i..i+4].copy_from_slice(&new[row_off + i .. row_off + i + 4]);
                    } else {
                        row_out[i..i+4].copy_from_slice(&old[row_off + i .. row_off + i + 4]);
                    }
                }
                AnimationType::Center => {
                    let dx = x as f32 - half_w;
                    if dx * dx + dy_sq < threshold_sq {
                        row_out[i..i+4].copy_from_slice(&new[row_off + i .. row_off + i + 4]);
                    } else {
                        row_out[i..i+4].copy_from_slice(&old[row_off + i .. row_off + i + 4]);
                    }
                }
                AnimationType::Outer => {
                    let dx = (x as f32 - half_w).abs() / half_w;
                    let dy_norm = dy.abs() / half_h;
                    if dx.max(dy_norm) > (1.0 - p) {
                        row_out[i..i+4].copy_from_slice(&new[row_off + i .. row_off + i + 4]);
                    } else {
                        row_out[i..i+4].copy_from_slice(&old[row_off + i .. row_off + i + 4]);
                    }
                }
                AnimationType::Pixel => {
                    let bs = (32.0 * (1.0 - p)).max(1.0) as u32;
                    if hash_coord(x as u32 / bs, y as u32 / bs) < (p * 255.0) as u8 {
                        row_out[i..i+4].copy_from_slice(&new[row_off + i .. row_off + i + 4]);
                    } else {
                        row_out[i..i+4].copy_from_slice(&old[row_off + i .. row_off + i + 4]);
                    }
                }
                AnimationType::Dissolve => {
                    if hash_coord(x as u32, y as u32) < (p * 255.0) as u8 {
                        row_out[i..i+4].copy_from_slice(&new[row_off + i .. row_off + i + 4]);
                    } else {
                        row_out[i..i+4].copy_from_slice(&old[row_off + i .. row_off + i + 4]);
                    }
                }
                AnimationType::Glitch => {
                    let strength = if p < 0.7 { 1.0 - p / 0.7 } else { 0.0 };
                    let shift = (hash_coord(y as u32, 0) as f32 / 255.0 * width as f32 * strength) as i32;
                    let sx = ((x as i32 + shift).rem_euclid(w as i32)) as usize;
                    let si = row_off + sx * 4;
                    let src = if hash_coord(x as u32, y as u32) > 180 { new } else { old };
                    row_out[i..i+4].copy_from_slice(&src[si..si+4]);
                }
                // 🌟 NEW ANIMATIONS
                AnimationType::SlideUp => {
                    let sy = y_isize - slide_up_off;
                    if sy >= 0 {
                        let si = (sy as usize) * w * 4 + i;
                        row_out[i..i+4].copy_from_slice(&new[si..si+4]);
                    } else {
                        row_out[i..i+4].copy_from_slice(&old[row_off + i .. row_off + i + 4]);
                    }
                }
                AnimationType::SlideDown => {
                    let sy = y_isize + slide_down_off;
                    if sy < h as isize {
                        let si = (sy as usize) * w * 4 + i;
                        row_out[i..i+4].copy_from_slice(&new[si..si+4]);
                    } else {
                        row_out[i..i+4].copy_from_slice(&old[row_off + i .. row_off + i + 4]);
                    }
                }
                AnimationType::Zoom => {
                    let sx = ((x as f32 - cx) / zoom_scale + cx) as usize;
                    let sy = ((y as f32 - cy) / zoom_scale + cy) as usize;
                    if sx < w && sy < h {
                        let si = sy * w * 4 + sx * 4;
                        row_out[i..i+4].copy_from_slice(&new[si..si+4]);
                    } else {
                        row_out[i..i+4].copy_from_slice(&old[row_off + i .. row_off + i + 4]);
                    }
                }
                AnimationType::Blinds => {
                    let blind_idx = (y as f32 / blind_h) as usize;
                    let local_y = (y as f32 - blind_idx as f32 * blind_h) / blind_h;
                    if local_y < p {
                        row_out[i..i+4].copy_from_slice(&new[row_off + i .. row_off + i + 4]);
                    } else {
                        row_out[i..i+4].copy_from_slice(&old[row_off + i .. row_off + i + 4]);
                    }
                }
                AnimationType::Diagonal => {
                    let diag = (x as f32 + y as f32) / (w as f32 + h as f32);
                    if diag < p {
                        row_out[i..i+4].copy_from_slice(&new[row_off + i .. row_off + i + 4]);
                    } else {
                        row_out[i..i+4].copy_from_slice(&old[row_off + i .. row_off + i + 4]);
                    }
                }
                AnimationType::Wave => {
                    let wave_p = (p + 0.1 * ((x as f32 * 0.02 + y as f32 * 0.02 + p * 5.0).sin())).clamp(0.0, 1.0);
                    let inv_wp = 1.0 - wave_p;
                    row_out[i]     = (old[row_off + i] as f32 * inv_wp + new[row_off + i] as f32 * wave_p) as u8;
                    row_out[i + 1] = (old[row_off + i + 1] as f32 * inv_wp + new[row_off + i + 1] as f32 * wave_p) as u8;
                    row_out[i + 2] = (old[row_off + i + 2] as f32 * inv_wp + new[row_off + i + 2] as f32 * wave_p) as u8;
                    row_out[i + 3] = 255;
                }
                _ => row_out[i..i+4].copy_from_slice(&new[row_off + i .. row_off + i + 4]),
            }
        }
    });
}
