//! File utilities for path processing, sorting, and metadata.

use std::cmp::Reverse;
use std::ffi::OsString;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use rayon::prelude::*;

/// Sort order direction.
#[derive(Clone, Debug, PartialEq)]
pub enum SortType {
    ASC,
    DESC,
}

/// Sort criteria for file listings.
#[derive(Clone, Debug, PartialEq)]
pub enum SortBy {
    Name,
    Size,
    DateAdded,
    Default,
}

/// What a listed item is, following a symlink to what it points at.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntryKind {
    File,
    Folder,
    Other,
}

/// A listed item's details, read from disk once with its listing so that drawing never
/// has to read the disk.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EntryInfo {
    /// None when it couldn't be read: it's gone, or a link to nothing.
    pub kind: Option<EntryKind>,
    /// The item itself is a symlink; the other fields describe what it points at.
    pub is_symlink: bool,
    /// Length in bytes; a folder's is that of its own entry, not its contents.
    pub size: u64,
    pub modified: Option<SystemTime>,
    pub created: Option<SystemTime>,
    /// Permission bits.
    pub mode: u32,
}

impl EntryInfo {
    /// Read `path` with one `symlink_metadata`, plus a `metadata` to follow a link.
    pub fn read(path: &Path) -> Self {
        let Ok(metadata) = fs::symlink_metadata(path) else {
            return Self::default();
        };
        let is_symlink = metadata.file_type().is_symlink();
        let metadata = if is_symlink {
            fs::metadata(path)
        } else {
            Ok(metadata)
        };
        let Ok(metadata) = metadata else {
            return Self {
                is_symlink,
                ..Self::default()
            };
        };
        let kind = if metadata.is_file() {
            EntryKind::File
        } else if metadata.is_dir() {
            EntryKind::Folder
        } else {
            EntryKind::Other
        };
        #[cfg(unix)]
        let mode = std::os::unix::fs::PermissionsExt::mode(&metadata.permissions());
        #[cfg(not(unix))]
        let mode = if metadata.permissions().readonly() {
            0o444
        } else {
            0o666
        };
        Self {
            kind: Some(kind),
            is_symlink,
            size: metadata.len(),
            modified: metadata.modified().ok(),
            created: metadata.created().ok(),
            mode,
        }
    }

    pub fn is_dir(&self) -> bool {
        self.kind == Some(EntryKind::Folder)
    }

    pub fn is_file(&self) -> bool {
        self.kind == Some(EntryKind::File)
    }

    /// A file's size; 0 for anything else.
    pub fn file_size(&self) -> u64 {
        if self.is_file() {
            self.size
        } else {
            0
        }
    }
}

/// A folder's items in display order, each beside its details.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Listing {
    pub files: Vec<String>,
    pub info: Vec<EntryInfo>,
}

impl From<Vec<String>> for Listing {
    /// Read the details of paths that didn't come from `read_listing`.
    fn from(files: Vec<String>) -> Self {
        let info = files
            .iter()
            .map(|file| EntryInfo::read(Path::new(file)))
            .collect();
        Self { files, info }
    }
}

/// Sort entries by the specified criteria and direction, using their details.
pub fn sort_entries_by_type(
    sort_by: SortBy,
    sort_type: SortType,
    mut entries: Vec<(PathBuf, EntryInfo)>,
) -> Vec<(PathBuf, EntryInfo)> {
    match sort_type {
        SortType::ASC => match sort_by {
            SortBy::Name => entries.sort_by(|(a, _), (b, _)| {
                a.file_name()
                    .unwrap()
                    .to_ascii_lowercase()
                    .cmp(&b.file_name().unwrap().to_ascii_lowercase())
            }),
            SortBy::Size => entries.sort_by_key(|(_, info)| info.size),
            SortBy::DateAdded => entries.sort_by_key(|(_, info)| date_added_key(info)),
            SortBy::Default => {}
        },
        SortType::DESC => match sort_by {
            SortBy::Name => entries.sort_by(|(a, _), (b, _)| {
                b.file_name()
                    .unwrap()
                    .to_ascii_lowercase()
                    .cmp(&a.file_name().unwrap().to_ascii_lowercase())
            }),
            SortBy::Size => entries.sort_by_key(|(_, info)| Reverse(info.size)),
            SortBy::DateAdded => entries.sort_by_key(|(_, info)| Reverse(date_added_key(info))),
            SortBy::Default => {}
        },
    }

    entries
}

/// Date sort key: creation time, then modification time, then a fixed epoch for an
/// unreadable entry.
fn date_added_key(info: &EntryInfo) -> SystemTime {
    info.created.or(info.modified).unwrap_or(UNIX_EPOCH)
}

/// Read each entry's details, then filter and sort them: one read per entry, in parallel,
/// that sorting, filtering and drawing all use.
pub fn convert_file_path_to_string(
    entries: Vec<PathBuf>,
    show_hidden: bool,
    sort_by: SortBy,
    sort_type: SortType,
) -> Listing {
    // Folders always show, files unless hidden, anything else never.
    let entries: Vec<(PathBuf, EntryInfo)> = entries
        .into_par_iter()
        .map(|path| {
            let info = EntryInfo::read(&path);
            (path, info)
        })
        .filter(|(path, info)| match info.kind {
            Some(EntryKind::Folder) => true,
            Some(EntryKind::File) => {
                show_hidden
                    || path
                        .file_name()
                        .and_then(|name| name.to_str())
                        .is_some_and(|name| !name.starts_with('.'))
            }
            _ => false,
        })
        .collect();

    let (files, info) = sort_entries_by_type(sort_by, sort_type, entries)
        .into_iter()
        .filter_map(|(path, info)| Some((path.into_os_string().into_string().ok()?, info)))
        .unzip();
    Listing { files, info }
}

/// Read a folder's items, filtered and sorted, each beside its details.
pub fn read_listing(
    start_path: String,
    show_hidden: bool,
    sort_by: SortBy,
    sort_type: &SortType,
) -> anyhow::Result<Listing> {
    let entries = fs::read_dir(start_path)?
        .map(|res| res.map(|e| e.path()))
        .collect::<Result<Vec<_>, io::Error>>()?;

    Ok(convert_file_path_to_string(
        entries,
        show_hidden,
        sort_by,
        sort_type.clone(),
    ))
}

/// Get sorted file paths from a starting directory.
pub fn get_file_path_data(
    start_path: String,
    show_hidden: bool,
    sort_by: SortBy,
    sort_type: &SortType,
) -> anyhow::Result<Vec<String>> {
    Ok(read_listing(start_path, show_hidden, sort_by, sort_type)?.files)
}

/// Check if a path is a file (not a directory).
pub fn is_file(path: String) -> bool {
    match fs::metadata(path) {
        Ok(file) => file.file_type().is_file(),
        Err(_) => false,
    }
}

/// Generate a unique copy path in `new_path` by prefixing the file name with
/// "copy_" (then "copy_2_", "copy_3_", ...) until no entry with that name exists.
pub fn generate_copy_file_dir_name(
    curr_path: impl AsRef<Path>,
    new_path: impl AsRef<Path>,
) -> PathBuf {
    let file_name = curr_path.as_ref().file_name().unwrap_or_default();
    let candidate = |prefix: String| {
        let mut name = OsString::from(prefix);
        name.push(file_name);
        new_path.as_ref().join(name)
    };
    // symlink_metadata also treats dangling symlinks as taken.
    let is_free = |path: &Path| fs::symlink_metadata(path).is_err();

    // First try without a number suffix
    let first = candidate("copy_".to_string());
    if is_free(&first) {
        return first;
    }

    // Generate a unique name using numbered format: copy_2_file.txt, copy_3_file.txt, etc.
    let mut n = 2usize;
    loop {
        let numbered = candidate(format!("copy_{}_", n));
        if is_free(&numbered) {
            return numbered;
        }
        n += 1;
    }
}

/// Check if a path exists.
pub fn check_if_exists(new_path: String) -> bool {
    Path::new(&new_path).try_exists().unwrap_or_default()
}

/// Get the parent directory path from a file path.
pub fn get_curr_path(path: String) -> String {
    Path::new(&path)
        .parent()
        .map(|parent| parent.to_string_lossy().to_string())
        .unwrap_or_default()
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    fn with_info(paths: Vec<PathBuf>) -> Vec<(PathBuf, EntryInfo)> {
        paths
            .into_iter()
            .map(|path| {
                let info = EntryInfo::read(&path);
                (path, info)
            })
            .collect()
    }

    #[cfg(unix)]
    #[test]
    fn file_path_data_lists_root() {
        for show_hidden in [false, true] {
            for sort_by in [
                SortBy::Default,
                SortBy::Name,
                SortBy::Size,
                SortBy::DateAdded,
            ] {
                let files =
                    get_file_path_data("/".into(), show_hidden, sort_by, &SortType::ASC).unwrap();
                assert!(!files.is_empty());
                for file in files {
                    let path = Path::new(&file);
                    assert_eq!(path.parent(), Some(Path::new("/")));
                    assert!(
                        show_hidden
                            || path.is_dir()
                            || !path
                                .file_name()
                                .unwrap()
                                .as_encoded_bytes()
                                .starts_with(b".")
                    );
                }
            }
        }
    }

    /// Fail unless each record describes the path beside it.
    pub(crate) fn assert_records_match(files: &[String], info: &[EntryInfo]) {
        assert_eq!(files.len(), info.len());
        for (file, info) in files.iter().zip(info) {
            assert_eq!(*info, EntryInfo::read(Path::new(file)), "{file}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn listing_records_stay_beside_their_paths_whatever_the_sort() {
        let dir = tempfile::tempdir().unwrap();
        for (name, len) in [
            ("b.txt", 300),
            ("a.txt", 10),
            (".hidden", 20),
            ("c.txt", 2000),
        ] {
            fs::write(dir.path().join(name), "x".repeat(len)).unwrap();
        }
        fs::create_dir(dir.path().join("folder")).unwrap();
        std::os::unix::fs::symlink(dir.path().join("folder"), dir.path().join("link")).unwrap();
        std::os::unix::fs::symlink(dir.path().join("gone"), dir.path().join("dangling")).unwrap();
        let start = dir.path().to_string_lossy().into_owned();

        for show_hidden in [false, true] {
            for sort_by in [
                SortBy::Default,
                SortBy::Name,
                SortBy::Size,
                SortBy::DateAdded,
            ] {
                for sort_type in [SortType::ASC, SortType::DESC] {
                    let listing =
                        read_listing(start.clone(), show_hidden, sort_by.clone(), &sort_type)
                            .unwrap();
                    // Never the dangling link.
                    assert_eq!(listing.files.len(), if show_hidden { 6 } else { 5 });
                    assert_records_match(&listing.files, &listing.info);
                }
            }
        }

        let link = EntryInfo::read(&dir.path().join("link"));
        assert!(link.is_symlink && link.is_dir());
        let dangling = EntryInfo::read(&dir.path().join("dangling"));
        assert!(dangling.is_symlink && dangling.kind.is_none());
    }

    #[test]
    fn file_path_data_reports_missing_directory() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("missing").to_string_lossy().into_owned();

        let error = get_file_path_data(missing.clone(), false, SortBy::Default, &SortType::ASC)
            .unwrap_err();

        assert_eq!(
            error.downcast_ref::<io::Error>().unwrap().kind(),
            io::ErrorKind::NotFound
        );
    }

    #[cfg(unix)]
    #[test]
    fn file_path_data_reports_unreadable_directory() {
        use std::os::unix::fs::PermissionsExt;

        if unsafe { libc::geteuid() } == 0 {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let permissions = fs::metadata(dir.path()).unwrap().permissions();
        fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o000)).unwrap();

        let path = dir.path().to_string_lossy().into_owned();
        let result = get_file_path_data(path.clone(), false, SortBy::Default, &SortType::ASC);
        fs::set_permissions(dir.path(), permissions).unwrap();

        let error = result.unwrap_err();
        assert_eq!(
            error.downcast_ref::<io::Error>().unwrap().kind(),
            io::ErrorKind::PermissionDenied
        );
    }

    #[test]
    fn test_is_file_with_directory() {
        // Current directory should not be a file
        assert!(!is_file(".".to_string()));
    }

    #[test]
    fn test_check_if_exists() {
        // Current directory should exist
        assert!(check_if_exists(".".to_string()));
        // Random path should not exist
        assert!(!check_if_exists("/nonexistent/path/12345".to_string()));
    }

    #[test]
    fn test_get_curr_path() {
        assert_eq!(get_curr_path("/foo/bar/baz.txt".to_string()), "/foo/bar");
        assert_eq!(get_curr_path("/foo/bar".to_string()), "/foo");
        assert_eq!(get_curr_path("/foo".to_string()), "/");
    }

    #[test]
    fn date_sort_is_deterministic_with_missing_entries() {
        let dir = tempfile::tempdir().unwrap();
        let mut entries = vec![dir.path().join("missing")];
        for name in ["b.txt", "a.txt", "c.txt"] {
            fs::write(dir.path().join(name), name).unwrap();
            entries.push(dir.path().join(name));
        }
        let entries = with_info(entries);

        for (sort_type, missing_index) in [(SortType::ASC, 0), (SortType::DESC, 3)] {
            let first = sort_entries_by_type(SortBy::DateAdded, sort_type.clone(), entries.clone());
            let second = sort_entries_by_type(SortBy::DateAdded, sort_type, first.clone());
            assert_eq!(first, second);
            // The missing entry falls back to UNIX_EPOCH, the oldest possible date.
            assert_eq!(first[missing_index].0, dir.path().join("missing"));
        }
    }

    #[test]
    fn size_sort_orders_by_length() {
        let dir = tempfile::tempdir().unwrap();
        let entries = with_info(
            [("big", 3), ("small", 1), ("mid", 2)]
                .into_iter()
                .map(|(name, len)| {
                    let path = dir.path().join(name);
                    fs::write(&path, "x".repeat(len)).unwrap();
                    path
                })
                .collect(),
        );
        let names = |sorted: Vec<(PathBuf, EntryInfo)>| -> Vec<String> {
            sorted
                .iter()
                .map(|(path, _)| path.file_name().unwrap().to_string_lossy().to_string())
                .collect()
        };

        let asc = sort_entries_by_type(SortBy::Size, SortType::ASC, entries.clone());
        assert_eq!(names(asc), ["small", "mid", "big"]);
        let desc = sort_entries_by_type(SortBy::Size, SortType::DESC, entries);
        assert_eq!(names(desc), ["big", "mid", "small"]);
    }

    #[test]
    fn copy_name_adds_numbered_suffix_when_taken() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("file.txt");
        for name in ["file.txt", "copy_file.txt", "copy_2_file.txt"] {
            fs::write(dir.path().join(name), "").unwrap();
        }

        assert_eq!(
            generate_copy_file_dir_name(&src, dir.path()),
            dir.path().join("copy_3_file.txt")
        );
    }

    #[test]
    fn copy_name_without_file_name_does_not_panic() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(
            generate_copy_file_dir_name(Path::new("/"), dir.path()),
            dir.path().join("copy_")
        );
    }

    #[cfg(unix)]
    #[test]
    fn copy_name_keeps_non_utf8_file_name() {
        use std::ffi::OsStr;
        use std::os::unix::ffi::OsStrExt;

        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join(OsStr::from_bytes(b"bad\xff.txt"));

        let first = generate_copy_file_dir_name(&src, dir.path());
        assert_eq!(
            first,
            dir.path().join(OsStr::from_bytes(b"copy_bad\xff.txt"))
        );

        // Some filesystems (e.g. APFS) reject non-UTF-8 names; check collisions where allowed.
        if fs::write(&first, "").is_ok() {
            let second = generate_copy_file_dir_name(&src, dir.path());
            assert_eq!(
                second,
                dir.path().join(OsStr::from_bytes(b"copy_2_bad\xff.txt"))
            );
        }
    }
}
