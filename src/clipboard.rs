//! Copy text to the system clipboard by piping it into an external tool.

use std::io::Write;
use std::process::{Command, Stdio};

use anyhow::{Result, bail};

/// Tried in order; the first one that is installed and succeeds wins.
/// `&[(&str, &[&str])]` is a slice of tuples: (program, arguments).
const TOOLS: &[(&str, &[&str])] = &[
    ("pbcopy", &[]),                             // macOS
    ("wl-copy", &[]),                            // Wayland
    ("xclip", &["-selection", "clipboard"]),     // X11
    ("xsel", &["--clipboard", "--input"]),       // X11
];

pub fn copy(text: &str) -> Result<()> {
    for (program, args) in TOOLS {
        let spawned = Command::new(program)
            .args(*args)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();

        // Not installed? Try the next tool.
        let Ok(mut child) = spawned else { continue };

        // `take()` moves the pipe out of the child; when `stdin` goes out of
        // scope at the end of this block it is dropped = closed = EOF.
        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(text.as_bytes())?;
        }
        if child.wait()?.success() {
            return Ok(());
        }
    }
    bail!("no clipboard tool found (pbcopy, wl-copy, xclip, xsel)")
}
