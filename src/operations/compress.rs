//! Compress a file or folder into a ZIP archive beside it, like Finder's "Compress".

use std::fmt::Display;
use std::fs::{self, File, Metadata, OpenOptions};
use std::io::{self, BufWriter, ErrorKind, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use walkdir::WalkDir;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, DateTime, ZipWriter};

/// Files from this size get ZIP64 sizes. Deflate can make incompressible data slightly
/// larger, so this stays below the 4 GiB limit.
const LARGE_FILE_BYTES: u64 = 4_000_000_000;

/// Reserve the archive for `source` beside it, named like Finder: `Photos.zip`, then
/// `Photos 2.zip`, … Exclusive creation never replaces a file or link (even a dangling one).
pub fn reserve_archive(source: &Path) -> Result<(PathBuf, File), String> {
    let name = source
        .file_name()
        .ok_or_else(|| format!("Cannot compress '{}'", source.display()))?;
    let mut number = 1;
    loop {
        let mut archive_name = name.to_os_string();
        if number > 1 {
            archive_name.push(format!(" {number}"));
        }
        archive_name.push(".zip");
        let archive = source.with_file_name(archive_name);
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&archive)
        {
            Ok(file) => return Ok((archive, file)),
            Err(e) if e.kind() == ErrorKind::AlreadyExists => number += 1,
            Err(e) => return Err(path_error("create", &archive, e)),
        }
    }
}

/// Write `source` into `archive`, reserved by [`reserve_archive`], with the item itself
/// as the top-level entry. Cancellation or any error removes the archive.
pub fn compress_to_zip(
    source: &Path,
    archive: &Path,
    file: File,
    cancel: &AtomicBool,
) -> Result<String, String> {
    match write_entries(source, archive, &file, cancel) {
        Ok(skipped) => {
            let mut message = format!(
                "Compressed '{}' to '{}'",
                display_name(source),
                display_name(archive)
            );
            if skipped > 0 {
                let plural = if skipped == 1 { "" } else { "s" };
                message.push_str(&format!(", skipped {skipped} special file{plural}"));
            }
            Ok(message)
        }
        Err(error) => Err(match remove_archive(archive, &file) {
            Ok(()) => format!("{error} (partial archive removed)"),
            Err(e) => format!(
                "{error} (failed to remove partial archive '{}': {e})",
                archive.display()
            ),
        }),
    }
}

/// Add every entry under `source` without following links (so links out of the folder
/// or loops are stored as links) and return how many pipes, sockets or devices it skipped.
fn write_entries(
    source: &Path,
    archive: &Path,
    file: &File,
    cancel: &AtomicBool,
) -> Result<usize, String> {
    let cancelled = || Err("Compression cancelled".to_string());
    let parent = source.parent().unwrap_or(Path::new(""));
    let mut zip = ZipWriter::new(BufWriter::new(file));
    let mut buffer = vec![0; 64 * 1024];
    let mut skipped = 0;
    let walk = WalkDir::new(source)
        .follow_links(false)
        .follow_root_links(false);
    for entry in walk {
        if cancel.load(Ordering::Relaxed) {
            return cancelled();
        }
        let entry = entry.map_err(|e| match (e.path(), e.io_error()) {
            (Some(path), Some(io_error)) => path_error("read", path, io_error),
            _ => e.to_string(),
        })?;
        let path = entry.path();
        let not_utf8 = || format!("Cannot store '{}': name is not UTF-8", path.display());
        let name = path
            .strip_prefix(parent)
            .ok()
            .and_then(Path::to_str)
            .ok_or_else(not_utf8)?;
        let metadata = fs::symlink_metadata(path).map_err(|e| path_error("read", path, e))?;
        let options = SimpleFileOptions::default()
            .compression_method(CompressionMethod::Deflated)
            .last_modified_time(zip_time(&metadata).unwrap_or_default());
        #[cfg(unix)]
        let options = {
            use std::os::unix::fs::PermissionsExt;
            options.unix_permissions(metadata.permissions().mode())
        };
        let file_type = metadata.file_type();
        if file_type.is_dir() {
            zip.add_directory(name, options)
                .map_err(|e| path_error("write", archive, e))?;
        } else if file_type.is_symlink() {
            let target = fs::read_link(path).map_err(|e| path_error("read", path, e))?;
            let target = target.to_str().ok_or_else(not_utf8)?;
            zip.add_symlink(name, target, options)
                .map_err(|e| path_error("write", archive, e))?;
        } else if file_type.is_file() {
            let mut input = File::open(path).map_err(|e| path_error("read", path, e))?;
            let options = options.large_file(metadata.len() >= LARGE_FILE_BYTES);
            zip.start_file(name, options)
                .map_err(|e| path_error("write", archive, e))?;
            loop {
                if cancel.load(Ordering::Relaxed) {
                    return cancelled();
                }
                let read = match input.read(&mut buffer) {
                    Ok(0) => break,
                    Ok(read) => read,
                    Err(e) if e.kind() == ErrorKind::Interrupted => continue,
                    Err(e) => return Err(path_error("read", path, e)),
                };
                zip.write_all(&buffer[..read])
                    .map_err(|e| path_error("write", archive, e))?;
            }
        } else if entry.depth() == 0 {
            return Err(format!(
                "Cannot compress '{}': not a file, folder or link",
                path.display()
            ));
        } else {
            // Opening a pipe would block forever.
            log::warn!("Skipping special file: {}", path.display());
            skipped += 1;
        }
    }
    let mut output = zip.finish().map_err(|e| path_error("write", archive, e))?;
    output
        .flush()
        .map_err(|e| path_error("write", archive, e))?;
    Ok(skipped)
}

/// Modification time as a ZIP (DOS) date in local time, which is how ff and unzip read it
/// back. It has 2 s resolution and only covers 1980–2107.
fn zip_time(metadata: &Metadata) -> Option<DateTime> {
    #[cfg(unix)]
    {
        let local = crate::utils::format::local_time(metadata.modified().ok()?)?;
        let field = |value: i32| u8::try_from(value).ok();
        DateTime::from_date_and_time(
            u16::try_from(local.tm_year + 1900).ok()?,
            field(local.tm_mon + 1)?,
            field(local.tm_mday)?,
            field(local.tm_hour)?,
            field(local.tm_min)?,
            field(local.tm_sec)?,
        )
        .ok()
    }
    #[cfg(not(unix))]
    {
        let _ = metadata;
        None
    }
}

/// Remove `archive` only while it is still the file this job created.
fn remove_archive(archive: &Path, file: &File) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let (created, current) = (file.metadata()?, fs::symlink_metadata(archive)?);
        if (created.dev(), created.ino()) != (current.dev(), current.ino()) {
            return Err(io::Error::other("it was replaced by another file"));
        }
    }
    fs::remove_file(archive)
}

fn path_error(action: &str, path: &Path, error: impl Display) -> String {
    format!("Could not {action} '{}': {error}", path.display())
}

fn display_name(path: &Path) -> String {
    path.file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned()
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use crate::file_reader_content::FileContent;
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::fs::{symlink, PermissionsExt};
    use std::sync::mpsc;
    use std::time::{Duration, SystemTime};

    fn compress(source: &Path) -> Result<(PathBuf, String), String> {
        let (archive, file) = reserve_archive(source)?;
        let message = compress_to_zip(source, &archive, file, &AtomicBool::new(false))?;
        Ok((archive, message))
    }

    fn entry_names(archive: &Path) -> Vec<String> {
        let archive = zip::ZipArchive::new(File::open(archive).unwrap()).unwrap();
        archive.file_names().map(str::to_string).collect()
    }

    fn sorted_listing(dir: &Path) -> Vec<String> {
        let mut names: Vec<_> = fs::read_dir(dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    fn set_modified(path: &Path, time: SystemTime) {
        File::open(path)
            .unwrap()
            .set_times(fs::FileTimes::new().set_modified(time))
            .unwrap();
    }

    #[test]
    fn folder_round_trips_through_ff_extraction() {
        let tmp = tempfile::tempdir().unwrap();
        let source = tmp.path().join("Photos");
        fs::create_dir_all(source.join("nested/deep")).unwrap();
        fs::create_dir(source.join("empty")).unwrap();
        for (relative, contents, mode) in [
            ("a.txt", "alpha", 0o644),
            (".hidden", "hidden", 0o600),
            ("run.sh", "#!/bin/sh\nexit 0\n", 0o755),
            ("nested/deep/b.txt", "bravo", 0o640),
        ] {
            fs::write(source.join(relative), contents).unwrap();
            fs::set_permissions(source.join(relative), fs::Permissions::from_mode(mode)).unwrap();
        }
        symlink("a.txt", source.join("link")).unwrap();
        // An odd second, since ZIP times have 2 s resolution.
        let modified = SystemTime::UNIX_EPOCH + Duration::from_secs(1_592_224_497);
        for entry in WalkDir::new(&source).contents_first(true) {
            let entry = entry.unwrap();
            if !entry.path_is_symlink() {
                set_modified(entry.path(), modified);
            }
        }

        let (archive, message) = compress(&source).unwrap();
        assert_eq!(archive, tmp.path().join("Photos.zip"));
        assert_eq!(message, "Compressed 'Photos' to 'Photos.zip'");
        let names = entry_names(&archive);
        assert!(
            names.iter().all(|name| name.starts_with("Photos/")),
            "{names:?}"
        );
        assert!(names.contains(&"Photos/empty/".to_string()), "{names:?}");

        // The link is stored as a link, not followed.
        let mut zip = zip::ZipArchive::new(File::open(&archive).unwrap()).unwrap();
        let mut link = zip.by_name("Photos/link").unwrap();
        assert_eq!(link.unix_mode().unwrap() & 0o170000, 0o120000);
        let mut target = String::new();
        link.read_to_string(&mut target).unwrap();
        assert_eq!(target, "a.txt");
        drop(link);

        // ff's `y` extracts into a new folder named after the archive.
        let out = tmp.path().join("out");
        fs::create_dir(&out).unwrap();
        fs::rename(&archive, out.join("Photos.zip")).unwrap();
        let extracted = FileContent::extract_zip_content(&out.join("Photos.zip").to_string_lossy());
        // ff's extractor refuses ZIP symlinks by design; everything else must come back.
        let error = extracted.unwrap_err();
        assert!(
            error.contains("1 failed") && error.contains("Refusing ZIP symlink"),
            "{error}"
        );
        let copy = out.join("Photos/Photos");
        for entry in WalkDir::new(&source) {
            let entry = entry.unwrap();
            let relative = entry.path().strip_prefix(&source).unwrap();
            if entry.path_is_symlink() {
                continue;
            }
            let original = entry.metadata().unwrap();
            let restored = fs::symlink_metadata(copy.join(relative)).unwrap();
            assert_eq!(original.file_type(), restored.file_type(), "{relative:?}");
            assert_eq!(
                original.permissions().mode() & 0o777,
                restored.permissions().mode() & 0o777,
                "{relative:?}"
            );
            let drift = original
                .modified()
                .unwrap()
                .duration_since(restored.modified().unwrap())
                .unwrap_or_else(|e| e.duration());
            assert!(drift <= Duration::from_secs(2), "{relative:?}: {drift:?}");
            if original.is_file() {
                assert_eq!(
                    fs::read(entry.path()).unwrap(),
                    fs::read(copy.join(relative)).unwrap()
                );
            }
        }
        let mut expected = sorted_listing(&source);
        expected.retain(|name| name != "link");
        assert_eq!(sorted_listing(&copy), expected);
    }

    #[test]
    fn single_file_gets_its_extension_kept() {
        let tmp = tempfile::tempdir().unwrap();
        let source = tmp.path().join("report.pdf");
        fs::write(&source, "pdf").unwrap();
        let (archive, message) = compress(&source).unwrap();
        assert_eq!(archive, tmp.path().join("report.pdf.zip"));
        assert_eq!(message, "Compressed 'report.pdf' to 'report.pdf.zip'");
        assert_eq!(entry_names(&archive), ["report.pdf"]);
    }

    #[test]
    fn taken_names_are_never_replaced() {
        let tmp = tempfile::tempdir().unwrap();
        let source = tmp.path().join("Photos");
        fs::create_dir(&source).unwrap();
        fs::write(tmp.path().join("Photos.zip"), "keep").unwrap();
        symlink("missing", tmp.path().join("Photos 2.zip")).unwrap();
        let (archive, message) = compress(&source).unwrap();
        assert_eq!(archive, tmp.path().join("Photos 3.zip"));
        assert_eq!(message, "Compressed 'Photos' to 'Photos 3.zip'");
        assert_eq!(
            fs::read_to_string(tmp.path().join("Photos.zip")).unwrap(),
            "keep"
        );
        assert_eq!(
            fs::read_link(tmp.path().join("Photos 2.zip")).unwrap(),
            Path::new("missing")
        );
    }

    #[test]
    fn special_files_are_skipped_without_blocking() {
        let tmp = tempfile::tempdir().unwrap();
        let source = tmp.path().join("Photos");
        fs::create_dir(&source).unwrap();
        fs::write(source.join("a.txt"), "alpha").unwrap();
        let fifo = CString::new(source.join("fifo").as_os_str().as_bytes()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(fifo.as_ptr(), 0o600) }, 0);
        let (tx, rx) = mpsc::channel();
        let worker_source = source.clone();
        std::thread::spawn(move || tx.send(compress(&worker_source)).unwrap());
        let (archive, message) = rx.recv_timeout(Duration::from_secs(10)).unwrap().unwrap();
        assert_eq!(
            message,
            "Compressed 'Photos' to 'Photos.zip', skipped 1 special file"
        );
        let mut names = entry_names(&archive);
        names.sort();
        assert_eq!(names, ["Photos/", "Photos/a.txt"]);
    }

    #[test]
    fn unreadable_file_fails_and_leaves_no_archive() {
        if unsafe { libc::geteuid() } == 0 {
            return; // root reads anything
        }
        let tmp = tempfile::tempdir().unwrap();
        let source = tmp.path().join("Photos");
        fs::create_dir(&source).unwrap();
        let secret = source.join("secret.txt");
        fs::write(&secret, "secret").unwrap();
        fs::set_permissions(&secret, fs::Permissions::from_mode(0o000)).unwrap();
        let error = compress(&source).unwrap_err();
        assert!(error.contains(&secret.display().to_string()), "{error}");
        assert!(error.ends_with("(partial archive removed)"), "{error}");
        assert_eq!(sorted_listing(tmp.path()), ["Photos"]);
    }

    #[test]
    fn cancellation_leaves_no_archive() {
        let tmp = tempfile::tempdir().unwrap();
        let source = tmp.path().join("Photos");
        fs::create_dir(&source).unwrap();
        fs::write(source.join("a.txt"), "alpha").unwrap();
        let (archive, file) = reserve_archive(&source).unwrap();
        let error = compress_to_zip(&source, &archive, file, &AtomicBool::new(true)).unwrap_err();
        assert_eq!(error, "Compression cancelled (partial archive removed)");
        assert_eq!(sorted_listing(tmp.path()), ["Photos"]);
    }

    #[test]
    fn archive_never_contains_itself() {
        let tmp = tempfile::tempdir().unwrap();
        let source = tmp.path().join("Photos");
        fs::create_dir(&source).unwrap();
        fs::write(source.join("a.txt"), "alpha").unwrap();
        // Links back to the folder the archive is written in are stored, not followed.
        symlink("..", source.join("up")).unwrap();
        symlink(".", tmp.path().join("here")).unwrap();

        let (archive, _) = compress(&source).unwrap();
        assert_eq!(archive.parent(), source.parent());
        let mut names = entry_names(&archive);
        names.sort();
        assert_eq!(names, ["Photos/", "Photos/a.txt", "Photos/up"]);

        let (archive, _) = compress(&tmp.path().join("here")).unwrap();
        assert_eq!(entry_names(&archive), ["here"]);
    }
}
