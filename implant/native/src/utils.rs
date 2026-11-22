//! Utility functions: sleep with jitter, random delays, etc.

use rand::Rng;
use std::time::Duration;
use tracing::debug;

/// Sleep for a duration with random jitter (±20%)
pub fn sleep_with_jitter(duration: Duration) {
    let jitter_factor = rand::thread_rng().gen_range(0.8..=1.2);
    let jittered = duration.mul_f32(jitter_factor);
    debug!("Sleeping {:?} (jittered from {:?})", jittered, duration);
    std::thread::sleep(jittered);
}

/// Generate a random delay between min and max seconds
pub fn random_delay(min_secs: u64, max_secs: u64) -> Duration {
    let secs = rand::thread_rng().gen_range(min_secs..=max_secs);
    Duration::from_secs(secs)
}

/// Sleep with random delay
pub fn sleep_random(min_secs: u64, max_secs: u64) {
    let delay = random_delay(min_secs, max_secs);
    debug!("Random sleep: {:?}", delay);
    std::thread::sleep(delay);
}

/// Get current timestamp as Unix epoch seconds
pub fn now_epoch() -> u64 {
    chrono::Utc::now().timestamp() as u64
}

/// Format a duration for logging
pub fn format_duration(d: Duration) -> String {
    let secs = d.as_secs();
    if secs >= 3600 {
        format!("{:.1}h", secs as f64 / 3600.0)
    } else if secs >= 60 {
        format!("{:.1}m", secs as f64 / 60.0)
    } else {
        format!("{}s", secs)
    }
}

/// Read a file's contents as bytes
pub fn read_file_bytes(path: &str) -> Result<Vec<u8>, std::io::Error> {
    std::fs::read(path)
}

/// Write bytes to a file
pub fn write_file_bytes(path: &str, data: &[u8]) -> Result<(), std::io::Error> {
    std::fs::write(path, data)
}

/// Check if a file exists
pub fn file_exists(path: &str) -> bool {
    std::path::Path::new(path).exists()
}

/// Get file metadata
pub fn get_file_metadata(path: &str) -> Option<FileMetadata> {
    let metadata = std::fs::metadata(path).ok()?;
    Some(FileMetadata {
        name: path.to_string(),
        size: metadata.len(),
        modified: metadata.modified().ok().map(|t| t.duration_since(std::time::UNIX_EPOCH).ok()?.as_secs()),
        is_dir: metadata.is_dir(),
        is_file: metadata.is_file(),
    })
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FileMetadata {
    pub name: String,
    pub size: u64,
    pub modified: Option<u64>,
    pub is_dir: bool,
    pub is_file: bool,
}
