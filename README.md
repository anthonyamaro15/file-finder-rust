# ff

A terminal file finder: type a folder's name, and you're there.

I keep my files in a very structured set of folders, and some projects end up nested
quite deep. Opening one in Neovim meant `cd`-ing through every level first. With ff you
type part of the folder's name and your shell lands in it, even nine levels down, with
search across every folder under your home.

[![Built With Ratatui](https://ratatui.rs/built-with-ratatui/badge.svg)](https://ratatui.rs/)
![Platform](https://img.shields.io/badge/Platform-MacOS%20%7C%20Linux-blue)
![Language](https://img.shields.io/badge/Language-Rust-orange)
![License](https://img.shields.io/badge/License-MIT-green)

![ff: root search over 32,096 folders, where "comp" lists dozens of components folders and "comp geoid" leaves one; Enter puts the shell in it. Then "ffr" finds file-finder-rust by its initials and lands there too, and the index manager switches off a Go module cache that fills 62% of the index](assets/demo.gif)

## Contents

- [Quick start](#quick-start)
- [Features](#features)
- [Installation](#installation)
- [Usage](#usage)
- [Keyboard shortcuts](#keyboard-shortcuts)
- [Index manager](#index-manager)
- [Configuration](#configuration)
- [Troubleshooting](#troubleshooting)
- [Contributing](#contributing)
- [License](#license)

## Quick start

```bash
brew trust anthonyamaro15/tap      # Homebrew 7 and newer; skip on older versions
brew install anthonyamaro15/tap/ff
echo 'eval "$(ff init zsh)"' >> ~/.zshrc && source ~/.zshrc   # bash: use ~/.bashrc and "ff init bash"
```

Run `ff`, press `Space` then `/`, type part of a folder's name, and press `Enter`: ff
quits and your shell is in the top result. To pick another result, press `Esc` to browse
them, then `Enter` on the folder you want.

## Features

**Find any folder**

- Root search over an index of every folder under your home, results as you type; the
  top bar shows how many folders were searched and how long it took
  (`124 of 33,301 · 2 ms`)
- Several words narrow it down, in any order: `geoid comp` finds
  `~/work/geoid-ai/app/components` among 77 `components` folders. Words found as typed
  rank above scattered letters, an exact name comes first, and initials or the starts
  of words count (`ffr` finds `file-finder-rust`, `customp` finds `custom-plugins`)
- `Enter` quits ff and your shell `cd`s into the folder (shell integration), or opens it
  in Neovim, VS Code or Zed
- Fuzzy search within the current folder
- The index refreshes in the background, and ff's own changes update it at once

**Control what's indexed**

- The index manager (`Space`, then `i`) lists the folders that fill the index, biggest
  first, and switches any of them out of it
- A tree view opened down to the biggest messes (app caches, build output, virtualenvs),
  even ones buried deep inside folders you keep
- `ignore_paths` and `ignore_directories` settings for exact folders and folder names

**Manage files**

- Vim-style navigation (`h`, `j`, `k`, `l`), command palette (`:`) and rebindable keys
- Create, rename, copy and delete; Copy to… and Move to… any folder picked by name
- Move to Trash or delete permanently, one item or a range selected in visual mode
- Extract and create ZIP archives, open with the default app, Get Info
- Sort by name, size or date; folder sizes counted in the background

**Preview**

- Code with syntax highlighting and line numbers, rendered Markdown
- Images in Kitty, iTerm2, WezTerm, Ghostty and Sixel terminals
- PDF text, archive contents, hex view for binary files

**Views**

- Normal (list and preview), full-width list, and dual pane with copy and move between
  panes
- A status line about the highlighted item and a hint row with the keys for the current
  mode
- Themes, syntax themes and Nerd Font icons

## Installation

### Homebrew (macOS and Linux)

```bash
brew trust anthonyamaro15/tap
brew install anthonyamaro15/tap/ff
```

Homebrew 7 and newer only load formulas from taps you've trusted; without the first line,
`brew install` and `brew upgrade ff` stop with "Refusing to load formula … from untrusted
tap". Older Homebrew versions don't have `brew trust` and don't need it. Upgrade with
`brew upgrade ff`.

Builds are published for macOS (Apple Silicon and Intel) and x86_64 Linux.

### From source

Requires Rust 1.88 or newer, on macOS or Linux.

```bash
git clone https://github.com/anthonyamaro15/file-finder-rust.git
cd file-finder-rust
cargo build --release
cp target/release/ff /usr/local/bin/
```

### Shell integration

To have your shell `cd` into the folder you pick, add one line to your shell's config and
reload it:

```bash
eval "$(ff init zsh)"    # ~/.zshrc
eval "$(ff init bash)"   # ~/.bashrc
```

With it, `Enter` on a folder leaves your shell in that folder. `Enter` on a file copies
its path to the clipboard (with `pbcopy`, `wl-copy` or `xclip`; without one it's
printed).

## Usage

```bash
ff               # open in start_path (~/Desktop by default)
ff ~/Documents   # open in a folder: absolute, relative, ~ or .
```

```
ff [OPTIONS] [PATH] [COMMAND]
```

| Option              | Short | Description                              | Example              |
| ------------------- | ----- | ---------------------------------------- | -------------------- |
| `--start <PATH>`    | `-s`  | Folder to open in                        | `ff -s ~/Documents`  |
| `--editor <EDITOR>` | `-e`  | Open the chosen item in an editor        | `ff -e nvim`         |
| `--print-path`      |       | Print the chosen path (used by the shell integration) | `ff --print-path` |
| `--reset-config`    |       | Delete `~/.config/ff` and write defaults | `ff --reset-config`  |
| `--rebuild-cache`   |       | Rebuild the folder index of `root_dir`   | `ff --rebuild-cache` |
| `--help`            | `-h`  | Show help                                | `ff --help`          |
| `--version`         | `-V`  | Show the version                         | `ff --version`       |

| Command        | Description                         | Example       |
| -------------- | ----------------------------------- | ------------- |
| `init <SHELL>` | Print the shell integration script  | `ff init zsh` |

### Opening in an editor

```bash
ff -e nvim                      # supported: nvim, vscode, zed
ff -e zed --start ~/Projects
```

`Enter` quits ff and opens the highlighted item in the editor. Without an editor, its
path is copied to the clipboard. The shell integration always runs `ff --print-path`, so
`-e` has no effect through it; run `command ff -e nvim` to open items in an editor.

## Keyboard shortcuts

Press `?` in ff for the live list, including your own bindings. Every Normal-mode key
can be rebound (see [Keybindings](#keybindings)).

### Navigation

| Key           | Action                                             |
| ------------- | -------------------------------------------------- |
| `↑` / `k`     | Move up                                            |
| `↓` / `j`     | Move down                                          |
| `gg`          | Go to the first item                               |
| `G` (Shift+G) | Go to the last item                                |
| `←` / `h`     | Parent folder                                      |
| `→` / `l`     | Enter folder                                       |
| `Enter`       | Quit, opening it in the editor or copying its path |

### Search

| Key           | Action                                                         |
| ------------- | -------------------------------------------------------------- |
| `/` or `i`    | Search the current folder                                      |
| `Space` + `/` | Root search, across every indexed folder                       |
| `Enter`       | Root search: choose the top result. Local search: same as `Esc` |
| `Esc`         | Stop typing; the results stay to browse                        |
| `Space` + `i` | Index manager: choose which folders are indexed                |

Root search finds folders, not files. Each word you type must match the folder's name or
a folder above it; a word with `/` matches across the whole path. Starting a local search with a space or `/` switches
to root search, and `Backspace` in an empty root search goes back. To get the full
listing back after a search, press `/` then `Esc`.

### Files

| Key           | Action                                                   |
| ------------- | -------------------------------------------------------- |
| `a`           | Create a file (end the name with `/` for a folder)       |
| `r`           | Rename                                                   |
| `c`           | Copy beside the item                                     |
| `Shift+C`     | Copy to… (pick a folder)                                 |
| `Shift+M`     | Move to… (pick a folder)                                 |
| `d`           | Move to Trash (with confirmation)                        |
| `D` (Shift+D) | Delete permanently (with confirmation; cannot be undone) |
| `y`           | Extract a ZIP archive                                    |
| `z`           | Compress to a ZIP archive beside the item                |
| `o`           | Open with the default app and quit                       |
| `I` (Shift+I) | Get Info: size, dates, permissions, owner                |
| `.`           | Show or hide hidden files                                |

Copy to… and Move to… open a folder picker: type to search folders as root search does,
or type a path starting with `/` or `~`, then pick one with `↑`/`↓` and `Enter`. In dual
pane they copy or move to the other pane instead.

### Selecting several items

| Key               | Action                                        |
| ----------------- | --------------------------------------------- |
| `V` or `v`        | Start selecting at the highlighted item       |
| `j` `k` `gg` `G`  | Extend the selection                          |
| `o`               | Swap the selection's ends                     |
| `d`               | Move the selected items to Trash              |
| `D` (Shift+D)     | Delete the selected items permanently         |
| `Esc`, `V` or `v` | Cancel the selection                          |

The selection is the run of items from where you pressed `V` to the highlighted one. Both
`d` and `D` ask for confirmation, naming the items. They work in the background, one item
at a time: if some fail, the rest still go, and the status bar says what failed (details
in `~/.config/ff/ff.log`). Other keys do nothing while selecting.

### Views and tools

| Key            | Action                                       |
| -------------- | -------------------------------------------- |
| `p`            | Cycle the view: normal, full list, dual pane |
| `Tab`          | Switch the active pane (dual pane)           |
| `s`            | Sort menu                                    |
| `:`            | Command palette                              |
| `?`            | Show all keys                                |
| `q` / `Ctrl-C` | Quit                                         |

The command palette lists every action with its key. Type to filter by what an action
does (`hid`, `copy`, `trash`), then press `Enter` to run it. Some actions have no key
until you bind one and are reached through the palette: Copy path, Copy name and Copy
parent folder path.

After a key that starts a sequence, such as `Space` or `g`, a box at the bottom right
lists the keys that can follow.

## Index manager

Root search covers every folder under `root_dir`, so folders you never search, such as
app caches, package caches or build output, make results noisier and searches slower.
Press `Space`, then `i` (or "Choose which folders are indexed" in the `:` palette) to see
what fills the index and leave those folders out.

```
╭ Index · by folder │ biggest ───────────────── 33,301 → 16,326 folders (−51%) ╮
│ ~                                                   folders   of index │
│   ○ Movies                                       16,975  off               │
│ ❯ ● Desktop                                      15,015  ▆▆▆▆▆▆▆▆▆    92%  │
│   ● Downloads                                       699  ▖             4%  │
╰ 1 unsaved change ──────────────────────────────────────────────────────────╯
```

- **By folder** lists a folder's subfolders, biggest first, with how much of the index
  each one holds. `l` and `h` go in and out.
- **Biggest** (`Tab`) shows the index as a tree whose heaviest branches open by
  themselves, down to the folders that fill them. Small folders are folded into
  "… N smaller" lines, and `l`/`h` open and fold branches. `Tab` switches between the
  views and keeps your place.
- `Space` switches the highlighted folder off or back on. The title shows the index size
  before and after your changes, and `●` (indexed), `○` (off) and `◐` (something inside
  is off) mark each folder.
- `Enter` saves your changes to `ignore_paths` in `settings.toml`, changing only that
  setting, and the index drops those folders at once. Folders switched back on return
  after a background re-read. `Esc` with unsaved changes asks before discarding them.

Hidden folders, `~/Library` and folder names in `ignore_directories` are never indexed.

## Configuration

Settings live in `~/.config/ff/settings.toml`. Any setting you leave out uses its
default, so the file only needs what you change. Command-line options take precedence
over the file.

| Setting                | Default                                    | What it does                                                         |
| ---------------------- | ------------------------------------------ | -------------------------------------------------------------------- |
| `start_path`           | `~/Desktop`                                | Where ff opens without a path (the current folder if it isn't one)  |
| `root_dir`             | your home folder                           | What root search and Copy to…/Move to… cover                         |
| `ignore_directories`   | `node_modules`, `target`, `.git`, …        | Folder names the index skips, at any depth                           |
| `ignore_paths`         | `[]`                                       | Exact folders below `root_dir` the index skips with everything below them, e.g. `["~/Movies", "~/go"]` |
| `theme`                | `"onedark"`                                | Theme file in `~/.config/ff/themes/`                                 |
| `syntax_theme`         | `"base16-ocean.dark"`                      | Theme for code previews                                              |
| `use_nerd_fonts`       | `"auto"`                                   | `"always"`, `"never"` or `"auto"`                                    |
| `show_size_bars`       | `true`                                     | Sizes and size bars in lists (dates still show when off)             |
| `show_hints`           | `true`                                     | The hint row at the bottom                                           |
| `show_borders`         | `false`                                    | Boxes around the panes                                               |
| `status_bar_style`     | `"minimal"`                                | `"classic"` draws the status line as bordered boxes                  |
| `[finder] max_results` | `500`                                      | Most results a root search or the Copy to…/Move to… picker keeps     |

`ff --reset-config` deletes everything in `~/.config/ff` (settings, custom themes, the
folder index and `ff.log`) and writes fresh defaults, without opening ff.

### Folder index

The index lives in `~/.config/ff/cache_directory.json` and covers the folders under
`root_dir`, whatever folder ff opens in. Hidden folders, gitignored folders, `~/Library`,
`ignore_directories` and `ignore_paths` are left out.

- **Building:** the first start, `--rebuild-cache` and a changed `root_dir` read the
  folders in the background while you browse. Until the first read finishes, root search
  says "Building the folder index…".
- **Refreshing:** ff's own folder changes update the index at once, and an index older
  than an hour is re-read in the background at startup.
- **Rule changes:** when `ignore_paths` or `ignore_directories` change, the next start
  drops the newly skipped folders at once and re-reads the rest in the background.

### Keybindings

Every Normal-mode key can be rebound under `[keymap.normal]`, as `"key" = "action"`. An
entry replaces only that key's default.

| Action           | Default keys        | What it does                              |
| ---------------- | ------------------- | ----------------------------------------- |
| `nav.down`       | `j`, `<Down>`       | Move down                                 |
| `nav.up`         | `k`, `<Up>`         | Move up                                   |
| `nav.top`        | `gg`                | Go to the first item                      |
| `nav.bottom`     | `G`                 | Go to the last item                       |
| `nav.parent`     | `h`, `<Left>`       | Parent directory                          |
| `nav.enter`      | `l`, `<Right>`      | Enter directory                           |
| `nav.choose`     | `<Enter>`           | Quit: open in the editor, or copy/print the path |
| `select.visual`  | `V`, `v`            | Start or end selecting adjacent items     |
| `file.create`    | `a`                 | Create file (end with / for a folder)     |
| `file.rename`    | `r`                 | Rename                                    |
| `file.duplicate` | `c`                 | Copy beside the item                      |
| `file.copy_to`   | `C`                 | Copy to… (the other pane in Dual Pane)    |
| `file.move_to`   | `M`                 | Move to… (the other pane in Dual Pane)    |
| `file.trash`     | `d`                 | Move to Trash                             |
| `file.delete`    | `D`                 | Permanently delete                        |
| `file.extract`   | `y`                 | Extract a ZIP archive                     |
| `file.compress`  | `z`                 | Compress to ZIP                           |
| `file.open`      | `o`                 | Open with the system default app and quit |
| `file.info`      | `I`                 | Get Info                                  |
| `copy.path`      | none                | Copy the highlighted item's full path     |
| `copy.name`      | none                | Copy its file or folder name              |
| `copy.parent`    | none                | Copy the path of the folder holding it    |
| `search.current` | `/`, `i`            | Search current directory                  |
| `search.root`    | `<leader>/`         | Root search                               |
| `index.manage`   | `<leader>i`         | Choose which folders are indexed          |
| `view.cycle`     | `p`                 | Cycle view mode                           |
| `view.sort`      | `s`                 | Sort options menu                         |
| `view.hidden`    | `.`                 | Toggle hidden files                       |
| `pane.switch`    | `<Tab>`             | Switch active pane                        |
| `help`           | `?`                 | Show keybindings                          |
| `palette`        | `:`                 | Command palette                           |
| `quit`           | `q`                 | Quit                                      |

Key syntax (names inside `<…>` are case-insensitive):

- One character: `"j"`, `"D"` (uppercase means Shift+letter), `"."`, `"?"`, `"/"`.
- Ctrl chords: `"<C-d>"`. Alt chords aren't supported.
- Named keys: `<Up>` `<Down>` `<Left>` `<Right>` `<Enter>` `<Tab>` `<Space>` `<Esc>`
  `<Backspace>` `<PageUp>` `<PageDown>` `<Home>` `<End>`.
- Leader sequences: `"<leader>/"` is the leader, then `/`. The leader defaults to Space
  (`leader = " "` under `[keymap]`).
- Two-key sequences: `"gg"` is `g`, then `g` (also `"g<Down>"`, `"<C-w>j"`). A key bound
  on its own acts at once, so binding `"g"` turns off `"gg"`.

```toml
[keymap]
leader = " "

[keymap.normal]
"x" = "file.trash"          # x also moves to Trash
"d" = "none"                # d does nothing
"<C-d>" = "nav.down"
"<leader>c" = "copy.path"   # Space, then c, copies the highlighted item's path
```

`"none"` removes a default binding, and an action can have several keys. `Ctrl-C` always
quits. Keys inside popups (prompts, the sort menu, text fields, the folder picker, Get
Info, the palette, the index manager and the help) are fixed, so a keymap can never lock
you out of one. Invalid entries never stop ff from starting: they're ignored, named in a
warning at startup and logged to `~/.config/ff/ff.log`.

### Themes

The default theme is OneDark. Custom themes live in `~/.config/ff/themes/<name>.toml`
and are picked with `theme` in `settings.toml`; a name without a file gets one written
with the OneDark colors. A theme file only needs the colors you change.

Each mode has one color under `[statusbar]`, used for its status bar badge, the cursor
mark, its prompt in the top bar and hint row, and its popup:

| Key           | Mode                                     | Default              |
| ------------- | ---------------------------------------- | -------------------- |
| `mode_normal` | Normal                                   | blue `#4fa6ed`       |
| `mode_search` | Search the current folder                | green `#8ebd6b`      |
| `mode_global` | Root search                              | yellow `#e2b86b`     |
| `mode_visual` | Visual select (also tints the selection) | purple `#bf68d9`     |
| `mode_delete` | Trash and permanent delete               | red `#e55561`        |
| `mode_create` | Create                                   | cyan `#48b0bd`       |
| `mode_rename` | Rename                                   | orange `#cc9057`     |
| `mode_sort`   | Sort                                     | foreground `#a0a8b7` |

In `[palette]`, `bright` colors the highlighted name, `muted` sizes and dates,
`light_gray` bar details and the scrollbar, `gray` dotfiles, and `dim` pane separators
and line numbers. `[ui] active_border` and `inactive_border` color pane borders (with
`show_borders = true`), `[statusbar] background` the top bar and status line, `[icons]`
the file-type icons and `[markdown]` the Markdown preview. Code previews use
`syntax_theme`, one of `base16-ocean.dark`, `base16-eighties.dark`, `base16-mocha.dark`,
`base16-ocean.light`, `InspiredGitHub`, `Solarized (dark)` or `Solarized (light)`.

### Image previews

| Terminal  | Protocol | Support            |
| --------- | -------- | ------------------ |
| Kitty     | Kitty    | Full               |
| Ghostty   | Kitty    | Full               |
| iTerm2    | iTerm2   | Full               |
| WezTerm   | iTerm2   | Full               |
| Foot      | Sixel    | Full               |
| Alacritty | none     | Half-block preview |

ff detects the protocol the first time it previews an image.

### Nerd Font icons

ff shows file-type icons from [Nerd Fonts](https://www.nerdfonts.com/), colored by file
type. With `use_nerd_fonts = "auto"` (the default) they're on in every terminal; if you
see boxes or question marks instead, install a Nerd Font or set
`use_nerd_fonts = "never"` for the built-in fallback icons.

```bash
brew install --cask font-jetbrains-mono-nerd-font
```

Then select the font in your terminal:

| Terminal  | Setting                                                                |
| --------- | ---------------------------------------------------------------------- |
| Ghostty   | `font-family = JetBrainsMono Nerd Font` in `~/.config/ghostty/config` |
| iTerm2    | Settings → Profiles → Text → Font                                      |
| Alacritty | `font.normal.family` in `~/.config/alacritty/alacritty.toml`           |
| Kitty     | `font_family` in `~/.config/kitty/kitty.conf`                          |
| WezTerm   | `font` in `~/.wezterm.lua`                                             |

## Troubleshooting

**`ff: command not found`.** After building from source, copy the binary somewhere on
your `PATH`, for example `cp target/release/ff /usr/local/bin/`.

**Enter doesn't change my shell's folder.** That takes the
[shell integration](#shell-integration). Without it, ff can only copy the path, and says
so when it quits. Add `eval "$(ff init zsh)"` to `~/.zshrc` (or the bash line to
`~/.bashrc`) and open a new shell.

**The editor doesn't open.** Check that it's on your `PATH` (`which nvim`, `which code`,
`which zed`); for VS Code on macOS, run "Shell Command: Install 'code' command in PATH"
from its command palette. With the shell integration, run `command ff -e nvim`, since the
`ff` function hands paths to the shell instead.

**A folder is missing from root search.** Root search finds folders under `root_dir`,
leaving out hidden and gitignored folders, `~/Library`, `ignore_directories` (which
includes `node_modules` and `target` by default) and `ignore_paths`. A folder created outside ff
appears after the next background refresh, or right away with `ff --rebuild-cache`.

**Root search is slow or noisy.** Open the index manager (`Space`, then `i`) and switch
off folders you never search; the title shows how much smaller the index gets. Folder
names that repeat everywhere, such as `__pycache__`, are better added to
`ignore_directories`.

## Contributing

Bug reports, feature ideas and pull requests are welcome on
[GitHub](https://github.com/anthonyamaro15/file-finder-rust/issues). Notable changes are
listed in the [changelog](CHANGELOG.md).

## License

MIT; see [LICENSE](LICENSE).

## Acknowledgments

- [Ratatui](https://github.com/ratatui/ratatui) for the terminal UI
- [Syntect](https://github.com/trishume/syntect) for syntax highlighting
- Inspired by [fzf](https://github.com/junegunn/fzf) and
  [lazygit](https://github.com/jesseduffield/lazygit)
