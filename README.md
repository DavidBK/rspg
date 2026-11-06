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

With pgcli, add this to pgcli config (`$xdg_home/pgcli/config`)

```ini
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

rspg is free software: you can redistribute it and/or modify it under the
terms of the [GNU General Public License](LICENSE) as published by the Free
Software Foundation, either version 3 of the License, or (at your option) any
later version.
