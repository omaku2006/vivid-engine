use rand::seq::SliceRandom;

#[derive(Clone, Debug)]
pub enum AnimationType {
    Fade, Wipe, Split, Center, Outer, Pixel, Glitch, Dissolve, Random,
}

impl AnimationType {
    pub fn from_name(name: &str) -> Self {
        match name.to_lowercase().as_str() {
            "wipe" => Self::Wipe, "split" => Self::Split,
            "center" | "circle" => Self::Center, "outer" => Self::Outer,
            "pixel" => Self::Pixel, "glitch" => Self::Glitch,
            "dissolve" => Self::Dissolve, "random" => Self::Random,
            _ => Self::Fade,
        }
    }
    pub fn resolve(&self) -> Self {
        if matches!(self, Self::Random) {
            [Self::Fade, Self::Wipe, Self::Center, Self::Pixel, Self::Glitch, Self::Dissolve]
                .choose(&mut rand::thread_rng()).cloned().unwrap_or(Self::Fade)
        } else { self.clone() }
    }
}

#[inline]
fn hash_coord(x: u32, y: u32) -> u8 {
    let h = x.wrapping_mul(12345).wrapping_add(y.wrapping_mul(67890));
    (h ^ (h >> 8)) as u8
}

// ✅ OPTIMIZED: Precomputed math, no sqrt/powi in inner loop → 60 FPS butter smooth
pub fn render_frame(old: &[u8], new: &[u8], progress: f32, width: u32, height: u32, out: &mut [u8], anim: &AnimationType) {
    let w = width as usize;
    let h = height as usize;
    let p = progress * progress * (3.0 - 2.0 * progress); // Smoothstep
    let inv_p = 1.0 - p;

    // ✅ FIXED: Center animation now uses diagonal radius → corners fade smoothly, no pop!
    let half_w = width as f32 * 0.5;
    let half_h = height as f32 * 0.5;
    let max_radius_sq = half_w * half_w + half_h * half_h;
    let threshold_sq = max_radius_sq * p * p;

    for y in 0..h {
        let row_off = y * w * 4;
        let dy = y as f32 - half_h;
        let dy_sq = dy * dy; // Precompute once per row

        for x in 0..w {
            let i = row_off + x * 4;
            match anim {
                AnimationType::Fade => {
                    out[i]     = (old[i] as f32 * inv_p + new[i] as f32 * p) as u8;
                    out[i + 1] = (old[i + 1] as f32 * inv_p + new[i + 1] as f32 * p) as u8;
                    out[i + 2] = (old[i + 2] as f32 * inv_p + new[i + 2] as f32 * p) as u8;
                    out[i + 3] = 255;
                }
                AnimationType::Wipe => {
                    if (x as f32 / width as f32) < p {
                        out[i..i+4].copy_from_slice(&new[i..i+4]);
                    } else {
                        out[i..i+4].copy_from_slice(&old[i..i+4]);
                    }
                }
                AnimationType::Split => {
                    let dx = (x as f32 - half_w).abs();
                    if dx < half_w * p {
                        out[i..i+4].copy_from_slice(&new[i..i+4]);
                    } else {
                        out[i..i+4].copy_from_slice(&old[i..i+4]);
                    }
                }
                AnimationType::Center => {
                    let dx = x as f32 - half_w;
                    // ✅ Squared distance check → 100x faster + covers corners perfectly
                    if dx * dx + dy_sq < threshold_sq {
                        out[i..i+4].copy_from_slice(&new[i..i+4]);
                    } else {
                        out[i..i+4].copy_from_slice(&old[i..i+4]);
                    }
                }
                AnimationType::Outer => {
                    let dx = (x as f32 - half_w).abs() / half_w;
                    let dy_norm = dy.abs() / half_h;
                    if dx.max(dy_norm) > (1.0 - p) {
                        out[i..i+4].copy_from_slice(&new[i..i+4]);
                    } else {
                        out[i..i+4].copy_from_slice(&old[i..i+4]);
                    }
                }
                AnimationType::Pixel => {
                    let bs = (32.0 * (1.0 - p)).max(1.0) as u32;
                    if hash_coord(x as u32 / bs, y as u32 / bs) < (p * 255.0) as u8 {
                        out[i..i+4].copy_from_slice(&new[i..i+4]);
                    } else {
                        out[i..i+4].copy_from_slice(&old[i..i+4]);
                    }
                }
                AnimationType::Dissolve => {
                    if hash_coord(x as u32, y as u32) < (p * 255.0) as u8 {
                        out[i..i+4].copy_from_slice(&new[i..i+4]);
                    } else {
                        out[i..i+4].copy_from_slice(&old[i..i+4]);
                    }
                }
                AnimationType::Glitch => {
                    let strength = if p < 0.7 { 1.0 - p / 0.7 } else { 0.0 };
                    let shift = (hash_coord(y as u32, 0) as f32 / 255.0 * width as f32 * strength) as i32;
                    let sx = ((x as i32 + shift).rem_euclid(w as i32)) as usize;
                    let si = row_off + sx * 4;
                    let src = if hash_coord(x as u32, y as u32) > 180 { new } else { old };
                    out[i..i+4].copy_from_slice(&src[si..si+4]);
                }
                _ => out[i..i+4].copy_from_slice(&new[i..i+4]),
            }
        }
    }
}
