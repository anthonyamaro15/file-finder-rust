//! Formatting utilities for file sizes and timestamps.

use std::time::SystemTime;

/// Format a file size in bytes to a human-readable string.
///
/// # Examples
/// - 512 -> "512 B"
/// - 1536 -> "1.5 KB"
/// - 1048576 -> "1.0 MB"
pub fn format_file_size(size: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
    let mut size = size as f64;
    let mut unit_index = 0;

    while size >= 1024.0 && unit_index < UNITS.len() - 1 {
        size /= 1024.0;
        unit_index += 1;
    }

    if unit_index == 0 {
        format!("{} {}", size as u64, UNITS[unit_index])
    } else {
        format!("{:.1} {}", size, UNITS[unit_index])
    }
}

/// Format a system time as a relative time string.
///
/// # Examples
/// - Recent time -> "just now"
/// - 30 minutes ago -> "30m ago"
/// - 5 hours ago -> "5h ago"
/// - 3 days ago -> "3d ago"
pub fn format_system_time(time: SystemTime) -> String {
    use std::time::UNIX_EPOCH;

    match time.duration_since(UNIX_EPOCH) {
        Ok(duration) => {
            let secs = duration.as_secs();
            let days = secs / 86400;
            let hours = (secs % 86400) / 3600;
            let minutes = (secs % 3600) / 60;

            if days > 0 {
                format!("{}d ago", days)
            } else if hours > 0 {
                format!("{}h ago", hours)
            } else if minutes > 0 {
                format!("{}m ago", minutes)
            } else {
                "just now".to_string()
            }
        }
        Err(_) => "unknown".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_file_size_bytes() {
        assert_eq!(format_file_size(0), "0 B");
        assert_eq!(format_file_size(512), "512 B");
        assert_eq!(format_file_size(1023), "1023 B");
    }

    #[test]
    fn test_format_file_size_kilobytes() {
        assert_eq!(format_file_size(1024), "1.0 KB");
        assert_eq!(format_file_size(1536), "1.5 KB");
    }

    #[test]
    fn test_format_file_size_megabytes() {
        assert_eq!(format_file_size(1048576), "1.0 MB");
    }

    #[test]
    fn test_format_file_size_gigabytes() {
        assert_eq!(format_file_size(1073741824), "1.0 GB");
    }
}
