//! Setting up the shell integration for the user: finding the shell's config file,
//! adding the `ff init` line to it, the question at first launch and `ff setup`.

use std::{
    fs,
    io::{self, BufRead, IsTerminal, Write},
    path::{Path, PathBuf},
};

use crate::utils::format::replace_home_with_tilde;

/// A shell `ff init` supports, with the config file that loads the integration.
#[derive(Debug, PartialEq)]
pub struct ShellConfig {
    pub name: &'static str,
    pub rc: PathBuf,
}

impl ShellConfig {
    /// From `$SHELL`'s file name; `None` for shells `ff init` doesn't support. macOS
    /// terminals start bash as a login shell, which reads `~/.bash_profile`, not `~/.bashrc`.
    pub fn detect(
        shell: Option<&str>,
        home: &Path,
        zdotdir: Option<&str>,
        macos: bool,
    ) -> Option<Self> {
        match Path::new(shell?).file_name()?.to_str()? {
            "zsh" => {
                let dir = zdotdir.filter(|dir| !dir.is_empty()).map(Path::new);
                Some(Self {
                    name: "zsh",
                    rc: dir.unwrap_or(home).join(".zshrc"),
                })
            }
            "bash" => Some(Self {
                name: "bash",
                rc: home.join(if macos { ".bash_profile" } else { ".bashrc" }),
            }),
            _ => None,
        }
    }

    /// The user's shell, from this process's environment.
    pub fn current() -> Option<Self> {
        let home = dirs::home_dir()?;
        let shell = std::env::var("SHELL").ok();
        let zdotdir = std::env::var("ZDOTDIR").ok();
        Self::detect(
            shell.as_deref(),
            &home,
            zdotdir.as_deref(),
            cfg!(target_os = "macos"),
        )
    }

    pub fn init_line(&self) -> String {
        format!("eval \"$(ff init {})\"", self.name)
    }

    /// The config file as shown to the user, with `~` for home.
    pub fn rc_shown(&self) -> String {
        replace_home_with_tilde(&self.rc.to_string_lossy())
    }

    /// The command that loads the config file in the running shell.
    pub fn source_command(&self) -> String {
        let shown = self.rc_shown();
        if shown.contains(' ') {
            format!("source \"{}\"", self.rc.display())
        } else {
            format!("source {shown}")
        }
    }

    /// The config file's text; bytes that aren't UTF-8 don't hide the `ff init` line.
    fn read_rc(&self) -> Option<String> {
        let bytes = fs::read(&self.rc).ok()?;
        Some(String::from_utf8_lossy(&bytes).into_owned())
    }

    /// Whether the config file already runs `ff init` (an unreadable file counts as no).
    pub fn has_integration(&self) -> bool {
        self.read_rc().is_some_and(|text| runs_ff_init(&text))
    }

    /// Append the `ff init` line unless it's already there; true when it was added.
    pub fn add_integration(&self) -> io::Result<bool> {
        let text = self.read_rc().unwrap_or_default();
        if runs_ff_init(&text) {
            return Ok(false);
        }
        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.rc)?;
        // A blank line separates it from what's already there.
        let gap = match text.chars().last() {
            None => "",
            Some('\n') => "\n",
            Some(_) => "\n\n",
        };
        write!(
            file,
            "{gap}# ff shell integration: Enter in ff takes your shell to the folder you pick\n{}\n",
            self.init_line()
        )?;
        Ok(true)
    }

    /// What to do after the line was added: the running shell read its config before.
    fn reload_message(&self) -> String {
        format!(
            "Added the ff shell integration to {}.\n\
             This terminal started before that line existed, so load it first:\n  \
             {}   (or open a new terminal), then run ff.",
            self.rc_shown(),
            self.source_command()
        )
    }
}

/// Whether a config file's text runs `ff init zsh` or `ff init bash` outside a comment.
fn runs_ff_init(text: &str) -> bool {
    text.lines().any(|line| {
        let code = line.split('#').next().unwrap_or_default();
        ["ff init zsh", "ff init bash"].iter().any(|call| {
            code.match_indices(call).any(|(at, _)| {
                // `ff` is a command of its own, not the end of a longer word.
                !code[..at]
                    .chars()
                    .last()
                    .is_some_and(|c| c.is_alphanumeric() || "_-.".contains(c))
            })
        })
    })
}

/// An empty answer, `y` or `yes` (any case) accepts.
fn accepts(answer: &str) -> bool {
    matches!(answer.trim().to_lowercase().as_str(), "" | "y" | "yes")
}

/// Records that the user declined, so ff doesn't ask at every launch.
fn declined_marker() -> Option<PathBuf> {
    crate::config::get_config_dir()
        .ok()
        .map(|dir| dir.join("shell_setup_declined"))
}

/// Before the app opens, offer to add the shell integration when ff was started
/// without it. True when ff should exit: the line was added and the shell must load it.
///
/// Asks only in a terminal, for zsh or bash, with no editor set (Enter opens files
/// then), when the config file doesn't run `ff init` yet and the user hasn't declined.
pub fn offer_on_launch(print_path: bool, editor_set: bool) -> bool {
    if print_path || editor_set || !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        return false;
    }
    let (Some(shell), Some(marker)) = (ShellConfig::current(), declined_marker()) else {
        return false;
    };
    if marker.exists() || shell.has_integration() {
        return false;
    }
    ask(&shell, &mut io::stdin().lock(), &mut io::stdout(), &marker).unwrap_or(false)
}

/// The question itself. Yes adds the line and returns true; no writes `marker`;
/// end of input (Ctrl-D) changes nothing, so ff asks again next time.
fn ask(
    shell: &ShellConfig,
    input: &mut impl BufRead,
    output: &mut impl Write,
    marker: &Path,
) -> io::Result<bool> {
    write!(
        output,
        "ff can take your shell into the folder you pick. That needs one line in {}:\n  {}\nAdd it now? [Y/n] ",
        shell.rc_shown(),
        shell.init_line()
    )?;
    output.flush()?;
    let mut answer = String::new();
    if input.read_line(&mut answer)? == 0 {
        writeln!(output)?;
        return Ok(false);
    }
    if !accepts(&answer) {
        if let Some(dir) = marker.parent() {
            fs::create_dir_all(dir)?;
        }
        fs::write(marker, "")?;
        writeln!(output, "Skipped. Run \"ff setup\" any time.")?;
        return Ok(false);
    }
    match shell.add_integration() {
        Ok(_) => {
            writeln!(output, "{}", shell.reload_message())?;
            Ok(true)
        }
        Err(error) => {
            writeln!(
                output,
                "ff: could not write {}: {error}\nAdd this line to it yourself:  {}",
                shell.rc_shown(),
                shell.init_line()
            )?;
            Ok(false)
        }
    }
}

/// `ff setup`: add the shell integration without asking. Returns the exit code.
pub fn run_setup() -> i32 {
    let Some(shell) = ShellConfig::current() else {
        eprintln!(
            "ff: setup supports zsh and bash. In zsh or bash, add one of these to its config:\n  \
             eval \"$(ff init zsh)\"    # ~/.zshrc\n  \
             eval \"$(ff init bash)\"   # ~/.bashrc"
        );
        return 1;
    };
    match shell.add_integration() {
        Ok(true) => {
            println!("{}", shell.reload_message());
            0
        }
        Ok(false) => {
            println!(
                "{} already has the ff shell integration. If Enter doesn't take you to the \
                 folder, run: {}   (or open a new terminal)",
                shell.rc_shown(),
                shell.source_command()
            );
            0
        }
        Err(error) => {
            eprintln!(
                "ff: could not write {}: {error}\nAdd this line to it yourself:  {}",
                shell.rc_shown(),
                shell.init_line()
            );
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn zsh_in(dir: &Path) -> ShellConfig {
        ShellConfig {
            name: "zsh",
            rc: dir.join(".zshrc"),
        }
    }

    #[test]
    fn detect_picks_the_config_file_for_zsh_and_bash_only() {
        let home = Path::new("/home/u");
        let rc = |shell, zdotdir, macos| {
            ShellConfig::detect(Some(shell), home, zdotdir, macos).map(|config| config.rc)
        };
        assert_eq!(rc("/bin/zsh", None, true), Some(home.join(".zshrc")));
        assert_eq!(
            rc("/bin/zsh", Some("/home/u/.config/zsh"), false),
            Some(home.join(".config/zsh/.zshrc"))
        );
        assert_eq!(rc("zsh", Some(""), false), Some(home.join(".zshrc")));
        assert_eq!(rc("/usr/bin/bash", None, false), Some(home.join(".bashrc")));
        assert_eq!(
            rc("/opt/homebrew/bin/bash", None, true),
            Some(home.join(".bash_profile"))
        );
        assert_eq!(rc("/usr/bin/fish", None, true), None);
        assert_eq!(ShellConfig::detect(None, home, None, true), None);
    }

    #[test]
    fn runs_ff_init_ignores_commented_lines() {
        assert!(runs_ff_init("export A=1\n  eval \"$(ff init zsh)\"\n"));
        assert!(runs_ff_init("eval \"$(/opt/homebrew/bin/ff init bash)\""));
        for not_it in [
            "# eval \"$(ff init zsh)\"",
            "export A=1 # ff init zsh later",
            "alias gdi=\"git diff init\"",
            "eval \"$(tiff init zsh)\"",
            "",
        ] {
            assert!(!runs_ff_init(not_it), "{not_it:?}");
        }
    }

    #[test]
    fn add_integration_appends_once_and_keeps_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let shell = zsh_in(dir.path());
        fs::write(&shell.rc, "export A=1").unwrap();

        assert!(shell.add_integration().unwrap());
        assert!(!shell.add_integration().unwrap());

        let text = fs::read_to_string(&shell.rc).unwrap();
        assert!(
            text.starts_with("export A=1\n\n# ff shell integration"),
            "{text:?}"
        );
        assert_eq!(text.matches("ff init zsh").count(), 1);
        assert!(shell.has_integration());
    }

    #[test]
    fn add_integration_sees_the_line_in_a_file_that_is_not_utf8() {
        let dir = tempfile::tempdir().unwrap();
        let shell = zsh_in(dir.path());
        fs::write(&shell.rc, b"export N=caf\xe9\neval \"$(ff init zsh)\"\n").unwrap();

        assert!(shell.has_integration());
        assert!(!shell.add_integration().unwrap());
    }

    #[test]
    fn source_command_quotes_paths_with_spaces() {
        let shell = ShellConfig {
            name: "zsh",
            rc: PathBuf::from("/Volumes/My Disk/.zshrc"),
        };
        assert_eq!(shell.source_command(), "source \"/Volumes/My Disk/.zshrc\"");
    }

    #[test]
    fn add_integration_creates_a_missing_file() {
        let dir = tempfile::tempdir().unwrap();
        let shell = zsh_in(dir.path());
        assert!(!shell.has_integration());
        assert!(shell.add_integration().unwrap());
        let text = fs::read_to_string(&shell.rc).unwrap();
        assert!(text.starts_with("# ff shell integration"), "{text:?}");
        assert!(shell.has_integration());
    }

    #[test]
    fn accepts_enter_and_yes_only() {
        for yes in ["\n", "y\n", "Y", " yes \n"] {
            assert!(accepts(yes), "{yes:?}");
        }
        for no in ["n\n", "no", "nope", "q"] {
            assert!(!accepts(no), "{no:?}");
        }
    }

    #[test]
    fn ask_adds_the_line_on_yes_and_remembers_no() {
        let dir = tempfile::tempdir().unwrap();
        let shell = zsh_in(dir.path());
        let marker = dir.path().join("config").join("shell_setup_declined");
        let mut out = Vec::new();

        // End of input: nothing changes.
        assert!(!ask(&shell, &mut "".as_bytes(), &mut out, &marker).unwrap());
        assert!(!marker.exists() && !shell.rc.exists());

        assert!(!ask(&shell, &mut "n\n".as_bytes(), &mut out, &marker).unwrap());
        assert!(marker.exists() && !shell.rc.exists());

        assert!(ask(&shell, &mut "\n".as_bytes(), &mut out, &marker).unwrap());
        assert!(shell.has_integration());

        let out = String::from_utf8(out).unwrap();
        assert!(out.contains("Add it now? [Y/n]"));
        assert!(out.contains("Skipped. Run \"ff setup\" any time."));
        assert!(out.contains("This terminal started before that line existed"));
    }
}
