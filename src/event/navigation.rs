use std::path::{Path, PathBuf};

pub fn parent_directory(path: &Path) -> Option<PathBuf> {
    path.parent().map(Path::to_path_buf)
}

pub fn next_index(current: Option<usize>, len: usize) -> Option<usize> {
    if len == 0 {
        None
    } else {
        Some(current.map(|i| (i + 1).min(len - 1)).unwrap_or(0))
    }
}

pub fn previous_index(current: Option<usize>, len: usize) -> Option<usize> {
    if len == 0 {
        None
    } else {
        Some(current.map(|i| i.saturating_sub(1)).unwrap_or(0))
    }
}

pub fn next_index_wrapping(current: Option<usize>, len: usize) -> Option<usize> {
    if len == 0 {
        None
    } else if current == Some(len - 1) {
        Some(0)
    } else {
        next_index(current, len)
    }
}

pub fn previous_index_wrapping(current: Option<usize>, len: usize) -> Option<usize> {
    if len == 0 {
        None
    } else if current == Some(0) {
        Some(len - 1)
    } else {
        previous_index(current, len)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn next_index_returns_none_for_empty_list() {
        assert_eq!(next_index(Some(0), 0), None);
    }

    #[test]
    fn next_index_stops_at_last_item() {
        assert_eq!(next_index(Some(2), 3), Some(2));
    }

    #[test]
    fn previous_index_stops_at_zero() {
        assert_eq!(previous_index(Some(0), 3), Some(0));
    }

    #[test]
    fn parent_directory_returns_parent_path() {
        assert_eq!(
            parent_directory(Path::new("/tmp/example/file.txt")),
            Some(PathBuf::from("/tmp/example"))
        );
    }

    #[test]
    fn next_index_wrapping_wraps_to_start() {
        assert_eq!(next_index_wrapping(Some(2), 3), Some(0));
    }

    #[test]
    fn previous_index_wrapping_wraps_to_end() {
        assert_eq!(previous_index_wrapping(Some(0), 3), Some(2));
    }
}
