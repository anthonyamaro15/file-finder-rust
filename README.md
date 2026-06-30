# 🚀 Rust File Finder

A blazing-fast, developer-focused terminal file explorer built with Rust.  
Navigate your projects with ease using fuzzy search, file previews, and powerful keyboard shortcuts.

![File Finder Demo](https://img.shields.io/badge/Platform-MacOS%20%7C%20Linux-blue)
![Language](https://img.shields.io/badge/Language-Rust-orange)
![License](https://img.shields.io/badge/License-MIT-green)

---

## ✨ Features

### 🔍 **Search**

- Local fuzzy search in current directory
- Root/global search across the directory cache
- Configurable search keybindings
- Smart ranking & real-time filtering

### 📁 **File Management**

- Vim-style navigation (`h`, `j`, `k`, `l`)
- Create, delete, rename, and copy files/directories
- Toggle hidden files
- Sort by name, size, or modified date

### 🖼️ **View Modes**

- **Normal**: File list + preview pane (50/50 split)
- **FullList**: Full-width file list (no preview)
- **DualPane**: Side-by-side file management with independent navigation
- Press `p` to cycle through modes
- `Tab` to switch active pane (highlighted border)
- Full navigation in both panes (`j`/`k`/`h`/`l`)
- Bidirectional copy/move between panes (`Shift+C` / `Shift+M`)

### 👀 **Previews**

- Syntax highlighting for code (configurable theme)
- **Terminal image rendering** (Kitty, iTerm2, Sixel protocols)
- **PDF text extraction**
- **Hex view for binary files**
- CSV preview in tabular format
- Archive contents (ZIP, TAR, TAR.GZ, TGZ)
- File metadata (size, permissions, mtime)

### 📊 **File Size Visualization**

- Visual size bars next to files
- Color coding: green (small), yellow (medium), red (large)
- Configurable via settings

### 🎨 **UI & Theming**

- OneDark theme (lazygit-inspired)
- Rounded borders and polished visual hierarchy
- Status bar with mode indicators
- Modal dialogs for confirmations
- Custom themes via TOML
- Configurable syntax highlighting theme

### ⚡ **Performance**

- Async, non-blocking operations
- Parallel scanning with Rayon
- Smart caching for instant global search
- File system watching for real-time updates
- Lazy file metadata loading
- Preview caching

---

## 🛠 Installation

### Homebrew (macOS/Linux)

```bash
brew install anthonyamaro15/tap/ff
```

Then add shell integration to your `~/.zshrc` or `~/.bashrc`:

```bash
eval "$(ff init zsh)"   # For zsh
eval "$(ff init bash)"  # For bash
```

### From Source

Prerequisites: Rust 1.70+, macOS or Linux

```bash
git clone https://github.com/anthonyamaro15/file-finder-rust.git
cd file-finder-rust
cargo build --release
cp target/release/ff /usr/local/bin/
```

---

## 🚀 Usage

### Basic

```bash
ff               # Launch in current directory
ff ~/Documents   # Launch in specific directory
```

### CLI Options

```bash
ff [OPTIONS] [PATH] [COMMAND]
```

| Option              | Short | Description                          | Example              |
| ------------------- | ----- | ------------------------------------ | -------------------- |
| `--start <PATH>`    | `-s`  | Starting directory path              | `ff -s ~/Documents`  |
| `--editor <EDITOR>` | `-e`  | Open file with editor                | `ff -e nvim`         |
| `--theme <THEME>`   | `-t`  | Theme name or theme file             | `ff -t onedark`      |
| `--print-path`      |       | Print path to stdout (shell cd)      | `ff --print-path`    |
| `--reset-config`    |       | Reset config to defaults             | `ff --reset-config`  |
| `--rebuild-cache`   |       | Rebuild directory cache              | `ff --rebuild-cache` |
| `--help`            | `-h`  | Show help                            | `ff --help`          |
| `--version`         | `-V`  | Show version                         | `ff --version`       |

| Command             | Description                          | Example              |
| ------------------- | ------------------------------------ | -------------------- |
| `init <SHELL>`      | Generate shell integration script    | `ff init zsh`        |

### Path Examples

```bash
ff .                # Current directory
ff ~/Projects       # Home subdir
ff /tmp             # Absolute path
ff ../src           # Relative path
```

### Editor Integration

```bash
# Supported editors: nvim, vscode, zed
ff -e nvim
ff -e zed --start ~/Projects
ff ~/Desktop -e vscode
```

If no editor is set, the selected file path is copied to clipboard.

### Shell Integration (cd on exit)

Want to `cd` directly to a selected directory? Add one line to your shell config:

**Zsh** (`~/.zshrc`):
```bash
eval "$(ff init zsh)"
```

**Bash** (`~/.bashrc`):
```bash
eval "$(ff init bash)"
```

Then reload your shell (`source ~/.zshrc` or `source ~/.bashrc`).

Now when you press Enter on a directory, your shell automatically `cd`s there:

```bash
ff              # Search for project
# Select directory, press Enter
pwd             # You're now in that directory!
```

For files, the path is copied to your clipboard.

---

## ⌨️ Keyboard Shortcuts

### 🧭 Navigation

| Key       | Action                 |
| --------- | ---------------------- |
| `↑` / `k` | Move up                |
| `↓` / `j` | Move down              |
| `←` / `h` | Parent directory       |
| `→` / `l` | Enter/open             |
| `Enter`   | Open file or copy path |

### 🔍 Search

| Key            | Action                      |
| -------------- | --------------------------- |
| `/` or `i`     | Search current directory    |
| `Esc`          | Exit search mode            |
| `Space` + `/`  | Root search (entire cache)  |

The `?` help popup reflects configured search keybindings.

### 📁 File Operations

| Key | Action                     |
| --- | -------------------------- |
| `a` | Create file/directory      |
| `d` | Delete (with confirmation) |
| `r` | Rename                     |
| `c` | Copy                       |
| `o` | Open in file explorer      |
| `.` | Toggle hidden files        |

### 🖼️ View Modes

| Key       | Action                                           |
| --------- | ------------------------------------------------ |
| `p`       | Cycle view mode (Normal → Full → Dual)           |
| `Tab`     | Switch active pane (Dual Pane mode)              |
| `Shift+C` | Copy from active pane to other pane              |
| `Shift+M` | Move from active pane to other pane              |

In Dual Pane mode, navigation keys (`j`/`k`/`h`/`l`) work on the active pane.

### 🔧 Tools

| Key | Action            |
| --- | ----------------- |
| `s` | Sort options menu |
| `?` | Show keybindings  |
| `q` | Quit              |

---

## 🎨 Theme & Customization

- Default UI theme: **OneDark**
- Custom themes: `~/.config/ff/themes/*.toml`
- Syntax highlighting themes: configurable in settings (default: `base16-ocean.dark`)
- Example:

```bash
ff -t onedark
```

### Syntax Theme

Configure in `~/.config/ff/settings.toml`:

```toml
syntax_theme = "base16-ocean.dark"
```

Colors are used for file types, syntax highlighting, and modal dialogs (🟢 create, 🔴 delete, 🟡 rename, 🔵 info).

### Keybindings

Search keybindings can be configured in `~/.config/ff/settings.toml`.

Default bindings:

```toml
[keymap]
leader = " "

[keymap.normal]
"/" = "search.current"
"i" = "search.current"
"<leader>/" = "search.root"
```

Example with comma as leader and `f` for current-directory search:

```toml
[keymap]
leader = ","

[keymap.normal]
"f" = "search.current"
"/" = "search.root"
"<leader>/" = "search.root"
```

Open `?` inside ff to see the active search bindings.

### Terminal Image Support

Image preview uses terminal graphics protocols when available:

| Terminal  | Protocol | Status              |
| --------- | -------- | ------------------- |
| Kitty     | Kitty    | Full support        |
| iTerm2    | iTerm2   | Full support (Mac)  |
| WezTerm   | iTerm2   | Full support        |
| Ghostty   | Kitty    | Full support        |
| Foot      | Sixel    | Full support        |
| Alacritty | -        | Halfblock fallback  |

### Nerd Font Icons

For the best visual experience, ff supports [Nerd Fonts](https://www.nerdfonts.com/) - patched fonts with file-type icons. Icons are colored based on file type (Rust=orange, JS=yellow, directories=blue, etc.).

**Auto-detection**: ff automatically detects these terminals and enables nerd font icons:
- Ghostty, iTerm2, Kitty, WezTerm, Alacritty, Hyper, VS Code, Tabby, Warp

**Installation**:

1. Install a Nerd Font:
```bash
# macOS (Homebrew)
brew install --cask font-jetbrains-mono-nerd-font

# Or other popular options:
brew install --cask font-fira-code-nerd-font
brew install --cask font-hack-nerd-font
```

Or download from [nerdfonts.com](https://www.nerdfonts.com/)

2. Configure your terminal to use it:

| Terminal  | Configuration |
| --------- | ------------- |
| Ghostty   | `font-family = JetBrainsMono Nerd Font` in `~/.config/ghostty/config` |
| iTerm2    | Preferences → Profiles → Text → Font |
| Alacritty | `font.normal.family` in `~/.config/alacritty/alacritty.toml` |
| Kitty     | `font_family` in `~/.config/kitty/kitty.conf` |
| WezTerm   | `font` in `~/.wezterm.lua` |

3. Restart your terminal

**Manual override** (in `~/.config/ff/settings.toml`):
```toml
use_nerd_fonts = "always"  # Force nerd fonts
use_nerd_fonts = "never"   # Force emoji fallback (📁, 📄)
use_nerd_fonts = "auto"    # Auto-detect (default)
```

---

## ⚙️ Configuration

- **Config file**: `~/.config/ff/settings.toml`
- **Cache**: `~/.config/ff/cache_directory.json`
- **Themes**: `~/.config/ff/themes/`

`ff` stores active TOML settings under `~/.config/ff/settings.toml`.
Legacy JSON configuration is compatibility-only and should not be used for new settings.

### Precedence

1. CLI arguments
2. Config file
3. Built-in defaults

Example:

```bash
# Config file: start_path = "~/Documents"
ff --start ~/Projects   # CLI wins, opens ~/Projects
```

Reset configuration:

```bash
ff --reset-config
```

---

## 🐛 Troubleshooting

**Command not found?**

If installed from source, make sure the binary is in your PATH:
```bash
cp target/release/ff /usr/local/bin/
```

**Editor not opening?**

- Ensure editor is in PATH (`which nvim`, `which code`, `which zed`)
- For VS Code on macOS: install `code` command in PATH via Command Palette

**Search not working?**

- Press `/` or `i` to search the current directory
- Press `Space` + `/` to search from the root cache
- Press `?` to confirm your active search keybindings

**Performance issues?**

- First cache build may take time on large dirs
- Toggle hidden files (`.`) to speed things up

---

## 🤝 Contributing

- Report bugs
- Suggest features
- Submit PRs
- Improve docs

---

## 📄 License

MIT License — see [LICENSE](LICENSE)

---

## 🙏 Acknowledgments

- [Ratatui](https://github.com/ratatui-org/ratatui) — Terminal UI
- [Syntect](https://github.com/thecodewarrior/syntect) — Syntax highlighting
- Inspired by [fzf](https://github.com/junegunn/fzf) and [lazygit](https://github.com/jesseduffield/lazygit)

---

**Happy file exploring! 🎉**  
_Built with ❤️ in Rust_
