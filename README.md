# rspg

A small terminal pager for [pgcli](https://www.pgcli.com/) and psql, written in Rust.
Inspired by [pspg](https://github.com/okbob/pspg).

- Frozen header rows and first column
- Cell cursor with vim-style navigation (`j/k/h/l`) or arrow keys
- Copy a cell or row to the clipboard (`y/Y`)
- Search with match highlighting (`/ or ?`, `n/N` to navigate)
- Pretty-printed JSON or long cells (`f`)
- Open a cell or the whole table in `$EDITOR` (`e`)

## Install

```sh
cargo install --path .
```

This puts `rspg` in `~/.cargo/bin`. Make sure that directory is on your `PATH`.

## Use

With pgcli, add this to pgcli config `main` section (`$XDG_CONFIG_HOME/pgcli/config`):

```ini
[main]
pager = rspg
# don't truncate long values (for long JSON cells)
max_field_width =
```

Other ways to run it:

```sh
PSQL_PAGER=rspg psql            # psql
psql -c 'select ...' | rspg     # pipe
rspg output.txt                 # file
```

## Keybindings

vim-style navigation. In the future, this will be configurable.
All the keys are shown in the button help bar

## License

[GPL-3.0-or-later](LICENSE)
