use std::collections::HashSet;
use std::path::Path;

use crate::agent::{AgentKind, AgentState, match_state};
use crate::process;
use crate::tmux::{ControlClient, Pane, Result};

pub struct PaneReading {
    pub pane: Pane,
    pub agent: Option<(AgentKind, AgentState)>,
}

pub fn collect_panes(socket: &Path, session_id: &str) -> Result<Vec<PaneReading>> {
    let mut client = ControlClient::attach(socket, session_id)?;
    let readings = read_panes(&mut client, session_id);
    match (readings, client.detach()) {
        (Ok(readings), Ok(())) => Ok(readings),
        (Err(error), Ok(())) | (Ok(_), Err(error)) => Err(error),
        (Err(error), Err(detach_error)) => {
            Err(format!("{error}; failed to detach: {detach_error}").into())
        }
    }
}

fn read_panes(client: &mut ControlClient, session_id: &str) -> Result<Vec<PaneReading>> {
    let panes = client.enumerate(session_id)?;
    let pane_pids: HashSet<u32> = panes.iter().map(|pane| pane.pane_pid).collect();
    let agents = process::classify_panes(&pane_pids);
    let mut readings = Vec::with_capacity(panes.len());
    for pane in panes {
        let kind = agents
            .get(&pane.pane_pid)
            .copied()
            .or_else(|| AgentKind::from_name(&pane.current_command));
        let agent = match kind {
            Some(kind) => {
                let screen = client
                    .capture(&pane.pane_id)
                    .map_err(|error| format!("capture-pane {} failed: {error}", pane.pane_id))?;
                Some((kind, match_state(kind, &screen)))
            }
            None => None,
        };
        readings.push(PaneReading { pane, agent });
    }
    Ok(readings)
}
