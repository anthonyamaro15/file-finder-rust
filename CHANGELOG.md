# Changelog

Notable changes to ff. Versions follow [Semantic Versioning](https://semver.org/), and
the format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

### Changed

- `Enter` while typing a root search goes straight to the top result, so the flow is
  `Space` `/`, type, `Enter`. With no results it only leaves the box; in a local search
  it keeps the filter, like `Esc`. ([#157](https://github.com/anthonyamaro15/file-finder-rust/pull/157))
- ff opens faster in terminals that don't answer its image-support query (detached tmux,
  some terminals, SSH): about 1 s to about 0.03 s. It now asks on the first image preview
  instead of at launch. ([#158](https://github.com/anthonyamaro15/file-finder-rust/pull/158))
- ff no longer captures the mouse, so you can select text with it again.
  ([#158](https://github.com/anthonyamaro15/file-finder-rust/pull/158))
- Building from source needs Rust 1.88 or newer (was 1.81).
  ([#153](https://github.com/anthonyamaro15/file-finder-rust/pull/153))

### Security

- Updated zip, pdf-extract, tar, time and rand to clear Dependabot alerts.
  ([#153](https://github.com/anthonyamaro15/file-finder-rust/pull/153))

## [0.2.0] - 2026-10-27

See the [v0.2.0 release](https://github.com/anthonyamaro15/file-finder-rust/releases/tag/v0.2.0).
Earlier versions are listed on the [releases page](https://github.com/anthonyamaro15/file-finder-rust/releases).

[Unreleased]: https://github.com/anthonyamaro15/file-finder-rust/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/anthonyamaro15/file-finder-rust/releases/tag/v0.2.0
