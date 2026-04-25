use std::fs;
use std::path::PathBuf;

pub const ANIMATIONS: &[&str] = &[
    "fade", "wipe", "split", "center", "outer", "pixel", "dissolve", "glitch",
    "slide_up", "slide_down", "zoom", "blinds", "diagonal", "wave", "random"
];

pub const DEFAULT_DURATION: f32 = 0.5;
pub const MIN_DURATION: f32 = 0.1;
pub const MAX_DURATION: f32 = 3.0;

#[derive(Clone, Debug)]
pub struct AppConfig {
    pub last_wallpaper: Option<String>,
    pub animation: String,
    pub duration: f32,
}

fn config_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
    PathBuf::from(home).join(".config/vivid-engine")
}

pub fn load() -> AppConfig {
    let path = config_dir().join("state.conf");
    let content = fs::read_to_string(&path).unwrap_or_default();
    
    let mut last = None;
    let mut anim = "fade".to_string();
    let mut duration = DEFAULT_DURATION;

    for line in content.lines() {
        let line = line.trim();
        if line.starts_with('#') || line.is_empty() { continue; }
        
        if let Some(val) = line.strip_prefix("LAST_WALLPAPER=") {
            last = Some(val.trim().to_string());
        } else if let Some(val) = line.strip_prefix("ANIMATION=") {
            anim = val.trim().to_lowercase();
        } else if let Some(val) = line.strip_prefix("DURATION=") {
            if let Ok(d) = val.trim().parse::<f32>() {
                duration = d.clamp(MIN_DURATION, MAX_DURATION);
            }
        }
    }
    
    AppConfig { last_wallpaper: last, animation: anim, duration }
}

pub fn save(wallpaper: &str, animation: &str, duration: f32) {
    let dir = config_dir();
    fs::create_dir_all(&dir).ok();
    
    let content = format!(
        "# Vivid Engine Config\nLAST_WALLPAPER={}\nANIMATION={}\nDURATION={:.2}\n",
        wallpaper, animation.to_lowercase(), duration.clamp(MIN_DURATION, MAX_DURATION)
    );
    fs::write(dir.join("state.conf"), content).ok();
}

pub fn is_duration_str(s: &str) -> bool {
    s.parse::<f32>().is_ok()
}
