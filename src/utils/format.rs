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

/// Replace home directory prefix with ~ for display purposes.
///
/// # Examples
/// - "/Users/foo/projects/rust" -> "~/projects/rust"
/// - "/etc/config" -> "/etc/config" (non-home path unchanged)
/// - "/" -> "/" (root unchanged)
pub fn replace_home_with_tilde(path: &str) -> String {
    if let Some(home) = dirs::home_dir() {
        let home_str = home.to_string_lossy();
        if path.starts_with(home_str.as_ref()) {
            let remainder = &path[home_str.len()..];
            if remainder.is_empty() {
                return "~".to_string();
            } else if remainder.starts_with('/') {
                return format!("~{}", remainder);
            }
        }
    }
    path.to_string()
}

/// Compress a single path segment to fit display constraints.
/// Keeps first 3 characters and adds "..." if segment is longer than 6 chars.
/// Uses chars() to handle multi-byte UTF-8 safely.
fn compress_segment(segment: &str) -> String {
    let char_count = segment.chars().count();
    if char_count <= 6 {
        segment.to_string()
    } else {
        let prefix: String = segment.chars().take(3).collect();
        format!("{}...", prefix)
    }
}

/// Format a path for display within a given width constraint.
/// Combines home directory replacement with smart compression.
///
/// # Algorithm
/// 1. Replace home directory with ~
/// 2. If path fits within max_width, return as-is
/// 3. Otherwise, compress middle segments (keeping first 3 chars + "...")
/// 4. Always preserve last segment (current directory name)
///
/// # Examples
/// - "~/projects/rust/file-finder-rust/src/render" with width 35
/// - Result: "~/pro.../rus.../file.../src/render"
pub fn format_path_for_display(path: &str, max_width: usize) -> String {
    // Step 1: Replace home with tilde
    let normalized = replace_home_with_tilde(path);

    // Step 2: If it fits, return as-is
    if normalized.len() <= max_width {
        return normalized;
    }

    // Step 3: Handle very small widths
    if max_width < 5 {
        return "...".chars().take(max_width).collect();
    }

    // Step 4: Parse path components
    let is_home_path = normalized.starts_with('~');
    let is_absolute = normalized.starts_with('/');

    let path_without_prefix = if is_home_path {
        &normalized[1..] // Remove ~
    } else {
        &normalized
    };

    let segments: Vec<&str> = path_without_prefix
        .split('/')
        .filter(|s| !s.is_empty())
        .collect();

    if segments.is_empty() {
        return normalized;
    }

    if segments.len() == 1 {
        // Single segment - truncate with ellipsis if needed
        return truncate_end(&normalized, max_width);
    }

    // Step 5: Calculate space budget
    let prefix_len = if is_home_path {
        2 // "~/"
    } else if is_absolute {
        1 // "/"
    } else {
        0
    };

    let last_segment = segments.last().unwrap();
    let last_segment_space = last_segment.len() + 1; // "/" + segment

    // If even last segment doesn't fit with prefix
    if prefix_len + last_segment_space > max_width {
        return truncate_end(&normalized, max_width);
    }

    // Step 6: Build result working backwards from second-to-last segment
    let available = max_width - prefix_len - last_segment.len();
    let middle_segments = &segments[..segments.len() - 1];

    let mut result_parts: Vec<String> = Vec::new();
    let mut used_width = 0;

    // Process segments from end to beginning (right to left)
    for segment in middle_segments.iter().rev() {
        let full_len = segment.len() + 1; // "/" + segment

        if used_width + full_len <= available {
            // Segment fits fully
            result_parts.insert(0, segment.to_string());
            used_width += full_len;
        } else {
            // Try compressed version
            let compressed = compress_segment(segment);
            let compressed_len = compressed.len() + 1;

            if used_width + compressed_len <= available {
                result_parts.insert(0, compressed);
                used_width += compressed_len;
            } else if used_width + 4 <= available {
                // Can't fit even compressed, add "..." and stop
                result_parts.insert(0, "...".to_string());
                break;
            } else {
                // Can't fit anything more
                break;
            }
        }
    }

    // Step 7: Reconstruct path
    let prefix = if is_home_path {
        "~"
    } else if is_absolute {
        ""
    } else {
        ""
    };
    let sep = if is_home_path || is_absolute { "/" } else { "" };

    if result_parts.is_empty() {
        format!("{}{}{}", prefix, sep, last_segment)
    } else {
        format!("{}{}{}/{}", prefix, sep, result_parts.join("/"), last_segment)
    }
}

/// Truncate a string at the end with ellipsis.
/// Uses chars() to handle multi-byte UTF-8 safely.
fn truncate_end(s: &str, max_width: usize) -> String {
    let char_count = s.chars().count();
    if char_count <= max_width {
        s.to_string()
    } else if max_width <= 3 {
        "...".chars().take(max_width).collect()
    } else {
        let prefix: String = s.chars().take(max_width - 3).collect();
        format!("{}...", prefix)
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

    // Path formatting tests

    #[test]
    fn test_replace_home_with_tilde() {
        // Test non-home paths (always work regardless of system)
        assert_eq!(replace_home_with_tilde("/etc/config"), "/etc/config");
        assert_eq!(replace_home_with_tilde("/"), "/");
        assert_eq!(replace_home_with_tilde("/usr/local/bin"), "/usr/local/bin");

        // Test home directory replacement (if home dir is available)
        if let Some(home) = dirs::home_dir() {
            let home_str = home.to_string_lossy().to_string();

            // Home directory alone
            assert_eq!(replace_home_with_tilde(&home_str), "~");

            // Home with subpath
            let path = format!("{}/projects/rust", home_str);
            assert_eq!(replace_home_with_tilde(&path), "~/projects/rust");

            // Home prefix but not at path boundary (edge case)
            let path_no_sep = format!("{}other", home_str);
            assert_eq!(replace_home_with_tilde(&path_no_sep), path_no_sep);
        }
    }

    #[test]
    fn test_compress_segment() {
        // Short segments stay unchanged
        assert_eq!(compress_segment("src"), "src");
        assert_eq!(compress_segment("ab"), "ab");
        assert_eq!(compress_segment("abcdef"), "abcdef"); // exactly 6 chars

        // Long segments get compressed
        assert_eq!(compress_segment("projects"), "pro...");
        assert_eq!(compress_segment("file-finder-rust"), "fil...");
        assert_eq!(compress_segment("verylongdirectoryname"), "ver...");
    }

    #[test]
    fn test_format_path_no_compression_needed() {
        // Short paths that fit
        assert_eq!(format_path_for_display("~/docs", 20), "~/docs");
        assert_eq!(format_path_for_display("/etc", 20), "/etc");
        assert_eq!(format_path_for_display("~/a/b/c", 20), "~/a/b/c");
    }

    #[test]
    fn test_format_path_compression() {
        // Test compression with a predictable path (starting with ~)
        let path = "~/projects/rust/file-finder/src/render";
        let result = format_path_for_display(path, 30);

        // Should fit within width
        assert!(result.len() <= 30, "Result '{}' exceeds 30 chars", result);
        // Should preserve last segment
        assert!(result.ends_with("render"), "Result '{}' doesn't end with 'render'", result);
        // Should start with ~/
        assert!(result.starts_with("~/"), "Result '{}' doesn't start with ~/", result);
    }

    #[test]
    fn test_format_path_very_short_width() {
        let path = "~/projects/rust";
        let result = format_path_for_display(path, 3);
        assert!(result.len() <= 3);
        assert_eq!(result, "...");
    }

    #[test]
    fn test_format_path_preserves_last_segment() {
        let path = "~/a/b/c/d/e/important";
        let result = format_path_for_display(path, 25);

        // Last segment should be preserved when possible
        assert!(result.len() <= 25);
        assert!(result.ends_with("important") || result.contains("..."));
    }

    #[test]
    fn test_format_path_root() {
        assert_eq!(format_path_for_display("/", 10), "/");
    }

    #[test]
    fn test_format_path_absolute_non_home() {
        let result = format_path_for_display("/etc/nginx/sites-available", 20);
        assert!(result.starts_with("/"));
        assert!(result.len() <= 20);
    }

    #[test]
    fn test_truncate_end() {
        assert_eq!(truncate_end("hello world", 8), "hello...");
        assert_eq!(truncate_end("hi", 10), "hi");
        assert_eq!(truncate_end("hello", 3), "...");
        assert_eq!(truncate_end("hello", 2), "..");
        assert_eq!(truncate_end("hello", 5), "hello");
    }

    #[test]
    fn test_compress_segment_unicode() {
        // Test with multi-byte UTF-8 characters (should not panic)
        // "日本語フォルダ" = 7 chars, compressed to first 3 + "..."
        assert_eq!(compress_segment("日本語フォルダ"), "日本語...");
        // "café" = 4 chars, no compression needed (<=6)
        assert_eq!(compress_segment("café"), "café");
        // "文件夹名称" = 5 chars, no compression needed (<=6)
        assert_eq!(compress_segment("文件夹名称"), "文件夹名称");
        // "文件夹名称很长" = 7 chars, compressed
        assert_eq!(compress_segment("文件夹名称很长"), "文件夹...");
    }

    #[test]
    fn test_truncate_end_unicode() {
        // Test with multi-byte UTF-8 characters (should not panic)
        // "日本語テストです" = 8 chars, truncate to 6 (3 + "...")
        assert_eq!(truncate_end("日本語テストです", 6), "日本語...");
        // "café" = 4 chars, fits in 10
        assert_eq!(truncate_end("café", 10), "café");
        // "日本語テスト" = 6 chars, fits exactly in 6
        assert_eq!(truncate_end("日本語テスト", 6), "日本語テスト");
    }
}
