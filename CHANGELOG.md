# Changelog

Notable changes to ff. Versions follow [Semantic Versioning](https://semver.org/), and
the format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

### Added

- `ignore_paths` setting: exact folders to leave out of the folder index with everything
  below them, e.g. `ignore_paths = ["~/Movies", "~/go"]`. Unlike `ignore_directories`,
  `"~/go"` doesn't skip other folders named `go`. Renaming an excluded folder in ff keeps
  it excluded; moving one elsewhere with Move to… doesn't carry the setting along yet.
  ([#162](https://github.com/anthonyamaro15/file-finder-rust/pull/162))
- When `ignore_paths` or `ignore_directories` change, ff drops the newly skipped
  folders from the index as soon as it starts and re-reads the rest in the background,
  instead of waiting up to an hour for the next refresh.
  ([#162](https://github.com/anthonyamaro15/file-finder-rust/pull/162))

### Changed

- `Enter` while typing a root search goes straight to the top result, so the flow is
  `Space` `/`, type, `Enter`. With no results it only leaves the box; in a local search
  it keeps the filter, like `Esc`. ([#157](https://github.com/anthonyamaro15/file-finder-rust/pull/157))
- ff opens faster in terminals that don't answer its image-support query (detached tmux,
  some terminals, SSH): about 1 s to about 0.03 s. It now asks on the first image preview
  instead of at launch. ([#158](https://github.com/anthonyamaro15/file-finder-rust/pull/158))
- Root search and the Copy to… / Move to… picker are about 3× faster on short queries
  (36,982 folders: `a` 27 → 9 ms, `fi` 20 → 8 ms per keystroke), with the same results.
  ([#160](https://github.com/anthonyamaro15/file-finder-rust/pull/160))
- Root search shows how many folders it covers and how long each search took, in the top
  bar: `36,982 folders` before you type, then `124 of 36,982 · 2 ms`.
  ([#161](https://github.com/anthonyamaro15/file-finder-rust/pull/161))
- ff no longer captures the mouse, so you can select text with it again.
  ([#158](https://github.com/anthonyamaro15/file-finder-rust/pull/158))
- Building from source needs Rust 1.88 or newer (was 1.81).
  ([#153](https://github.com/anthonyamaro15/file-finder-rust/pull/153))

### Fixed

- Root search could return a few fewer results than its limit when the folder index
  listed a path twice. ([#160](https://github.com/anthonyamaro15/file-finder-rust/pull/160))

### Security

- Updated zip, pdf-extract, tar, time and rand to clear Dependabot alerts.
  ([#153](https://github.com/anthonyamaro15/file-finder-rust/pull/153))

## [0.2.0] - 2026-10-27

See the [v0.2.0 release](https://github.com/anthonyamaro15/file-finder-rust/releases/tag/v0.2.0).
Earlier versions are listed on the [releases page](https://github.com/anthonyamaro15/file-finder-rust/releases).

[Unreleased]: https://github.com/anthonyamaro15/file-finder-rust/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/anthonyamaro15/file-finder-rust/releases/tag/v0.2.0
