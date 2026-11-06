//! rspg — a tiny pager for pgcli / psql, written to learn Rust.
//!
//! Usage:
//!   psql -c 'select * from pg_class' | rspg
//!   rspg some_output.txt
//!   (in pgcli)  \pager rspg   or   PAGER=rspg pgcli

// `mod app;` tells the compiler "there is a module in src/app.rs".
// Modules are Rust's unit of code organisation (like a file/namespace).
mod app;
mod cell;
mod clipboard;
mod editor;
mod keys;
mod table;

use std::fs;
use std::io::{self, IsTerminal, Read};
use std::path::PathBuf;

// `anyhow::Result<T>` is shorthand for `Result<T, anyhow::Error>`.
// It lets us use `?` on almost any error type without defining our own.
use anyhow::{Context, Result, bail};
use clap::Parser;

use crate::app::App;
use crate::table::Table;

/// Command line arguments. `#[derive(Parser)]` generates the parsing
/// code for us at compile time (a *procedural macro*).
#[derive(Parser, Debug)]
#[command(version, about = "A simple pager for tabular data")]
struct Args {
    /// File to display. If omitted, reads from stdin.
    file: Option<PathBuf>,
}

fn main() -> Result<()> {
    let args = Args::parse();

    // `match` on an `Option` forces us to handle both cases — no nulls in Rust.
    let text = match &args.file {
        Some(path) => fs::read_to_string(path)
            .with_context(|| format!("cannot read {}", path.display()))?,
        None => read_stdin()?,
    };

    // Nothing to page (e.g. the query failed): exit quietly, like `less -F`.
    if text.trim().is_empty() {
        return Ok(());
    }

    // `&text` lends the String to `parse`; Table makes its own owned copies
    // of the lines, so `text` can be dropped afterwards.
    let table = Table::parse(&text);
    let mut app = App::new(table);

    // ratatui::init() switches to the alternate screen + raw mode and
    // installs a panic hook that restores the terminal if we crash.
    // Keyboard events are read from /dev/tty, so this works even though
    // stdin is the pipe with our data.
    let mut terminal = ratatui::init();
    let result = app.run(&mut terminal);
    ratatui::restore();

    result
}

fn read_stdin() -> Result<String> {
    let mut stdin = io::stdin();
    if stdin.is_terminal() {
        // Nobody piped anything in; reading would just wait forever.
        bail!("no input: pipe data into rspg or pass a file name");
    }
    let mut buf = String::new();
    stdin.read_to_string(&mut buf).context("cannot read stdin")?;
    Ok(buf)
}
