# 🚀 Rust File Finder

A blazing-fast, feature-rich terminal file explorer built with Rust. Navigate your file system with ease using fuzzy search, file previews, and powerful keyboard shortcuts.

![File Finder Demo](https://img.shields.io/badge/Platform-MacOS%20%7C%20Linux-blue)
![Language](https://img.shields.io/badge/Language-Rust-orange)
![License](https://img.shields.io/badge/License-MIT-green)

https://github.com/user-attachments/assets/162e767c-9a19-4b2a-a8cc-3683deea1cfe

## ✨ Features

### 🔍 **Advanced Search**

- **Local Fuzzy Search**: Lightning-fast fuzzy matching within current directory
- **Global Search**: Search across entire directory tree (prefix with space ` ` or `/`)
- **Smart Ranking**: Results sorted by relevance with fuzzy matching scores
- **Real-time Filtering**: See results as you type

### 📁 **File Management**

- **Quick Navigation**: Vim-style movement keys (`h`, `j`, `k`, `l`)
- **File Operations**: Create, delete, rename, and copy files/directories
- **Hidden Files**: Toggle visibility of hidden files
- **Sorting Options**: Sort by name, size, or date modified (ASC/DESC)

### 👀 **Rich Previews**

- **Syntax Highlighting**: Code preview with syntax highlighting
- **Image Metadata**: View image dimensions and format information
- **Archive Contents**: Peek inside ZIP files and archives
- **CSV Data**: Preview CSV files in tabular format
- **File Metadata**: Size, permissions, and modification time

### 🎨 **Beautiful UI**

- **OneDark Theme**: Elegant lazygit-inspired color scheme
- **Status Bar**: Real-time file information and navigation hints
- **Progress Indicators**: Visual feedback for long operations
- **Modal Dialogs**: Clean confirmation dialogs with emojis

### ⚡ **Performance**

- **Async Operations**: Non-blocking file operations with progress tracking
- **Parallel Processing**: Multi-threaded file scanning using Rayon
- **Smart Caching**: Directory cache for instant global search
- **File System Watching**: Real-time updates when files change

## 🛠 Installation

### Prerequisites

- Rust 1.70+ installed
- macOS or Linux operating system

### From Source

```bash
git clone https://github.com/your-username/rust-file-finder.git
cd rust-file-finder
cargo build --release
```

### Basic Usage

```bash
# Launch from current directory
ff

# Launch from specific directory
ff ~/Documents
ff /tmp
ff .

# Using command-line options
ff --start ~/Projects --editor nvim
ff -s . -e zed
```

## 🖥️ Command Line Interface

### CLI Options

```bash
ff [OPTIONS] [PATH]
```

| Option              | Short | Description                       | Example              |
| ------------------- | ----- | --------------------------------- | -------------------- |
| `--start <PATH>`    | `-s`  | Starting directory path           | `ff -s ~/Documents`  |
| `--editor <EDITOR>` | `-e`  | Editor to use for opening files   | `ff -e nvim`         |
| `--theme <THEME>`   | `-t`  | Theme name or path to theme file  | `ff -t onedark`      |
| `--reset-config`    |       | Reset configuration to defaults   | `ff --reset-config`  |
| `--rebuild-cache`   |       | Rebuild directory cache           | `ff --rebuild-cache` |
| `--help`            | `-h`  | Show help information             | `ff --help`          |
| `--version`         | `-V`  | Show version                      | `ff --version`       |
| `[PATH]`            |       | Optional positional path argument | `ff ~/Desktop`       |

### Path Examples

#### **Current Directory**

```bash
ff .                    # Current directory
ff --start .           # Same using --start flag
```

#### **Home Directory**

```bash
ff ~                    # Home directory
ff ~/Documents          # Documents folder
ff ~/Desktop            # Desktop folder
ff --start ~/Projects   # Projects folder using --start
```

#### **Absolute Paths**

```bash
ff /tmp                 # Temporary directory
ff /Applications        # Applications folder
ff --start /usr/local   # Using --start with absolute path
```

#### **Relative Paths**

```bash
ff ../                  # Parent directory
ff src/                 # Source directory
ff --start ../other-project  # Relative path with --start
```

### Editor Integration

#### **Supported Editors**

- `nvim` - Neovim
- `vscode` - Visual Studio Code
- `zed` - Zed Editor

#### **Editor Examples**

```bash
# Set editor with current directory
ff --editor nvim
ff -e zed

// (--editor AND -e refer to the same)

# Combine with different starting paths
ff --editor vscode ~/Documents
ff -e nvim --start ~/Projects

# Positional path with editor
ff ~/Desktop --editor zed
```

### Configuration Management

```bash
# Reset all settings to defaults
ff --reset-config

# Rebuild directory cache for faster global search
ff --rebuild-cache

# Check current version
ff --version
ff -V
```

### Advanced CLI Examples

```bash
# Multiple options combined
ff --start ~/Projects --editor nvim --theme onedark

# Path precedence: --start takes priority over positional path
ff ~/Documents --start ~/Desktop  # Opens ~/Desktop (--start wins)

# Tilde expansion works everywhere
ff --start ~/Documents/Projects/my-app

# Current directory shortcuts
ff .                    # Launch in current directory
ff --start . --editor zed  # Current directory with Zed
```

## ⌨️ Keyboard Shortcuts

### 🧭 **Navigation**

| Key       | Action                                   |
| --------- | ---------------------------------------- |
| `↑` / `k` | Move up                                  |
| `↓` / `j` | Move down                                |
| `←` / `h` | Go to parent directory                   |
| `→` / `l` | Enter directory or open file             |
| `Enter`   | Select file (open with IDE or copy path) |

### 🔍 **Search**

| Key            | Action                                     |
| -------------- | ------------------------------------------ |
| `i`            | Enter search mode                          |
| `Esc`          | Exit search mode                           |
| `Space` or `/` | Start global search (searches entire tree) |
| Regular text   | Local fuzzy search in current directory    |

### 📁 **File Operations**

| Key | Action                                   |
| --- | ---------------------------------------- |
| `a` | Create new file/directory                |
| `d` | Delete selected item (with confirmation) |
| `r` | Rename selected item                     |
| `c` | Copy file/directory                      |
| `.` | Toggle hidden files                      |

### 🔧 **Tools**

| Key | Action                |
| --- | --------------------- |
| `s` | Sort options menu     |
| `?` | Show keybindings help |
| `q` | Quit application      |

### 📋 **Sort Options** (when in sort mode)

| Key | Action               |
| --- | -------------------- |
| `n` | Sort by name         |
| `s` | Sort by size         |
| `t` | Sort by date created |
| `a` | Ascending order      |
| `d` | Descending order     |
| `q` | Exit sort mode       |

## 🎯 Usage Examples

### Basic File Navigation

```bash
# Start the file finder
cargo run

# Use arrow keys or vim keys (hjkl) to navigate
# Press 'l' or Enter to open directories
# Press 'h' to go back to parent directory
```

### Search Examples

```bash
# Local search (in current directory)
i → type "config" → see matching files in current folder

# Global search (entire directory tree)
i → type " config" → search for "config" across all subdirectories
i → type "/main.rs" → find all main.rs files in the project
```

### File Operations

```bash
# Create a new file
a → type "new_file.txt" → Enter

# Create a new directory
a → type "new_folder" → Enter

# Delete with confirmation
d → y (confirm) or n (cancel)

# Rename file
r → edit name → Enter

# Copy file/directory
c → file copied with "copy_" prefix
```

### IDE Integration

```bash
# Open files with your preferred editor
cargo run nvim    # Files open in Neovim
cargo run code    # Files open in VS Code
cargo run zed     # Files open in Zed

# Without IDE: file paths are copied to clipboard
cargo run         # Selected file path copied to clipboard
```

## 🎨 Theme & Customization

The tool features a beautiful OneDark theme inspired by lazygit with:

- **Syntax highlighting** for code previews
- **Color-coded file types** and statuses
- **Themed modal dialogs** with appropriate colors:
  - 🔴 Red for delete confirmations
  - 🟢 Green for create operations
  - 🟡 Yellow for rename operations
  - 🔵 Blue for informational dialogs

## 🚧 File Support

### Preview Support

- **Source Code**: `.rs`, `.js`, `.py`, `.go`, `.java`, `.cpp`, etc.
- **Markup**: `.md`, `.html`, `.xml`, `.json`, `.yaml`, etc.
- **Images**: `.png`, `.jpg`, `.gif`, `.bmp` (metadata only)
- **Archives**: `.zip`, `.tar`, `.gz` (contents listing)
- **Data**: `.csv`, `.txt` files

### File Operations

- **Text files**: Full create, read, update, delete
- **Directories**: Create, rename, delete (recursive)
- **Binary files**: Copy, move, delete (preview as metadata)

## ⚙️ Configuration

### Configuration Precedence

The file finder uses a layered configuration system with the following precedence (highest to lowest):

1. **CLI Arguments** (highest priority)
2. **Environment Variables** (future feature)
3. **Configuration File** (`~/.config/ff/settings.toml`)
4. **Built-in Defaults** (lowest priority)

#### **Example Precedence**

```bash
# If settings.toml has start_path = "/Users/user/Documents"
# But you run: ff --start ~/Projects
# Result: Opens ~/Projects (CLI overrides settings file)
```

### Configuration Files

#### **Settings Location**

- **Settings**: `~/.config/ff/settings.toml`
- **Theme**: `~/.config/ff/themes/onedark.toml`
- **Cache**: `~/.config/ff/cache_directory.json`

#### **Reset Configuration**

```bash
# Reset all settings to defaults
ff --reset-config
```

### Default Settings

- **Start Path**: Current working directory
- **Hidden Files**: Hidden by default (toggle with `.`)
- **Sort Order**: Alphabetical ascending
- **Cache**: Automatically builds directory cache for fast global search
- **Theme**: OneDark theme

### IDE Integration

```bash
# Supported IDEs (set via CLI or config)
nvim     # Neovim
vscode   # Visual Studio Code
zed      # Zed Editor

# Without IDE integration
# File paths are automatically copied to clipboard
```

## 🔧 Advanced Features

### Global Search

- Start search with space or `/` to search across entire directory tree
- Uses fuzzy matching with intelligent scoring
- Results ranked by relevance
- Instantly navigates to files anywhere in your project

### Copy Progress

- Real-time progress bars for copy operations
- Async copying with cancellation support
- Batch progress updates for performance
- Error handling with user-friendly messages

### File System Watching

- Automatically detects file system changes
- Real-time UI updates when files are added/removed
- Smart refresh to avoid performance impact

## 🐛 Troubleshooting

### CLI Issues

**Command not found?**

```bash
# Make sure the binary is in your PATH or use full path
./target/release/file-finder --help

# Or create an alias
alias ff='./target/release/file-finder'
```

**Path not working?**

```bash
# Use quotes for paths with spaces
ff --start "~/My Documents"

# Verify tilde expansion
ff --start ~/Documents  # ✓ Works
ff --start ~Documents   # ❌ Won't work
```

**Editor not opening files?**

```bash
# Check if editor is in PATH
which nvim   # Should show path to nvim
which code   # Should show path to code
which zed    # Should show path to zed

# For VS Code on macOS, you might need to install shell command:
# Open VS Code → Command Palette (⇧⌘P) → "Shell Command: Install 'code' command in PATH"
```

**Configuration not persisting?**

```bash
# Reset configuration if corrupted
ff --reset-config

# Check configuration directory exists
ls ~/.config/ff/
```

### Common Issues

**Search not working?**

- Make sure you're in search mode (press `i`)
- For global search, prefix with space ` ` or `/`

**Files not opening in IDE?**

- Check that your IDE is installed and in PATH
- Use exact CLI editor names: `nvim`, `vscode`, `zed`
- Remember to select a file first (arrow keys), then press Enter

**Performance issues?**

- Large directories may take time to cache initially
- Use local search for faster results in current directory
- Hidden files toggle (`.`) can improve performance

**Permission errors?**

- Ensure you have read permissions for directories
- Some system directories may not be accessible

## 🤝 Contributing

We welcome contributions! Please feel free to:

- Report bugs and issues
- Suggest new features
- Submit pull requests
- Improve documentation

## 📄 License

This project is licensed under the MIT License - see the LICENSE file for details.

## 🙏 Acknowledgments

- Built with [Ratatui](https://github.com/ratatui-org/ratatui) for terminal UI
- Uses [Syntect](https://github.com/thecodewarrior/syntect) for syntax highlighting
- Inspired by tools like [fzf](https://github.com/junegunn/fzf) and [lazygit](https://github.com/jesseduffield/lazygit)
- OneDark theme adaptation from the popular editor theme

---

**Happy file exploring! 🎉**

_Built with ❤️ in Rust_
