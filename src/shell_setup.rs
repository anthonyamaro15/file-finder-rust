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
    /// From `$SHELL`'s file name; `None` for shells `ff init` doesn't support.
    pub fn detect(shell: Option<&str>, home: &Path, zdotdir: Option<&str>) -> Option<Self> {
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
                rc: home.join(".bashrc"),
            }),
            _ => None,
        }
    }

    /// The user's shell, from this process's environment.
    pub fn current() -> Option<Self> {
        let home = dirs::home_dir()?;
        let shell = std::env::var("SHELL").ok();
        let zdotdir = std::env::var("ZDOTDIR").ok();
        Self::detect(shell.as_deref(), &home, zdotdir.as_deref())
    }

    pub fn init_line(&self) -> String {
        format!("eval \"$(ff init {})\"", self.name)
    }

    /// The config file as shown to the user, with `~` for home.
    pub fn rc_shown(&self) -> String {
        replace_home_with_tilde(&self.rc.to_string_lossy())
    }

    /// Whether the config file already runs `ff init` (an unreadable file counts as no).
    pub fn has_integration(&self) -> bool {
        fs::read_to_string(&self.rc).is_ok_and(|text| runs_ff_init(&text))
    }

    /// Append the `ff init` line unless it's already there; true when it was added.
    pub fn add_integration(&self) -> io::Result<bool> {
        if self.has_integration() {
            return Ok(false);
        }
        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.rc)?;
        // A blank line separates it from what's already there.
        let gap = if file.metadata()?.len() > 0 { "\n" } else { "" };
        write!(
            file,
            "{gap}# ff shell integration: Enter in ff takes your shell to the folder you pick\n{}\n",
            self.init_line()
        )?;
        Ok(true)
    }

    /// What to do after the line was added: the running shell read its config before.
    fn reload_message(&self) -> String {
        let rc = self.rc_shown();
        format!(
            "Added the ff shell integration to {rc}.\n\
             This terminal started before that line existed, so load it first:\n  \
             source {rc}   (or open a new terminal), then run ff."
        )
    }
}

/// Whether a config file's text runs `ff init` on an uncommented line.
fn runs_ff_init(text: &str) -> bool {
    text.lines()
        .map(str::trim_start)
        .any(|line| !line.starts_with('#') && line.contains("ff init"))
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
            let rc = shell.rc_shown();
            println!(
                "{rc} already has the ff shell integration. If Enter doesn't take you to the \
                 folder, run: source {rc}   (or open a new terminal)"
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
        let zsh = ShellConfig::detect(Some("/bin/zsh"), home, None).unwrap();
        assert_eq!((zsh.name, zsh.rc), ("zsh", home.join(".zshrc")));
        let zdot = ShellConfig::detect(Some("/bin/zsh"), home, Some("/home/u/.config/zsh"));
        assert_eq!(zdot.unwrap().rc, Path::new("/home/u/.config/zsh/.zshrc"));
        let empty_zdot = ShellConfig::detect(Some("zsh"), home, Some(""));
        assert_eq!(empty_zdot.unwrap().rc, home.join(".zshrc"));
        let bash = ShellConfig::detect(Some("/opt/homebrew/bin/bash"), home, None).unwrap();
        assert_eq!((bash.name, bash.rc), ("bash", home.join(".bashrc")));
        assert_eq!(ShellConfig::detect(Some("/usr/bin/fish"), home, None), None);
        assert_eq!(ShellConfig::detect(None, home, None), None);
    }

    #[test]
    fn runs_ff_init_ignores_commented_lines() {
        assert!(runs_ff_init("export A=1\n  eval \"$(ff init zsh)\"\n"));
        assert!(!runs_ff_init("# eval \"$(ff init zsh)\"\nexport A=1"));
        assert!(!runs_ff_init(""));
    }

    #[test]
    fn add_integration_appends_once_and_keeps_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let shell = zsh_in(dir.path());
        fs::write(&shell.rc, "export A=1").unwrap();

        assert!(shell.add_integration().unwrap());
        assert!(!shell.add_integration().unwrap());

        let text = fs::read_to_string(&shell.rc).unwrap();
        assert!(text.starts_with("export A=1\n"));
        assert_eq!(text.matches("ff init zsh").count(), 1);
        assert!(shell.has_integration());
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
