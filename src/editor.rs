//! Open text in an external editor ($VISUAL / $EDITOR / vim).
//!
//! The text goes into a temp file which is deleted afterwards — edits are
//! discarded on purpose (a pager can't write back to the database).

use std::env;
use std::fs::File;
use std::io::{self, Write};
use std::process::Command;

use anyhow::{Context, Result, bail};
use ratatui::DefaultTerminal;
use ratatui::crossterm::{execute, terminal};

/// What to open; built by the app, executed by the main loop.
pub struct EditRequest {
    pub text: String,
    /// File extension (".json" gives syntax highlighting in vim).
    pub suffix: &'static str,
    /// 1-based line to put the cursor on.
    pub line: Option<usize>,
}

pub fn open(terminal: &mut DefaultTerminal, req: &EditRequest) -> Result<()> {
    // NamedTempFile deletes the file when it is dropped (RAII) — even if we
    // return early with an error.
    let mut file = tempfile::Builder::new()
        .prefix("rspg-")
        .suffix(req.suffix)
        .tempfile()
        .context("cannot create temp file")?;
    file.write_all(req.text.as_bytes())?;
    file.flush()?;

    // Give the terminal back to the normal screen, run the editor, and
    // always take it back afterwards — whether the editor worked or not.
    ratatui::restore();
    let result = run_editor(file.path(), req.line);
    resume(terminal)?;
    result
}

fn run_editor(path: &std::path::Path, line: Option<usize>) -> Result<()> {
    let editor = env::var("VISUAL")
        .or_else(|_| env::var("EDITOR"))
        .unwrap_or_else(|_| "vim".to_string());

    // $EDITOR may contain arguments, e.g. "code --wait".
    let mut parts = editor.split_whitespace();
    let Some(program) = parts.next() else { bail!("$EDITOR is empty") };

    // Our stdin is the pipe from pgcli, so hand the editor the real terminal.
    let tty = File::options().read(true).write(true).open("/dev/tty")?;

    let status = Command::new(program)
        .args(parts)
        .args(line.map(|n| format!("+{n}"))) // Option is iterable: 0 or 1 arg
        .arg(path)
        .stdin(tty.try_clone()?)
        .stdout(tty.try_clone()?)
        .stderr(tty)
        .status()
        .with_context(|| format!("cannot run editor `{program}`"))?;

    if !status.success() {
        bail!("{program} exited with {status}");
    }
    Ok(())
}

/// The reverse of `ratatui::restore()`.
fn resume(terminal: &mut DefaultTerminal) -> io::Result<()> {
    terminal::enable_raw_mode()?;
    execute!(io::stdout(), terminal::EnterAlternateScreen)?;
    // Forget what ratatui thinks is on screen, so the next draw repaints all.
    terminal.clear()
}
