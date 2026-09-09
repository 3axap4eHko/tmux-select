mod agent;
mod picker;
mod process;
mod tmux;

use std::collections::{HashMap, HashSet};
use std::ffi::OsString;
use std::process::ExitCode;

use agent::{AgentKind, AgentState, match_state};
use tmux::{ControlClient, Pane, Result};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("tmux-select: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<()> {
    if let Some(name) = rename_argument(std::env::args_os().skip(1))? {
        let tmux =
            std::env::var("TMUX").map_err(|_| "TMUX is not set; rename must run inside tmux")?;
        let pane_id = std::env::var("TMUX_PANE")
            .map_err(|_| "TMUX_PANE is not set; cannot identify the calling pane")?;
        return tmux::rename_window(&tmux, &pane_id, &name);
    }

    let session = tmux::current_session_id()?;
    let mut client = ControlClient::attach(&session)?;
    let panes = client.enumerate(&session)?;
    let pane_pids: HashSet<u32> = panes.iter().map(|pane| pane.pane_pid).collect();
    let mut windows = group_windows(panes);
    let default_index = windows.iter().position(|window| window.active).unwrap_or(0);
    let agents = process::classify_panes(&pane_pids);

    let readings = windows
        .iter()
        .map(|window| read_agents(&mut client, window, &agents))
        .collect::<Vec<_>>();
    let candidates = windows
        .iter()
        .zip(&readings)
        .map(|(window, readings)| candidate_for(window, readings))
        .collect();
    client.detach()?;

    let tmux = std::env::var("TMUX")?;
    if let Some(target) = picker::pick(candidates, default_index, |index, name| {
        let window = windows.get_mut(index).ok_or("selected window is missing")?;
        let readings = readings
            .get(index)
            .ok_or("selected window readings are missing")?;
        window.name = tmux::rename_selected_window(&tmux, &window.window_id, name)?;
        window.label.clone_from(&window.name);
        Ok(candidate_for(window, readings))
    })? {
        tmux::switch_to(&target.window_id, target.pane_id.as_deref())?;
    }
    Ok(())
}

fn rename_argument(mut args: impl Iterator<Item = OsString>) -> Result<Option<String>> {
    let Some(command) = args.next() else {
        return Ok(None);
    };
    if command != "rename" {
        return Err("usage: tmux-select [rename <name>]".into());
    }
    let name = args
        .next()
        .ok_or("usage: tmux-select rename <name>")?
        .into_string()
        .map_err(|_| "window name must be valid UTF-8")?;
    if args.next().is_some() {
        return Err("usage: tmux-select rename <name> (quote names containing spaces)".into());
    }
    if name.trim().is_empty() || name.contains('\0') {
        return Err("window name must be nonempty and contain no NUL bytes".into());
    }
    Ok(Some(name))
}

struct Window {
    window_id: String,
    window_index: u32,
    active: bool,
    name: String,
    label: String,
    panes: Vec<Pane>,
}

struct AgentReading {
    pane_id: String,
    label: &'static str,
    state: AgentState,
}

fn group_windows(panes: Vec<Pane>) -> Vec<Window> {
    let mut order: Vec<String> = Vec::new();
    let mut windows: HashMap<String, Window> = HashMap::new();
    for pane in panes {
        let window = windows.entry(pane.window_id.clone()).or_insert_with(|| {
            order.push(pane.window_id.clone());
            Window {
                window_id: pane.window_id.clone(),
                window_index: pane.window_index,
                active: pane.window_active,
                name: pane.window_name.clone(),
                label: String::new(),
                panes: Vec::new(),
            }
        });
        if pane.pane_active || window.label.is_empty() {
            window.label = pane.label.clone();
        }
        window.panes.push(pane);
    }
    let mut grouped: Vec<Window> = order
        .into_iter()
        .filter_map(|id| windows.remove(&id))
        .collect();
    grouped.sort_by_key(|window| window.window_index);
    grouped
}

fn read_agents(
    client: &mut ControlClient,
    window: &Window,
    agents: &HashMap<u32, AgentKind>,
) -> Vec<AgentReading> {
    let mut agent_panes: Vec<(&Pane, AgentKind)> = window
        .panes
        .iter()
        .filter_map(|pane| pane_agent(pane, agents).map(|kind| (pane, kind)))
        .collect();
    agent_panes.sort_by_key(|(pane, _)| pane.pane_index);

    let mut readings = Vec::with_capacity(agent_panes.len());
    for (pane, kind) in agent_panes {
        let state = match client.capture(&pane.pane_id) {
            Ok(screen) => match_state(kind, &screen),
            Err(error) => {
                eprintln!("tmux-select: capture-pane {} failed: {error}", pane.pane_id);
                AgentState::Idle
            }
        };
        readings.push(AgentReading {
            pane_id: pane.pane_id.clone(),
            label: kind.label(),
            state,
        });
    }
    readings
}

fn pane_agent(pane: &Pane, agents: &HashMap<u32, AgentKind>) -> Option<AgentKind> {
    agents
        .get(&pane.pane_pid)
        .copied()
        .or_else(|| AgentKind::from_name(&pane.current_command))
}

fn state_color(state: AgentState) -> picker::SpanColor {
    match state {
        AgentState::Idle => picker::SpanColor::Yellow,
        AgentState::Working => picker::SpanColor::Green,
        AgentState::Blocked => picker::SpanColor::Red,
    }
}

fn candidate_for(window: &Window, readings: &[AgentReading]) -> picker::Candidate {
    let label = window.label.replace(['\t', '\n', '\r'], " ");
    let mut display = format!("{:>2}: {}", window.window_index, label);
    let mut length = display.chars().count();
    let mut spans = Vec::with_capacity(readings.len());
    let mut blocked: Option<String> = None;
    for reading in readings {
        let head = format!(" [{}: ", reading.label);
        let state = reading.state.label();
        display.push_str(&head);
        display.push_str(state);
        display.push(']');
        let start = length + head.chars().count();
        spans.push((
            start..start + state.chars().count(),
            state_color(reading.state),
        ));
        length = start + state.chars().count() + 1;
        if reading.state == AgentState::Blocked && blocked.is_none() {
            blocked = Some(reading.pane_id.clone());
        }
    }
    picker::Candidate {
        display,
        name: window.name.clone(),
        spans,
        window_id: window.window_id.clone(),
        pane_id: blocked,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_arguments_open_the_picker() {
        assert_eq!(rename_argument(std::iter::empty()).unwrap(), None);
    }

    #[test]
    fn rename_accepts_one_literal_name() {
        for name in [
            "ISSUE-123",
            "ISSUE-123 fix login",
            "-leading",
            "  padded  ",
            ";",
        ] {
            let args = [OsString::from("rename"), OsString::from(name)];
            assert_eq!(
                rename_argument(args.into_iter()).unwrap(),
                Some(name.into())
            );
        }
    }

    #[test]
    fn invalid_arguments_fail_before_opening_the_picker() {
        for args in [
            vec!["unknown"],
            vec!["rename"],
            vec!["rename", ""],
            vec!["rename", " \t"],
            vec!["rename", "bad\0name"],
            vec!["rename", "ISSUE-123", "extra"],
        ] {
            assert!(rename_argument(args.into_iter().map(OsString::from)).is_err());
        }
    }

    fn window() -> Window {
        Window {
            window_id: "@3".to_string(),
            window_index: 12,
            active: false,
            name: "api".into(),
            label: "~/api".to_string(),
            panes: Vec::new(),
        }
    }

    fn reading(pane_id: &str, label: &'static str, state: AgentState) -> AgentReading {
        AgentReading {
            pane_id: pane_id.to_string(),
            label,
            state,
        }
    }

    #[test]
    fn non_agent_window_has_no_groups_and_no_pane_target() {
        let candidate = candidate_for(&window(), &[]);
        assert_eq!(candidate.display, "12: ~/api");
        assert_eq!(candidate.window_id, "@3");
        assert_eq!(candidate.pane_id, None);
        assert!(candidate.spans.is_empty());
    }

    #[test]
    fn first_blocked_pane_becomes_the_target() {
        let readings = [
            reading("%1", "claude", AgentState::Working),
            reading("%4", "codex", AgentState::Blocked),
            reading("%6", "claude", AgentState::Blocked),
        ];
        let candidate = candidate_for(&window(), &readings);
        assert_eq!(
            candidate.display,
            "12: ~/api [claude: working] [codex: blocked] [claude: blocked]"
        );
        assert_eq!(candidate.pane_id.as_deref(), Some("%4"));
        assert_eq!(
            candidate.spans,
            vec![
                (19..26, picker::SpanColor::Green),
                (36..43, picker::SpanColor::Red),
                (54..61, picker::SpanColor::Red),
            ]
        );
    }

    #[test]
    fn spans_use_char_indexes_for_a_non_ascii_path() {
        let mut window = window();
        window.label = "~/посткод".to_string();
        let readings = [reading("%1", "claude", AgentState::Idle)];
        let candidate = candidate_for(&window, &readings);
        let chars: Vec<char> = candidate.display.chars().collect();
        let (range, color) = &candidate.spans[0];
        let word: String = chars[range.start..range.end].iter().collect();
        assert_eq!(word, "idle");
        assert_eq!(*color, picker::SpanColor::Yellow);
    }

    #[test]
    fn windows_are_ordered_by_index_and_keep_active_pane_path() {
        let panes = vec![
            Pane {
                pane_id: "%5".into(),
                window_id: "@1".into(),
                window_index: 2,
                window_active: false,
                pane_active: false,
                pane_index: 1,
                pane_pid: 5,
                current_command: "bash".into(),
                window_name: "shell".into(),
                label: "/a".into(),
            },
            Pane {
                pane_id: "%9".into(),
                window_id: "@1".into(),
                window_index: 2,
                window_active: false,
                pane_active: true,
                pane_index: 2,
                pane_pid: 9,
                current_command: "vim".into(),
                window_name: "shell".into(),
                label: "/active".into(),
            },
            Pane {
                pane_id: "%2".into(),
                window_id: "@0".into(),
                window_index: 1,
                window_active: true,
                pane_active: true,
                pane_index: 1,
                pane_pid: 2,
                current_command: "claude".into(),
                window_name: "Fix allocation leak".into(),
                label: "Fix allocation leak".into(),
            },
        ];
        let windows = group_windows(panes);
        assert_eq!(windows.len(), 2);
        assert_eq!(windows[0].window_index, 1);
        assert!(windows[0].active);
        assert_eq!(
            candidate_for(&windows[0], &[]).display,
            " 1: Fix allocation leak"
        );
        assert_eq!(windows[1].window_index, 2);
        assert!(!windows[1].active);
        assert_eq!(windows[1].label, "/active");
    }

    #[test]
    fn fixed_window_names_preserve_agent_spans_and_blocked_targets() {
        let mut window = window();
        window.label = "Fix\tallocation\nleak\r\u{e9}".into();
        let candidate = candidate_for(&window, &[reading("%4", "claude", AgentState::Blocked)]);
        assert_eq!(
            candidate.display,
            "12: Fix allocation leak \u{e9} [claude: blocked]"
        );
        assert_eq!(candidate.pane_id.as_deref(), Some("%4"));
        let (range, color) = &candidate.spans[0];
        let state: String = candidate
            .display
            .chars()
            .skip(range.start)
            .take(range.len())
            .collect();
        assert_eq!(state, "blocked");
        assert_eq!(*color, picker::SpanColor::Red);
    }
}
