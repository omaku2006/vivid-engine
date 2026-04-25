use clap::Parser;
use std::process::Command;
use std::thread;
use std::time::Duration;

mod args;
use args::Args;

fn main() {
    let args = Args::parse();
    
    if !std::path::Path::new(&args.file).exists() {
        eprintln!("❌ File not found: {}", args.file);
        std::process::exit(1);
    }
    
    println!("🖼️ Setting wallpaper using wbg method...");
    
    // Method 1: Use wbg (if installed)
    let status = Command::new("wbg")
        .arg(&args.file)
        .status();
    
    if status.is_ok() {
        println!("✅ Wallpaper set via wbg");
        
        // Keep running
        loop {
            thread::sleep(Duration::from_secs(3600));
        }
        return;
    }
    
    // Method 2: Use swaybg
    let status = Command::new("swaybg")
        .args(["-i", &args.file, "-m", "fill"])
        .status();
    
    if status.is_ok() {
        println!("✅ Wallpaper set via swaybg");
        
        loop {
            thread::sleep(Duration::from_secs(3600));
        }
        return;
    }
    
    // Method 3: Use hyprpaper (Hyprland)
    let status = Command::new("hyprctl")
        .args(["hyprpaper", "preload", &args.file])
        .status();
    
    if status.is_ok() {
        Command::new("hyprctl")
            .args(["hyprpaper", "wallpaper", &format!("HDMI-1,{}", args.file)])
            .status()
            .ok();
        println!("✅ Wallpaper set via hyprpaper");
        
        loop {
            thread::sleep(Duration::from_secs(3600));
        }
        return;
    }
    
    eprintln!("❌ No wallpaper tool found! Install: wbg, swaybg, or hyprpaper");
}
