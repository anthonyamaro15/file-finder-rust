//! Formatting utilities for file sizes, counts, dates, permissions and paths.

use std::time::{SystemTime, UNIX_EPOCH};

/// Format a count with thousands separators, e.g. 1234567 -> "1,234,567".
pub fn format_thousands(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, digit) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(digit);
    }
    out
}

/// Unix permission bits as `rwxr-xr-x`.
pub fn permission_bits(mode: u32) -> String {
    (0..9)
        .rev()
        .map(|bit| {
            if mode & (1 << bit) != 0 {
                ['x', 'w', 'r'][bit % 3]
            } else {
                '-'
            }
        })
        .collect()
}

const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// Local date and time like "Oct 24, 2026 at 3:41 PM", or `None` when the system
/// can't convert it.
pub fn format_local_time(time: SystemTime) -> Option<String> {
    #[cfg(unix)]
    {
        let local = local_time(time)?;
        let hour = match local.tm_hour % 12 {
            0 => 12,
            hour => hour,
        };
        Some(format!(
            "{} {}, {} at {}:{:02} {}",
            MONTHS.get(usize::try_from(local.tm_mon).ok()?)?,
            local.tm_mday,
            local.tm_year + 1900,
            hour,
            local.tm_min,
            if local.tm_hour < 12 { "AM" } else { "PM" }
        ))
    }
    #[cfg(not(unix))]
    {
        let _ = (time, UNIX_EPOCH);
        None
    }
}

/// `time` broken down in the local time zone, or `None` when the system can't convert it.
#[cfg(unix)]
pub fn local_time(time: SystemTime) -> Option<libc::tm> {
    let seconds = match time.duration_since(UNIX_EPOCH) {
        Ok(after) => libc::time_t::try_from(after.as_secs()).ok()?,
        Err(before) => -libc::time_t::try_from(before.duration().as_secs()).ok()?,
    };
    // SAFETY: zero is valid for tm, and both pointers refer to live values.
    let mut local: libc::tm = unsafe { std::mem::zeroed() };
    if unsafe { libc::localtime_r(&seconds, &mut local) }.is_null() {
        return None;
    }
    Some(local)
}

/// A moment's local calendar date and time, worked out once when an item is read so
/// that drawing never asks the time zone.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LocalStamp {
    pub year: i32,
    /// 0 for January.
    pub month: u8,
    pub day: u8,
    pub hour: u8,
    pub minute: u8,
}

/// `time` in the local time zone, or `None` when the system can't convert it.
pub fn local_stamp(time: SystemTime) -> Option<LocalStamp> {
    #[cfg(unix)]
    {
        let local = local_time(time)?;
        Some(LocalStamp {
            year: local.tm_year + 1900,
            month: u8::try_from(local.tm_mon).ok()?,
            day: u8::try_from(local.tm_mday).ok()?,
            hour: u8::try_from(local.tm_hour).ok()?,
            minute: u8::try_from(local.tm_min).ok()?,
        })
    }
    #[cfg(not(unix))]
    {
        let _ = time;
        None
    }
}

/// A list's label for a modified time, by its calendar day and year against `today`:
/// "09:15" on the same day, "Oct 21" in the same year, else "2024"; blank without one.
pub fn date_label(stamp: Option<LocalStamp>, today: LocalStamp) -> String {
    let Some(stamp) = stamp else {
        return String::new();
    };
    if stamp.year != today.year {
        stamp.year.to_string()
    } else if (stamp.month, stamp.day) != (today.month, today.day) {
        let month = MONTHS.get(usize::from(stamp.month)).unwrap_or(&"");
        format!("{month} {:>2}", stamp.day)
    } else {
        format!("{:02}:{:02}", stamp.hour, stamp.minute)
    }
}

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
    let prefix = if is_home_path { "~" } else { "" };
    let sep = if is_home_path || is_absolute { "/" } else { "" };

    if result_parts.is_empty() {
        format!("{}{}{}", prefix, sep, last_segment)
    } else {
        format!(
            "{}{}{}/{}",
            prefix,
            sep,
            result_parts.join("/"),
            last_segment
        )
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
    fn thousands_separators_group_digits_from_the_right() {
        for (n, expected) in [
            (0, "0"),
            (999, "999"),
            (1_000, "1,000"),
            (56_789, "56,789"),
            (1_234_567, "1,234,567"),
            (u64::MAX, "18,446,744,073,709,551,615"),
        ] {
            assert_eq!(format_thousands(n), expected);
        }
    }

    #[cfg(unix)]
    #[test]
    fn local_time_reads_the_same_in_any_time_zone() {
        // Noon UTC mid-month is the same local month everywhere, far from DST changes.
        for (noon_utc, month) in [(1_768_478_400, "Jan"), (1_784_116_800, "Jul")] {
            let timestamp: libc::time_t = noon_utc;
            // SAFETY: zero is valid for tm, and both pointers refer to live values.
            let mut local: libc::tm = unsafe { std::mem::zeroed() };
            assert!(!unsafe { libc::localtime_r(&timestamp, &mut local) }.is_null());
            let midnight = timestamp
                - libc::time_t::from(local.tm_hour * 3600 + local.tm_min * 60 + local.tm_sec);
            for (offset, time) in [
                (0, "12:00 AM"),
                (9 * 3600 + 5 * 60, "9:05 AM"),
                (12 * 3600, "12:00 PM"),
                (15 * 3600 + 41 * 60, "3:41 PM"),
            ] {
                let at = UNIX_EPOCH + std::time::Duration::from_secs((midnight + offset) as u64);
                assert_eq!(
                    format_local_time(at).unwrap(),
                    format!("{month} {}, 2026 at {time}", local.tm_mday)
                );
            }
        }
    }

    #[test]
    fn date_labels_read_against_today() {
        let at = |year, month, day, hour, minute| {
            Some(LocalStamp {
                year,
                month,
                day,
                hour,
                minute,
            })
        };
        let today = at(2026, 9, 25, 14, 30).unwrap();
        for (stamp, label) in [
            (at(2026, 9, 25, 9, 15), "09:15"),
            (at(2026, 9, 21, 18, 0), "Oct 21"),
            (at(2026, 2, 3, 0, 0), "Mar  3"),
            (at(2024, 9, 25, 9, 15), "2024"),
            // A future date is judged the same way.
            (at(2026, 9, 25, 23, 59), "23:59"),
            (at(2026, 11, 25, 9, 15), "Dec 25"),
            (at(2027, 0, 1, 0, 0), "2027"),
            (None, ""),
        ] {
            assert_eq!(date_label(stamp, today), label, "{stamp:?}");
        }
    }

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
        assert!(
            result.ends_with("render"),
            "Result '{}' doesn't end with 'render'",
            result
        );
        // Should start with ~/
        assert!(
            result.starts_with("~/"),
            "Result '{}' doesn't start with ~/",
            result
        );
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
