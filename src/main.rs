mod agent;
mod collect;
mod picker;
mod process;
mod tmux;

use std::collections::HashMap;
use std::ffi::OsString;
use std::io::Write;
use std::path::Path;
use std::process::ExitCode;

use agent::AgentState;
use collect::{PaneReading, collect_panes};
use tmux::Result;

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
    let command = parse_arguments(std::env::args_os().skip(1))?;
    let tmux =
        std::env::var("TMUX").map_err(|_| "TMUX is not set; tmux-select must run inside tmux")?;
    match command {
        CliCommand::Status(target) => {
            let session = status_session(&tmux, target.as_deref())?;
            let panes = collect_panes(Path::new(tmux::socket_from_tmux_value(&tmux)?), &session)?;
            let status = status_line(
                panes
                    .iter()
                    .filter_map(|reading| reading.agent.map(|(_, state)| state)),
            );
            writeln!(std::io::stdout().lock(), "{status}")?;
            Ok(())
        }
        CliCommand::Pick => {
            let session = tmux::session_id_from_tmux_value(&tmux)?;
            let panes = collect_panes(Path::new(tmux::socket_from_tmux_value(&tmux)?), &session)?;
            show_picker(&tmux, panes)
        }
        CliCommand::Rename(name) => {
            let pane_id = std::env::var("TMUX_PANE")
                .map_err(|_| "TMUX_PANE is not set; cannot identify the calling pane")?;
            tmux::rename_window(&tmux, &pane_id, &name)
        }
    }
}

fn show_picker(tmux: &str, panes: Vec<PaneReading>) -> Result<()> {
    let mut windows = group_windows(panes);
    let default_index = windows.iter().position(|window| window.active).unwrap_or(0);

    let readings = windows.iter().map(agent_readings).collect::<Vec<_>>();
    let candidates = windows
        .iter()
        .zip(&readings)
        .map(|(window, readings)| candidate_for(window, readings))
        .collect();
    if let Some(target) = picker::pick(candidates, default_index, |index, name| {
        let window = windows.get_mut(index).ok_or("selected window is missing")?;
        let readings = readings
            .get(index)
            .ok_or("selected window readings are missing")?;
        window.name = tmux::rename_selected_window(tmux, &window.window_id, name)?;
        window.label.clone_from(&window.name);
        Ok(candidate_for(window, readings))
    })? {
        tmux::switch_to(
            Path::new(tmux::socket_from_tmux_value(tmux)?),
            &target.window_id,
            target.pane_id.as_deref(),
        )?;
    }
    Ok(())
}

#[derive(Debug, PartialEq, Eq)]
enum CliCommand {
    Pick,
    Rename(String),
    Status(Option<String>),
}

fn parse_arguments(mut args: impl Iterator<Item = OsString>) -> Result<CliCommand> {
    let Some(command) = args.next() else {
        return Ok(CliCommand::Pick);
    };
    if command == "status" {
        let Some(option) = args.next() else {
            return Ok(CliCommand::Status(None));
        };
        if option != "-t" {
            return Err("usage: tmux-select status [-t <session-id>]".into());
        }
        let session = args
            .next()
            .and_then(|value| value.into_string().ok())
            .ok_or("usage: tmux-select status [-t <session-id>]")?;
        if args.next().is_some() {
            return Err("usage: tmux-select status [-t <session-id>]".into());
        }
        if !session.strip_prefix('$').is_some_and(|number| {
            !number.is_empty() && number.bytes().all(|byte| byte.is_ascii_digit())
        }) {
            return Err("session target must be an ID such as $0".into());
        }
        return Ok(CliCommand::Status(Some(session)));
    }
    if command != "rename" {
        return Err("usage: tmux-select [status | rename <name>]".into());
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
    Ok(CliCommand::Rename(name))
}

fn status_session(tmux: &str, target: Option<&str>) -> Result<String> {
    match target {
        Some(session) => Ok(session.to_owned()),
        None => tmux::session_id_from_tmux_value(tmux).map_err(|error| {
            format!("{error}; status-line jobs require status -t '#{{session_id}}'").into()
        }),
    }
}

fn status_line(states: impl Iterator<Item = AgentState>) -> String {
    let mut working = 0usize;
    let mut blocked = 0usize;
    let mut idle = 0usize;
    for state in states {
        match state {
            AgentState::Working => working += 1,
            AgentState::Blocked => blocked += 1,
            AgentState::Idle => idle += 1,
        }
    }
    let mut line = String::new();
    for (label, count) in [("working", working), ("blocked", blocked), ("idle", idle)] {
        if count != 0 {
            if !line.is_empty() {
                line.push_str(" | ");
            }
            line.push_str(&format!("{label}: {count}"));
        }
    }
    line
}

struct Window {
    window_id: String,
    window_index: u32,
    active: bool,
    name: String,
    label: String,
    panes: Vec<PaneReading>,
}

struct AgentReading {
    pane_id: String,
    label: &'static str,
    state: AgentState,
}

fn group_windows(panes: Vec<PaneReading>) -> Vec<Window> {
    let mut order: Vec<String> = Vec::new();
    let mut windows: HashMap<String, Window> = HashMap::new();
    for reading in panes {
        let pane = &reading.pane;
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
        window.panes.push(reading);
    }
    let mut grouped: Vec<Window> = order
        .into_iter()
        .filter_map(|id| windows.remove(&id))
        .collect();
    grouped.sort_by_key(|window| window.window_index);
    grouped
}

fn agent_readings(window: &Window) -> Vec<AgentReading> {
    let mut agent_panes: Vec<_> = window
        .panes
        .iter()
        .filter_map(|reading| reading.agent.map(|agent| (&reading.pane, agent)))
        .collect();
    agent_panes.sort_by_key(|(pane, _)| pane.pane_index);

    let mut readings = Vec::with_capacity(agent_panes.len());
    for (pane, (kind, state)) in agent_panes {
        readings.push(AgentReading {
            pane_id: pane.pane_id.clone(),
            label: kind.label(),
            state,
        });
    }
    readings
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
    use tmux::Pane;

    #[test]
    fn no_arguments_open_the_picker() {
        assert_eq!(
            parse_arguments(std::iter::empty()).unwrap(),
            CliCommand::Pick
        );
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
                parse_arguments(args.into_iter()).unwrap(),
                CliCommand::Rename(name.into())
            );
        }
    }

    #[test]
    fn invalid_arguments_fail_before_opening_the_picker() {
        for args in [
            vec!["unknown"],
            vec!["status", "extra"],
            vec!["rename"],
            vec!["rename", ""],
            vec!["rename", " \t"],
            vec!["rename", "bad\0name"],
            vec!["rename", "ISSUE-123", "extra"],
        ] {
            assert!(parse_arguments(args.into_iter().map(OsString::from)).is_err());
        }
    }

    #[test]
    fn status_accepts_an_optional_explicit_session_id() {
        assert_eq!(
            parse_arguments([OsString::from("status")].into_iter()).unwrap(),
            CliCommand::Status(None)
        );
        assert_eq!(
            parse_arguments(["status", "-t", "$12"].map(OsString::from).into_iter()).unwrap(),
            CliCommand::Status(Some("$12".into()))
        );
        for args in [
            vec!["status", "-t"],
            vec!["status", "-t", "$"],
            vec!["status", "-t", "-1"],
            vec!["status", "-t", "name"],
            vec!["status", "-t", "$0'; new-session"],
            vec!["status", "-t", "$0", "extra"],
        ] {
            assert!(parse_arguments(args.into_iter().map(OsString::from)).is_err());
        }
    }

    #[test]
    fn status_target_overrides_the_job_or_pane_session() {
        assert_eq!(status_session("socket,123,-1", Some("$2")).unwrap(), "$2");
        assert_eq!(status_session("socket,123,0", Some("$2")).unwrap(), "$2");
        assert_eq!(status_session("socket,123,0", None).unwrap(), "$0");
        assert!(
            status_session("socket,123,-1", None)
                .unwrap_err()
                .to_string()
                .contains("status -t")
        );
    }

    #[test]
    fn status_counts_states_in_fixed_order_and_omits_zero_counts() {
        use AgentState::{Blocked, Idle, Working};
        for (states, expected) in [
            (vec![], ""),
            (vec![Idle], "idle: 1"),
            (vec![Blocked, Blocked], "blocked: 2"),
            (vec![Working], "working: 1"),
            (vec![Idle, Working], "working: 1 | idle: 1"),
            (
                vec![Idle, Blocked, Working, Idle, Working],
                "working: 2 | blocked: 1 | idle: 2",
            ),
        ] {
            assert_eq!(status_line(states.into_iter()), expected);
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
        let windows = group_windows(
            panes
                .into_iter()
                .map(|pane| PaneReading { pane, agent: None })
                .collect(),
        );
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
