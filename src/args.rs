use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "vivid-engine", about = "Lightweight Wayland wallpaper engine", version)]
pub struct Args {
    #[arg(help = "Path to image/video (restores last if omitted)")]
    pub file: Option<String>,

    #[arg(
        short, 
        long,
        value_name = "ANIM|DURATION",
        help = "Set animation: name (fade/wipe/center/etc) OR duration in seconds (0.1-3.0)\n\
                Use '-a list' to see all animations"
    )]
    pub animation: Option<String>,
}
