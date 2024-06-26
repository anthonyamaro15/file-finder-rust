## Overview

file-finder-rust is a terminal-based file navigation and search tool actively under development. Designed for developers who frequently work in the terminal, it streamlines the process of navigating the file system and opening projects with popular editors such as Neovim, VSCode, and Zed.

This project is a personal tool that I use daily, and I wanted to share it with other developers who might find it useful. The application provides an intuitive interface for file browsing, with options to navigate through directories and a search feature for quick access to directories.

Important: Currently, only available for macOS.

### Features

- File Navigation: Navigate through your file system using simple keyboard shorcuts:
  - "l" OR ">" to move to the next directory
  - "h" OR "<" to move to the previous directory
  - "d" to delete file or directory
  - "a" to create file or directory
- Editor Integration: Open projects directly in "neovim", "vscoode", or "zed".
  - Example use to open project with vscode: "ff vscode"
- Search: Use the input field for quick searching of directories.
- Configuration: Automatically generates a configuration file at the root path on the first run
  - cache_directory.json: cache json file from all directories on the system
  - settings.json: configuration settings.

### Installation

To install the project, ensure you have Rust and Cargo installed.

1. Clone the repository

```
git clone https://github.com/yourusername/file-finder-rust.git
cd file-finder-rust
```

2. Run the project:

```
cargo run
```

first run will take a few minutes to create the directory cache.

Feedback and Ideas: As this project is actively under development, any feedback or ideas for improvement are greatly appreciated!.

### Issues

If you encounter any bugs or have suggestions for improvements, please create an issue.

- Provide a detailed description of the bug or suggestion, including steps to reproduce the issue if applicable.
