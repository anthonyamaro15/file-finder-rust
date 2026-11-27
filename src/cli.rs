use crate::config::settings::LayoutStyle;
use crate::config::Settings;
use clap::{Parser, Subcommand, ValueEnum};
use log::{debug, warn};
use std::{
    env, io,
    path::{Path, PathBuf},
};

/// File Finder - Terminal-based file navigation and search tool
#[derive(Parser)]
#[command(name = "ff")]
#[command(version)]
#[command(about = "A fast, interactive file finder with editor integration")]
#[command(
    long_about = "File Finder (ff) is a terminal-based file navigation and search tool built in Rust.
It provides an interactive file browser with editor integration, search capabilities, and file preview functionality."
)]
pub struct CliArgs {
    /// Starting directory path
    #[arg(long, short = 's', value_name = "PATH")]
    pub start: Option<PathBuf>,

    /// Theme name or path to theme file
    #[arg(long, short = 't', value_name = "THEME")]
    pub theme: Option<String>,

    /// Editor to use for opening files
    #[arg(long, short = 'e', value_enum)]
    pub editor: Option<Editor>,

    /// Reset configuration to defaults
    #[arg(long)]
    pub reset_config: bool,

    /// Rebuild the folder index of root_dir, in the background
    #[arg(long)]
    pub rebuild_cache: bool,

    /// UI layout style (classic, modern, miller)
    #[arg(long, short = 'l', value_enum)]
    pub layout: Option<LayoutArg>,

    /// Hand the chosen path to the shell integration from `ff init` (through
    /// $FF_RESULT_FILE, or stdout without it) instead of copying it
    #[arg(long)]
    pub print_path: bool,

    /// Optional positional path argument (if --start also provided, --start takes precedence)
    #[arg(value_name = "PATH")]
    pub path: Option<PathBuf>,

    /// Subcommand (e.g., init)
    #[command(subcommand)]
    pub command: Option<Commands>,
}

/// Available subcommands
#[derive(Subcommand, Debug, Clone)]
pub enum Commands {
    /// Generate shell integration script
    Init {
        /// Shell type to generate script for
        #[arg(value_enum)]
        shell: Shell,
    },
    /// Add the shell integration to your shell's config file (zsh or bash)
    Setup,
}

/// Supported shells for init command
#[derive(Debug, Clone, ValueEnum)]
pub enum Shell {
    /// Zsh shell
    Zsh,
    /// Bash shell
    Bash,
}

impl Shell {
    /// Generate the shell integration script
    pub fn generate_script(&self) -> &'static str {
        match self {
            Shell::Zsh | Shell::Bash => {
                r#"# ff shell integration
ff() {
    # Commands that don't open ff run as they are.
    case "${1-}" in
        init|help|setup) command ff "$@"; return ;;
    esac
    local arg
    for arg in "$@"; do
        case "$arg" in
            -h|--help|-V|--version|--reset-config) command ff "$@"; return ;;
        esac
    done

    local tmp result
    tmp=$(mktemp "${TMPDIR:-/tmp}/ff_result.XXXXXX") || return

    # Run ff with print-path mode (writes selected path to $FF_RESULT_FILE)
    FF_RESULT_FILE="$tmp" command ff --print-path "$@"
    local rc=$?
    result=$(cat "$tmp")
    rm -f "$tmp"

    # Nothing chosen (quit, or ff failed): fail, so `ff && nvim .` stops here.
    [[ -n "$result" ]] || return $(( rc ? rc : 1 ))

    if [[ -d "$result" ]]; then
        cd "$result"
        return
    fi
    # A file: copy it with the first clipboard tool that works, or print it.
    # The tools stay running to serve the copy, so they must not hold the caller's output.
    if { command -v pbcopy >/dev/null 2>&1 &&
        printf '%s' "$result" | LC_CTYPE=UTF-8 pbcopy >/dev/null 2>&1; } ||
        { command -v wl-copy >/dev/null 2>&1 &&
            printf '%s' "$result" | wl-copy >/dev/null 2>&1; } ||
        { command -v xclip >/dev/null 2>&1 &&
            printf '%s' "$result" | xclip -selection clipboard >/dev/null 2>&1; }; then
        echo "Path copied: $result"
    else
        echo "$result"
    fi
}"#
            }
        }
    }
}

/// Supported editors
#[derive(Debug, Clone, ValueEnum)]
pub enum Editor {
    Nvim,
    Vscode,
    Zed,
}

impl std::fmt::Display for Editor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Editor::Nvim => write!(f, "nvim"),
            Editor::Vscode => write!(f, "vscode"),
            Editor::Zed => write!(f, "zed"),
        }
    }
}

/// Layout style for CLI
#[derive(Debug, Clone, ValueEnum)]
pub enum LayoutArg {
    /// Classic UI with heavy borders and emoji icons
    Classic,
    /// Modern UI with minimal borders and nerd font icons (default)
    Modern,
    /// Miller columns three-pane layout
    Miller,
}

impl From<LayoutArg> for LayoutStyle {
    fn from(arg: LayoutArg) -> Self {
        match arg {
            LayoutArg::Classic => LayoutStyle::Classic,
            LayoutArg::Modern => LayoutStyle::Modern,
            LayoutArg::Miller => LayoutStyle::Miller,
        }
    }
}

/// Effective configuration after applying precedence rules
/// Precedence: CLI > ENV (future) > settings.toml > built-in defaults
#[derive(Debug, Clone)]
pub struct EffectiveConfig {
    pub start_path: PathBuf,
    pub theme: Option<String>,
    pub editor: Option<Editor>,
    pub layout_style: LayoutStyle,
    /// Print selected path to stdout instead of copying to clipboard
    pub print_path: bool,
    /// Shown once in the UI when the settings start path was unusable
    pub start_notice: Option<String>,
    /// The folder the folder index covers: `root_dir`, whatever folder ff opens in
    pub index_root: PathBuf,
    /// Shown once in the UI when `root_dir` was unusable
    pub index_notice: Option<String>,
}

impl CliArgs {
    /// Get the effective start path with proper precedence
    /// If both positional [path] and --start are provided, prefer --start with warning
    pub fn get_effective_start_path(&self) -> Option<PathBuf> {
        match (&self.start, &self.path) {
            (Some(start_path), Some(pos_path)) => {
                warn!(
                    "Both --start '{}' and positional path '{}' provided. Using --start.",
                    start_path.display(),
                    pos_path.display()
                );
                Some(start_path.clone())
            }
            (Some(start_path), None) => Some(start_path.clone()),
            (None, Some(pos_path)) => Some(pos_path.clone()),
            (None, None) => None,
        }
    }

    /// Parse command line arguments
    pub fn parse_args() -> Self {
        Self::parse()
    }
}

/// Path normalization utilities
pub(crate) mod path_utils {
    use super::*;
    use std::io;

    /// Normalize a path by:
    /// 1. Expanding ~ to home directory
    /// 2. Converting "." to current directory
    /// 3. Canonicalizing the path if it exists
    pub fn normalize_path(path: &Path) -> io::Result<PathBuf> {
        let expanded = expand_home(path)?;
        let absolute = if expanded == Path::new(".") {
            env::current_dir()?
        } else if expanded.is_relative() {
            env::current_dir()?.join(expanded)
        } else {
            expanded
        };

        // Try to canonicalize, but fall back to the absolute path if it fails
        // (e.g., if the path doesn't exist yet)
        absolute.canonicalize().or(Ok(absolute))
    }

    /// Expand ~ to the home directory
    pub(crate) fn expand_home(path: &Path) -> io::Result<PathBuf> {
        if let Some(path_str) = path.to_str() {
            if let Some(remainder) = path_str.strip_prefix("~/") {
                if let Some(home) = dirs::home_dir() {
                    return Ok(home.join(remainder));
                }
            } else if path_str == "~" {
                if let Some(home) = dirs::home_dir() {
                    return Ok(home);
                }
            }
        }
        Ok(path.to_path_buf())
    }
}

/// Pick the start directory. A CLI path that isn't a directory is an error;
/// a settings path that isn't one falls back to `fallback`, with a notice for the UI.
fn resolve_start_path(
    cli_path: Option<PathBuf>,
    settings_path: PathBuf,
    fallback: Option<PathBuf>,
) -> io::Result<(PathBuf, Option<String>)> {
    if let Some(path) = cli_path {
        return match path.metadata() {
            Ok(metadata) if metadata.is_dir() => Ok((path, None)),
            Ok(_) => Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("start path '{}' is not a directory", path.display()),
            )),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Err(io::Error::new(
                e.kind(),
                format!("start path '{}' does not exist", path.display()),
            )),
            Err(e) => Err(io::Error::new(
                e.kind(),
                format!("start path '{}': {}", path.display(), e),
            )),
        };
    }

    if settings_path.is_dir() {
        return Ok((settings_path, None));
    }
    let reason = not_a_folder(&settings_path);
    let fallback = fallback.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            format!(
                "start path '{}' {} and no fallback directory is available",
                settings_path.display(),
                reason
            ),
        )
    })?;
    let notice = format!(
        "Start path '{}' {} — opened {}",
        settings_path.display(),
        reason,
        fallback.display()
    );
    Ok((fallback, Some(notice)))
}

/// Why `path`, which isn't a folder, can't be used.
fn not_a_folder(path: &Path) -> &'static str {
    if path.exists() {
        "is not a directory"
    } else {
        "not found"
    }
}

/// The folder to index: `root_dir`, or `start_path` with a notice for the UI when
/// `root_dir` isn't a folder.
fn resolve_index_root(root_dir: &str, start_path: &Path) -> (PathBuf, Option<String>) {
    let root =
        path_utils::normalize_path(Path::new(root_dir)).unwrap_or_else(|_| PathBuf::from(root_dir));
    if root.is_dir() {
        return (root, None);
    }
    let notice = format!(
        "root_dir '{}' {} — global search covers {}",
        root.display(),
        not_a_folder(&root),
        start_path.display()
    );
    (start_path.to_path_buf(), Some(notice))
}

/// Compute effective configuration by applying precedence rules
/// Precedence: CLI > ENV (future) > settings.toml > built-in defaults
pub fn compute_effective_config(
    cli_args: &CliArgs,
    settings: &Settings,
) -> io::Result<EffectiveConfig> {
    // Determine effective start path
    let cli_path = match cli_args.get_effective_start_path() {
        Some(cli_path) => {
            debug!("Using CLI-provided start path: {}", cli_path.display());
            Some(path_utils::normalize_path(&cli_path)?)
        }
        None => {
            debug!("Using settings start path: {}", settings.start_path);
            None
        }
    };
    let settings_path = path_utils::normalize_path(&PathBuf::from(&settings.start_path))
        .unwrap_or_else(|_| PathBuf::from(&settings.start_path));
    // Settings from 0.3.2 and earlier hold ~/Desktop even where there is none; open home
    // quietly there, as a new install's default does.
    let settings_path = match dirs::home_dir() {
        Some(home) if settings_path == home.join("Desktop") && !settings_path.exists() => home,
        _ => settings_path,
    };
    let fallback = env::current_dir()
        .ok()
        .filter(|dir| dir.is_dir())
        .or_else(dirs::home_dir);
    let (start_path, start_notice) = resolve_start_path(cli_path, settings_path, fallback)?;
    let (index_root, index_notice) = resolve_index_root(&settings.root_dir, &start_path);
    for notice in start_notice.iter().chain(&index_notice) {
        warn!("{}", notice);
    }

    // Determine effective theme (CLI > settings > default)
    let theme = cli_args
        .theme
        .clone()
        .or_else(|| Some("onedark".to_string())); // Default theme if none specified

    // Determine effective editor (CLI > settings > none)
    let editor = cli_args.editor.clone();

    // Determine effective layout style (CLI > settings > default)
    let layout_style = cli_args
        .layout
        .clone()
        .map(LayoutStyle::from)
        .unwrap_or_else(|| settings.layout_style.clone());

    // Print path mode (CLI only, no settings equivalent)
    let print_path = cli_args.print_path;

    debug!(
        "Effective config - Start: {}, Theme: {:?}, Editor: {:?}, Layout: {:?}, PrintPath: {}",
        start_path.display(),
        theme,
        editor,
        layout_style,
        print_path
    );

    Ok(EffectiveConfig {
        start_path,
        theme,
        editor,
        layout_style,
        print_path,
        start_notice,
        index_root,
        index_notice,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_script_uses_private_result_file() {
        for shell in [Shell::Zsh, Shell::Bash] {
            let script = shell.generate_script();
            assert!(script.contains("mktemp"));
            assert!(script.contains(r#"FF_RESULT_FILE="$tmp" command ff --print-path"#));
            assert!(script.contains(r#"rm -f "$tmp""#));
            assert!(!script.contains("/tmp/ff_result"));
        }
    }

    /// Quitting without a choice must fail, so `ff && nvim .` stops; a chosen folder cds;
    /// commands that don't open ff keep their own status. Runs in bash and zsh where present.
    #[cfg(unix)]
    #[test]
    fn shell_function_fails_on_cancel_and_cds_on_a_folder() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join("bin");
        let dest = dir.path().join("dest");
        std::fs::create_dir_all(&bin).unwrap();
        std::fs::create_dir_all(&dest).unwrap();
        // A stand-in ff: writes `dest` as the result only when $PICK is set, and exits
        // with $CODE.
        let stub = bin.join("ff");
        std::fs::write(
            &stub,
            format!(
                "#!/bin/sh\n[ -n \"$PICK\" ] && printf %s '{}' > \"$FF_RESULT_FILE\"\nexit ${{CODE:-0}}\n",
                dest.display()
            ),
        )
        .unwrap();
        std::fs::set_permissions(&stub, std::fs::Permissions::from_mode(0o755)).unwrap();
        let dest = dest.canonicalize().unwrap();

        for (shell, kind) in [("bash", Shell::Bash), ("zsh", Shell::Zsh)] {
            // `set -u` too: the function must not read unset arguments.
            let script = format!(
                "{}\nset -u\nff && echo ran; echo \"cancel=$?\"\nCODE=3 ff; echo \"failed=$?\"\n\
                 ff --version; echo \"version=$?\"\nPICK=1 ff; echo \"pick=$? $(pwd -P)\"\n",
                kind.generate_script()
            );
            let out = match std::process::Command::new(shell)
                .arg("-c")
                .arg(script)
                .env("PATH", format!("{}:/usr/bin:/bin", bin.display()))
                .output()
            {
                Ok(out) => out,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
                Err(e) => panic!("{shell}: {e}"),
            };
            let out = String::from_utf8(out.stdout).unwrap();
            let lines: Vec<&str> = out.lines().collect();

            assert!(!lines.contains(&"ran"), "{shell}: {out}");
            assert!(lines.contains(&"cancel=1"), "{shell}: {out}");
            assert!(lines.contains(&"failed=3"), "{shell}: {out}");
            assert!(lines.contains(&"version=0"), "{shell}: {out}");
            let pick = format!("pick=0 {}", dest.display());
            assert!(lines.contains(&pick.as_str()), "{shell}: {out}");
        }
    }

    /// A chosen file goes to the first clipboard tool that works: a failing wl-copy (X11
    /// with wl-clipboard installed) falls through to xclip, and "Path copied" only shows
    /// when a tool took the path.
    #[cfg(unix)]
    #[test]
    fn shell_function_copies_a_file_with_the_first_tool_that_works() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        let file = dir.path().join("notes.md");
        let clip = dir.path().join("clip");
        let stub = |name: &str, body: String| {
            let path = bin.join(name);
            std::fs::write(&path, format!("#!/bin/sh\n{body}")).unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        };
        stub(
            "ff",
            format!("printf %s '{}' > \"$FF_RESULT_FILE\"\n", file.display()),
        );
        stub("wl-copy", "echo 'no Wayland' >&2\nexit 1\n".into());
        stub(
            "xclip",
            format!("[ -z \"$XCLIP_FAIL\" ] && cat > '{}'\n", clip.display()),
        );
        // Found on this machine's PATH; the function runs with only the stubs and these.
        let which = |tool: &str| {
            std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
                .map(|dir| dir.join(tool))
                .find(|path| path.is_file())
        };
        for tool in ["mktemp", "cat", "rm"] {
            std::os::unix::fs::symlink(which(tool).unwrap(), bin.join(tool)).unwrap();
        }

        let mut ran = 0;
        for (shell, kind, flags) in [
            ("bash", Shell::Bash, &["-c"][..]),
            ("zsh", Shell::Zsh, &["-f", "-c"][..]),
        ] {
            let Some(shell_path) = which(shell) else {
                continue;
            };
            ran += 1;
            let _ = std::fs::remove_file(&clip);
            let script = format!(
                "{}\nff; echo \"rc=$?\"\nexport XCLIP_FAIL=1\nff; echo \"rc=$?\"\n",
                kind.generate_script()
            );
            let out = std::process::Command::new(shell_path)
                .args(flags)
                .arg(script)
                .env("PATH", &bin)
                .output()
                .unwrap();
            let stdout = String::from_utf8(out.stdout).unwrap();
            let stderr = String::from_utf8(out.stderr).unwrap();
            let file = file.display().to_string();
            let copied = format!("Path copied: {file}");
            assert_eq!(
                stdout.lines().collect::<Vec<_>>(),
                [copied.as_str(), "rc=0", file.as_str(), "rc=0"],
                "{shell}: {stdout}{stderr}"
            );
            assert_eq!(std::fs::read_to_string(&clip).unwrap(), file, "{shell}");
            assert_eq!(stderr, "", "{shell}");
        }
        assert!(ran > 0, "no bash or zsh to run the function in");
    }

    #[test]
    fn cli_start_path_missing_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("missing");

        let err = resolve_start_path(Some(missing.clone()), dir.path().into(), None).unwrap_err();

        assert_eq!(
            err.to_string(),
            format!("start path '{}' does not exist", missing.display())
        );
    }

    #[test]
    fn cli_start_path_file_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("file.txt");
        std::fs::write(&file, "").unwrap();

        let err = resolve_start_path(Some(file.clone()), dir.path().into(), None).unwrap_err();

        assert_eq!(
            err.to_string(),
            format!("start path '{}' is not a directory", file.display())
        );
    }

    #[test]
    fn cli_start_path_valid_is_used() {
        let cli_dir = tempfile::tempdir().unwrap();
        let settings_dir = tempfile::tempdir().unwrap();

        let (path, notice) = resolve_start_path(
            Some(cli_dir.path().into()),
            settings_dir.path().into(),
            None,
        )
        .unwrap();

        assert_eq!(path, cli_dir.path());
        assert_eq!(notice, None);
    }

    #[test]
    fn settings_start_path_missing_falls_back_with_notice() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("Desktop");
        let cwd = tempfile::tempdir().unwrap();

        let (path, notice) =
            resolve_start_path(None, missing.clone(), Some(cwd.path().into())).unwrap();

        assert_eq!(path, cwd.path());
        assert_eq!(
            notice.unwrap(),
            format!(
                "Start path '{}' not found — opened {}",
                missing.display(),
                cwd.path().display()
            )
        );
    }

    #[test]
    fn the_index_covers_root_dir_whatever_folder_ff_opens_in() {
        let root = tempfile::tempdir().unwrap();
        let start = tempfile::tempdir().unwrap();
        let opened = tempfile::tempdir().unwrap();
        let settings = Settings {
            root_dir: root.path().to_string_lossy().into_owned(),
            start_path: start.path().to_string_lossy().into_owned(),
            ..Settings::default()
        };
        let canonical = |dir: &tempfile::TempDir| dir.path().canonicalize().unwrap();

        for args in [
            vec!["ff"],
            vec!["ff", opened.path().to_str().unwrap()],
            vec!["ff", "--rebuild-cache", opened.path().to_str().unwrap()],
        ] {
            let cli = CliArgs::try_parse_from(&args).unwrap();
            let config = compute_effective_config(&cli, &settings).unwrap();
            let start_path = if args.len() == 1 {
                canonical(&start)
            } else {
                canonical(&opened)
            };
            assert_eq!(config.start_path, start_path, "{args:?}");
            assert_eq!(config.index_root, canonical(&root), "{args:?}");
            assert_eq!(config.index_notice, None, "{args:?}");
        }
    }

    #[test]
    fn root_dir_that_is_not_a_folder_falls_back_to_the_start_path_with_notice() {
        let dir = tempfile::tempdir().unwrap();
        let start = dir.path().join("start");
        std::fs::create_dir(&start).unwrap();
        let file = dir.path().join("file.txt");
        std::fs::write(&file, "").unwrap();
        let missing = dir.path().join("missing");

        for (root_dir, reason) in [(&missing, "not found"), (&file, "is not a directory")] {
            let (root, notice) = resolve_index_root(root_dir.to_str().unwrap(), &start);
            assert_eq!(root, start);
            assert_eq!(
                notice.unwrap(),
                format!(
                    "root_dir '{}' {reason} — global search covers {}",
                    root_dir
                        .canonicalize()
                        .unwrap_or(root_dir.clone())
                        .display(),
                    start.display()
                )
            );
        }
        // `~` is the home folder, as for the start path.
        let (root, notice) = resolve_index_root("~", &start);
        assert_eq!(
            (root, notice),
            (dirs::home_dir().unwrap().canonicalize().unwrap(), None)
        );
    }

    #[test]
    fn settings_start_path_valid_is_used_without_notice() {
        let settings_dir = tempfile::tempdir().unwrap();
        let cwd = tempfile::tempdir().unwrap();

        let (path, notice) =
            resolve_start_path(None, settings_dir.path().into(), Some(cwd.path().into())).unwrap();

        assert_eq!(path, settings_dir.path());
        assert_eq!(notice, None);
    }
}
