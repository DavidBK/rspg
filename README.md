# rspg

A small terminal pager for [pgcli](https://www.pgcli.com/) and psql, written in Rust.
Inspired by [pspg](https://github.com/okbob/pspg).

- Frozen header rows and first column
- Cell cursor with vim-style navigation
- Copy a cell or a whole row to the clipboard
- Search with match highlighting
- Pretty-printed JSON cells
- Open a cell or the whole table in `$EDITOR`

## Install

```sh
cargo install --path .
```

This puts `rspg` in `~/.cargo/bin`.

## Use

With pgcli, add this to `~/.config/pgcli/config`:

```ini
pager = rspg
# don't truncate long values (needed for JSON cells)
max_field_width =
```

Run `\pset pager always` in pgcli to page every result, not only long ones.

Other ways to run it:

```sh
PSQL_PAGER=rspg psql            # psql
psql -c 'select ...' | rspg     # pipe
rspg output.txt                 # file
```

## Keys

| Key | Action |
|---|---|
| `h j k l`, arrows | move one cell |
| `w` / `b` | next / previous column |
| `Ctrl-d` / `Ctrl-u` | half page down / up |
| `Space`, `PgDn` / `PgUp` | page down / up |
| `g` / `G` | first / last row |
| `0` / `$` | first / last column |
| `H` / `L`, `Shift-←/→` | scroll view left / right |
| `Ctrl-e` / `Ctrl-y` | scroll view down / up |
| `y` / `Y` | copy cell / row (row is tab-separated) |
| `f`, `Enter` | cell detail popup (JSON pretty-printed) |
| `e` / `E` | open cell / table in `$VISUAL`, `$EDITOR` or vim |
| `/` / `?` | search forward / backward |
| `n` / `N` | next / previous match |
| `*` / `#` | search for the current cell's value |
| `Esc` | clear search highlight, then quit |
| `q` | quit |

Search is smart-case: an all-lowercase query ignores case. Copying uses
`pbcopy`, `wl-copy`, `xclip` or `xsel`. Changes made in the editor are
discarded.

## Development

```sh
cargo run -- file.txt
cargo test
```
