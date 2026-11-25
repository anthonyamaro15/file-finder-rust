# Changelog

Notable changes to ff. Versions follow [Semantic Versioning](https://semver.org/), and
the format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

## [0.3.1] - 2026-11-24

### Changed

- The README's Homebrew steps start with `brew trust anthonyamaro15/tap`, which Homebrew 7
  and newer require before installing or upgrading from a third-party tap.

### Fixed

- A copy, move, extract or delete that finished while you were typing a root search
  cleared the search but left its results in use, so the list showed the folder while
  `⏎`, `d` and the other actions used a search result you could not see. The search and
  its results now stay, and the list refreshes underneath.
  ([#170](https://github.com/anthonyamaro15/file-finder-rust/pull/170))
- Typing a search left the cursor on the row it had in the folder, so `⏎` in a root
  search could choose a result other than the top one. The cursor now goes to the first
  result whenever the search text changes.
- Root search drew only the first 50 results while the cursor could move through all of
  them, so past row 50 the actions used a result that was not on screen. Every result
  is drawn now, up to `max_search_results`.
  ([#171](https://github.com/anthonyamaro15/file-finder-rust/pull/171))
- Building from source failed with GCC 16, which rejects the Oniguruma C code bundled in
  onig_sys 69.8.1. onig is updated to 6.5.3, which builds.
  ([#173](https://github.com/anthonyamaro15/file-finder-rust/pull/173))

### Security

- Updated anyhow, crossbeam-epoch and quick-xml to clear RustSec advisories, and removed
  the unused indicatif dependency. CI now checks dependencies for advisories.
  ([#172](https://github.com/anthonyamaro15/file-finder-rust/pull/172))

## [0.3.0] - 2026-11-14

### Added

- Index manager (`Space` then `i`, or "Choose which folders are indexed" in the `:`
  palette): the folders under `root_dir` (your home by default), biggest first, with how
  much of the index each one fills. `␣` switches a folder off or back on, `l`/`h` go in and out, `⏎` saves to
  `ignore_paths`, and the title shows the index size before and after.
- Index manager's "biggest" view (`Tab`): the index as a tree whose heaviest branches
  are opened for you down to what fills them (caches, build output, virtualenvs,
  plugins), with everything small folded into "… N smaller" lines. `␣` switches any
  branch off, `l`/`h` open and fold, and `Tab` goes back to the by-folder view at the
  same place. ([#164](https://github.com/anthonyamaro15/file-finder-rust/pull/164))
  ([#163](https://github.com/anthonyamaro15/file-finder-rust/pull/163))
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

- Root search and the Copy to… / Move to… picker take several words, in any order:
  `geoid comp` finds `…/geoid-ai/app/components`, where a space used to find nothing.
  Each word must match the folder's name or a folder above it, and every word is
  highlighted. Results are ranked by how each word was found (as initials, from the
  starts of words, inside a word, or as scattered letters), then an exact name (spaces
  included: `user data` finds `User Data`), then words in the folder's own name, then
  shallower folders. So `config` lists a folder named `config` first, `ffr` finds
  `file-finder-rust` and `customp` finds `custom-plugins`. Searches take about as long
  as before.
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

- The `ff` shell function now fails when you quit without choosing anything, so
  `ff && nvim .` no longer opens the editor after a cancel. `ff --version`, `ff --help`,
  `ff init` and `ff --reset-config` run as before.
- `src` folders are indexed by default: `src` is no longer in the default
  `ignore_directories`. If your `settings.toml` lists it, remove it there to find
  them.
- Root search could return a few fewer results than its limit when the folder index
  listed a path twice. ([#160](https://github.com/anthonyamaro15/file-finder-rust/pull/160))

### Security

- Updated zip, pdf-extract, tar, time and rand to clear Dependabot alerts.
  ([#153](https://github.com/anthonyamaro15/file-finder-rust/pull/153))

## [0.2.0] - 2026-10-27

See the [v0.2.0 release](https://github.com/anthonyamaro15/file-finder-rust/releases/tag/v0.2.0).
Earlier versions are listed on the [releases page](https://github.com/anthonyamaro15/file-finder-rust/releases).

[Unreleased]: https://github.com/anthonyamaro15/file-finder-rust/compare/v0.3.1...HEAD
[0.3.1]: https://github.com/anthonyamaro15/file-finder-rust/compare/v0.3.0...v0.3.1
[0.3.0]: https://github.com/anthonyamaro15/file-finder-rust/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/anthonyamaro15/file-finder-rust/releases/tag/v0.2.0
