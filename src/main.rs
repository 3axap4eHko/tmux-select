mod agent;
mod picker;
mod process;
mod tmux;

use std::collections::{HashMap, HashSet};
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
    let session = tmux::current_session_id()?;
    let mut client = ControlClient::attach(&session)?;
    let panes = client.enumerate(&session)?;
    let pane_pids: HashSet<u32> = panes.iter().map(|pane| pane.pane_pid).collect();
    let windows = group_windows(panes);
    let agents = process::classify_panes(&pane_pids);

    let mut candidates = Vec::with_capacity(windows.len());
    for window in &windows {
        candidates.push(build_entry(&mut client, window, &agents)?);
    }
    client.detach()?;

    if let Some(target) = picker::pick(candidates)? {
        tmux::switch_to(&target.window_id, target.pane_id.as_deref())?;
    }
    Ok(())
}

struct Window {
    window_id: String,
    window_index: u32,
    path: String,
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
                path: String::new(),
                panes: Vec::new(),
            }
        });
        if pane.pane_active || window.path.is_empty() {
            window.path = pane.current_path.clone();
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

fn build_entry(
    client: &mut ControlClient,
    window: &Window,
    agents: &HashMap<u32, AgentKind>,
) -> Result<picker::Candidate> {
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
            Err(_) => AgentState::Idle,
        };
        readings.push(AgentReading {
            pane_id: pane.pane_id.clone(),
            label: kind.label(),
            state,
        });
    }
    Ok(candidate_for(window, &readings))
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
    let path = window.path.replace(['\t', '\n', '\r'], " ");
    let mut display = format!("{:>2}: {}", window.window_index, path);
    let mut length = display.chars().count();
    let mut spans = Vec::with_capacity(readings.len());
    let mut blocked: Option<String> = None;
    for reading in readings {
        let head = format!(" [{}: ", reading.label);
        let state = reading.state.label();
        display.push_str(&head);
        display.push_str(state);
        display.push(']');
        let start = length + head.len();
        spans.push((start..start + state.len(), state_color(reading.state)));
        length = start + state.len() + 1;
        if reading.state == AgentState::Blocked && blocked.is_none() {
            blocked = Some(reading.pane_id.clone());
        }
    }
    picker::Candidate {
        display,
        spans,
        window_id: window.window_id.clone(),
        pane_id: blocked,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn window() -> Window {
        Window {
            window_id: "@3".to_string(),
            window_index: 12,
            path: "~/api".to_string(),
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
        window.path = "~/посткод".to_string();
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
                pane_active: false,
                pane_index: 1,
                pane_pid: 5,
                current_command: "bash".into(),
                current_path: "/a".into(),
            },
            Pane {
                pane_id: "%9".into(),
                window_id: "@1".into(),
                window_index: 2,
                pane_active: true,
                pane_index: 2,
                pane_pid: 9,
                current_command: "vim".into(),
                current_path: "/active".into(),
            },
            Pane {
                pane_id: "%2".into(),
                window_id: "@0".into(),
                window_index: 1,
                pane_active: true,
                pane_index: 1,
                pane_pid: 2,
                current_command: "claude".into(),
                current_path: "/work".into(),
            },
        ];
        let windows = group_windows(panes);
        assert_eq!(windows.len(), 2);
        assert_eq!(windows[0].window_index, 1);
        assert_eq!(windows[1].window_index, 2);
        assert_eq!(windows[1].path, "/active");
    }
}
