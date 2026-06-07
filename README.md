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

Pick a window and it switches to it; if a pane there is `blocked`, it focuses
that pane. Type to fuzzy-filter, Up/Down (or Ctrl-P/Ctrl-N) to move, Enter to
select, Esc to cancel.

## Requirements

- [tmux](https://github.com/tmux/tmux) (developed against 3.5a) - the only
  runtime dependency; the fuzzy finder is built in (no fzf required)
- Rust (edition 2024) to build

Linux is fully supported. On other platforms the build works, but agent
classification falls back to the pane's foreground command name (the
process-tree walk needs `/proc`).

## Install

```sh
cargo install --locked --path .
# or: cargo build --locked --release  (binary at target/release/tmux-select)
```

Then bind it in `~/.tmux.conf`:

```tmux
bind / display-popup -E "tmux-select"
```

Press your prefix then `/` to open the picker.

## License

Licensed under the Apache License, Version 2.0. Copyright 2026 Ivan Zakharchanka.
See [LICENSE](LICENSE).
