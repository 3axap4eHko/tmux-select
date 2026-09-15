# tmux-select

An agent-aware tmux window picker. It replaces the usual
`list-windows | fzf | select-window` binding with a picker that annotates each
window with the detected state of any AI coding agents (Claude Code, Codex, Kimi
Code, Pi, OpenCode, Qwen Code, Grok Build, and Muse Code) running in its panes,
so you can see at a glance which window needs your attention and jump straight
to the pane that is blocked.

https://github.com/user-attachments/assets/74f50ead-c774-462d-a619-862d38bb2888

It lists the windows of the **current tmux session**, initially in window-index order.
Pick one and it switches to it; if a pane there is blocked, it focuses the
first blocked pane so you land exactly where input is needed.

## Requirements

- [tmux](https://github.com/tmux/tmux) (developed against 3.5a) - the only
  runtime dependency; the fuzzy finder is built in (no fzf required)
- Rust (edition 2024) to build from source

Linux and macOS are fully supported. On other platforms the build works,
but agent classification falls back to the pane's foreground command name
(the process-tree walk needs `/proc` or libproc).

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

The picker opens with every window of the current session listed and the active
window preselected. Type to fuzzy-filter, move with the arrow keys, press Enter
to switch.

### Status line

Add the status command to your tmux configuration:

```tmux
set -g status-right "#(tmux-select status -t '#{session_id}')"
set -g status-right-length 80
set -g status-interval 5
```

This replaces the right-hand status text. To keep existing content, insert
`#(tmux-select status -t '#{session_id}')` into your existing `status-right` value instead.
The executable must be available on the tmux server's PATH.

`tmux-select status` prints one line of agent-pane counts for the current session,
ordered as `working: 2 | blocked: 1 | idle: 3`. Zero counts are omitted; no detected
agents produces an empty line. Each invocation captures fresh states and exits
without opening the picker or changing focus. tmux refreshes the command output
at its status interval. Idle means no recognized working or blocked indicator,
not necessarily a completed task. Failures produce stderr diagnostics and a
nonzero exit code.

Status-line jobs have no session ID in their environment. The `-t` argument
supplies the session whose status line is being drawn and must be a tmux session
ID such as `$0`. Inside a pane, `tmux-select status` can omit `-t` and use the
inherited session. The tmux server socket still comes from `$TMUX`.

Both modes share one collection pass: one tmux control connection, one pane
enumeration, one process scan, and captures only for detected agent panes.
Captures reuse that connection without spawning a tmux process per pane.
Status output aggregates counts without the picker's window grouping or sorting.
Capture failures abort collection in both modes instead of reporting an idle agent.

#### Catppuccin badge

For an existing Catppuccin setup, add this after the theme loads and replace your
final `status-left` assignment. Adjust the `source` path to your plugin location.

```tmux
set-environment -gh MODULE_NAME agents

set -g @catppuccin_agents_icon "AI "
set -gF @catppuccin_agents_color "#{E:@thm_mauve}"
set -g @catppuccin_agents_text " #(tmux-select status -t '#{session_id}')"

source ~/.config/tmux/plugins/tmux/utils/status_module.conf

set -g status-left "#{E:@catppuccin_status_session}#{E:@catppuccin_status_agents}"
set -g status-left-length 100
set -g status-interval 5
```

Use `set-environment -gh` here: `%hidden MODULE_NAME="agents"` assigns the value
during parsing, so theme loading can overwrite it before the module builder runs.
The badge supplies its own colors; raw text appended after the session module
can inherit identical foreground and background colors and become invisible.
The `AI` label remains visible when there are no detected agents.

Reload your tmux configuration after saving. The module format follows
[Catppuccin's custom status module API](https://github.com/catppuccin/tmux/blob/9d21d7ccd50df82bd732be2850ce2798e78b6391/docs/tutorials/02-custom-status.md).

### Window names and renaming

Windows with `automatic-rename` off show their window name instead of the active
pane's directory. Renaming a window in tmux turns this option off for that window,
so its assigned name appears and is searchable the next time the picker opens.
Windows with automatic renaming on continue to show their directory. Disabling
automatic renaming globally makes all windows inheriting that setting show names.
The label is read in the existing batched pane query, with no saved picker state.

Press `Ctrl-R` to rename the highlighted window. Its current window name is
prefilled; type or use Backspace to edit, or `Ctrl-U` to clear it. Enter saves and
returns to the picker with the updated name and existing agent annotations.
Esc, `Ctrl-C`, or `Ctrl-G` cancels the edit. Errors stay in the rename prompt.
The filter is preserved, so a renamed window disappears if it no longer matches.
Saving uses one tmux subprocess and does not switch windows.
Both rename methods change the tmux window name, not a pane title. Names are
passed literally to tmux, including `#`, semicolons, and backslashes.

### Rename from an agent

```sh
tmux-select rename "ISSUE-123 fix login"
```

This renames the window containing the calling pane, using inherited `TMUX` and
`TMUX_PANE`. It works even when another window is active and does not switch focus
or open the picker. The name appears the next time the picker opens. tmux stores
the name and disables automatic renaming for that window; tmux-select saves no
state and uses a single tmux command.

An agent instruction can say: `Before starting a task, run tmux-select rename
"<task ID> <short description>".` The agent's command runner must preserve both
environment variables. Pass the name as one quoted argument. A missing or invalid
context, an empty or whitespace-only name, or a failed tmux command produces an
error and a nonzero exit status.

### Keys

| Key | Action |
| --- | --- |
| `Enter` | switch to the selected window |
| `Up` / `Ctrl-P` | move selection up |
| `Down` / `Ctrl-N` | move selection down |
| `Ctrl-R` | rename the highlighted window |
| any character | append to the filter query |
| `Backspace` | delete the last query character |
| `Ctrl-U` | clear the query |
| `Esc` / `Ctrl-C` / `Ctrl-G` | cancel without switching |

### Agent states

Each agent pane in a window is annotated as `[<agent>: <state>]`. States are
inferred from terminal output captured when the picker opens; reopen it to
refresh them. Renaming updates the window label but keeps the captured states.

- `blocked` - the agent is paused on a prompt or approval and needs input.
  Selecting the window jumps to that pane.
- `working` - an active-turn or supported waiting indicator is detected.
- `idle` - no recognized working or blocked indicator is detected.

Input requests take precedence over working indicators. A Codex background
terminal footer alone does not count as working: a completed turn remains idle
even if a background terminal is still running.

### Filtering by state

The fuzzy filter matches the whole line: window index, name or directory, and
agent annotations. Type `blocked` to find windows needing input; names and paths
can also match, so this is not an exclusive state filter.

An empty filter keeps window-index order. With a query, matches are ranked by
fuzzy score, then earliest match position, with window-index order breaking ties.

## Tests

Run `cargo test --locked`. To also run the live tmux tests, install tmux and use
`cargo test --locked -- --include-ignored`. These tests create isolated servers
under `/tmp/agents` and remove them afterward.
They cover caller-window targeting, Ctrl-R editing and errors, and literal names,
including repeated edits of names containing backslashes. The status test covers
empty output, counts for all three states, explicit targets, session isolation,
focus preservation, and status-line rendering and refresh through actual `#()` jobs.

## License

Licensed under the Apache License, Version 2.0. Copyright 2026 Ivan Zakharchanka.
See [LICENSE](LICENSE).
