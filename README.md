# tmux-select

An agent-aware tmux window picker. It replaces the usual
`list-windows | fzf | select-window` binding with a picker that annotates each
window with the live state of any AI coding agents (Claude Code, Codex) running
in its panes, so you can see at a glance which window needs your attention and
jump straight to the pane that is blocked.

```
12: ~/projects/api [claude: blocked] [codex: working]
 7: ~/notes
 3: ~/projects/api [claude: idle]
```

It lists the windows of the **current tmux session**, ordered by window index.
Pick one and it switches to it; if a pane there is blocked, it focuses the
first blocked pane so you land exactly where input is needed.

## Requirements

- [tmux](https://github.com/tmux/tmux) (developed against 3.5a) - the only
  runtime dependency; the fuzzy finder is built in (no fzf required)
- Rust (edition 2024) to build from source

Linux is fully supported. On other platforms the build works, but agent
classification falls back to the pane's foreground command name (the
process-tree walk needs `/proc`).

## Install

From crates.io:

```sh
cargo install --locked tmux-select
```

Or from a local checkout:

```sh
cargo install --locked --path .
# or: cargo build --locked --release  (binary at target/release/tmux-select)
```

Then bind it in `~/.tmux.conf`:

```tmux
bind / display-popup -E "tmux-select"
```

Press your prefix then `/` to open the picker. The `-E` flag closes the popup
automatically when the picker exits.

`tmux-select` must run inside a tmux session (it reads `$TMUX`); launching it
from a plain shell exits with an error. The `display-popup` binding always
satisfies this.

## Usage

The picker opens with every window of the current session listed. Type to
fuzzy-filter, move with the arrow keys, press Enter to switch.

### Keys

| Key | Action |
| --- | --- |
| `Enter` | switch to the selected window |
| `Up` / `Ctrl-P` | move selection up |
| `Down` / `Ctrl-N` | move selection down |
| any character | append to the filter query |
| `Backspace` | delete the last query character |
| `Ctrl-U` | clear the query |
| `Esc` / `Ctrl-C` / `Ctrl-G` | cancel without switching |

### Agent states

Each agent pane in a window is annotated as `[<agent>: <state>]`:

- `blocked` - the agent is paused on a prompt or approval and needs input.
  Selecting the window jumps to that pane.
- `working` - the agent is actively running.
- `idle` - the agent is at its prompt with nothing in progress.

### Filtering by state

The fuzzy filter matches against the whole line, including the annotations, so
typing `blocked` narrows the list to exactly the windows needing input. This is
the fastest way to find what needs attention, since windows stay in index order
rather than sorting blocked ones to the top.

## License

Licensed under the Apache License, Version 2.0. Copyright 2026 Ivan Zakharchanka.
See [LICENSE](LICENSE).
