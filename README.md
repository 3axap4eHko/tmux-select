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
that pane.

## How it works

1. Opens a single tmux control-mode client (`tmux -C attach-session`, held for
   the run), so all reads go over one channel instead of forking `tmux` per
   command.
2. Enumerates the current session's windows and panes in one `list-panes -s`.
3. Classifies agent panes by reading the process table once and climbing each
   `claude`/`codex` process to its owning pane (so agents launched under an
   `npm`/`node`/shell wrapper are still found), restricted to the terminal's
   foreground process group.
4. Captures each agent pane and matches its output to `idle`, `working`, or
   `blocked`.
5. Opens a built-in fuzzy picker (no external dependency) over one entry per
   window, and on selection switches to that window, focusing a blocked pane.

Type to fuzzy-filter; Up / Down (or Ctrl-P / Ctrl-N) move; Enter selects;
Esc / Ctrl-C cancels. Ranking favors matches at word and path boundaries, so
typing `tmux` puts `~/projects/tmux-select` ahead of an incidental mid-word hit.

## State detection

States are read from each agent's on-screen output, anchored to the last handful
of live lines. The transcript above that freezes finished status lines (a
past-tense "Crunched for 6m 41s", a completed workflow), so detection keys only on
shapes that appear while the agent is active or waiting:

- Claude works behind a present-continuous spinner glued to an ellipsis
  ("Channeling…", "Forging…"); when a step ends the line is rewritten in the past
  tense, so the gerund-plus-ellipsis is what marks live work. A running workflow
  shows a `◯ … agents done` footer instead.
- Codex works only while it paints a `◦/• Working` line.
- Claude is `blocked` when its `Enter to select` choice-menu footer is open (the
  attention state).

So:

- `blocked` - Claude is waiting for you at a choice prompt.
- `working` - the agent is actively running.
- `idle` - at a prompt, nothing happening. Anything matching no live marker maps
  here. (Codex currently resolves to idle or working only; its composer footer is
  always shown, so it carries no blocked signal.)

## Requirements

- [tmux](https://github.com/tmux/tmux) (developed against 3.5a) - the only
  runtime dependency; the fuzzy finder is built in (no fzf required)
- Rust (edition 2024) to build

Linux is fully supported. On other platforms the build works and everything
runs, but agent classification falls back to the pane's foreground command name
(the process-tree walk needs `/proc`).

## Install

```sh
cargo install --locked --path .
# or: cargo build --locked --release  (binary at target/release/tmux-select)
```

`--locked` builds against the committed `Cargo.lock` instead of re-resolving
dependencies, so you get the exact versions CI tested.

Then bind it in `~/.tmux.conf`:

```tmux
bind / display-popup -E "tmux-select"
```

Press your prefix then `/` to open the picker.

## Scope

This is v1: window mode, current session. A per-pane rendering mode (one entry
per agent) is planned. Switching across sessions, sending input to agents, and
running as a background monitor are out of scope - it reflects state at the
moment you open it.

## Architecture

A single binary; std-only except [termion](https://crates.io/crates/termion)
(terminal raw mode and key input). No async runtime.

- `src/tmux.rs` - control-mode client: attach with `no-output,ignore-size`, the
  `%begin/%end/%error` block protocol, enumeration, capture, detach by closing
  stdin, and the one-shot post-selection switch.
- `src/process.rs` - one process-table pass mapping each pane to the agent
  running in it by climbing agent processes to their owning pane (Linux `/proc`,
  macOS `ps`), restricted to the foreground process group.
- `src/agent.rs` - per-agent idle/working/blocked screen detectors, keyed on the
  live bottom region of the capture.
- `src/picker.rs` - the in-process picker: a termion render/input loop plus an
  fzy-style affine-gap fuzzy matcher (optimal alignment, O(query x text)).
- `src/main.rs` - orchestration: enumerate, classify, capture, build entries,
  run the picker, switch.

Build and test with `cargo build` and `cargo test`.

## License

Licensed under the Apache License, Version 2.0. Copyright 2026 Ivan Zakharchanka.
See [LICENSE](LICENSE).
