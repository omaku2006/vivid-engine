use std::fs;
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::os::unix::net::{UnixListener, UnixStream};

pub const SOCKET_PATH: &str = "/tmp/vivid-engine.sock";

#[derive(Debug, Clone)]
pub enum IpcCommand {
    SetWallpaper { path: String, animation: String, duration: f32 },
    SetAnimation { name: String },
    SetDuration { seconds: f32 },
    GetStatus,
}

impl IpcCommand {
    pub fn parse(s: &str) -> Option<Self> {
        let parts: Vec<&str> = s.trim().split('|').collect();
        match parts.first().map(|s| *s) {
            Some("SET_WALLPAPER") if parts.len() >= 4 => Some(IpcCommand::SetWallpaper {
                path: parts[1].to_string(),
                animation: parts[2].to_string(),
                duration: parts[3].parse().ok()?,
            }),
            Some("SET_ANIMATION") if parts.len() >= 2 => Some(IpcCommand::SetAnimation { name: parts[1].to_string() }),
            Some("SET_DURATION") if parts.len() >= 2 => Some(IpcCommand::SetDuration { seconds: parts[1].parse().ok()? }),
            Some("GET_STATUS") => Some(IpcCommand::GetStatus),
            _ => None,
        }
    }
    pub fn serialize(&self) -> String {
        match self {
            IpcCommand::SetWallpaper { path, animation, duration } => format!("SET_WALLPAPER|{}|{}|{:.2}", path, animation, duration),
            IpcCommand::SetAnimation { name } => format!("SET_ANIMATION|{}", name),
            IpcCommand::SetDuration { seconds } => format!("SET_DURATION|{:.2}", seconds),
            IpcCommand::GetStatus => "GET_STATUS".to_string(),
        }
    }
}

pub fn send_command(cmd: &IpcCommand) -> Result<String, Box<dyn std::error::Error>> {
    let stream = UnixStream::connect(SOCKET_PATH)?;
    stream.set_read_timeout(Some(std::time::Duration::from_secs(10)))?;
    stream.set_write_timeout(Some(std::time::Duration::from_secs(3)))?;

    let mut writer = BufWriter::new(&stream);
    writer.write_all(cmd.serialize().as_bytes())?;
    writer.write_all(b"\n")?;
    writer.flush()?;

    let mut reader = BufReader::new(&stream);
    let mut response = String::new();
    reader.read_line(&mut response)?;
    Ok(response.trim().to_string())
}

pub fn start_listener() -> Result<UnixListener, Box<dyn std::error::Error>> {
    let _ = fs::remove_file(SOCKET_PATH);
    let listener = UnixListener::bind(SOCKET_PATH)?;
    listener.set_nonblocking(true).ok(); // ✅ Non-blocking accept
    fs::set_permissions(SOCKET_PATH, std::os::unix::fs::PermissionsExt::from_mode(0o600))?;
    Ok(listener)
}

pub fn try_connect() -> Option<UnixStream> {
    UnixStream::connect(SOCKET_PATH).ok()
}
