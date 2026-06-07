use std::collections::{HashMap, HashSet};

use crate::agent::AgentKind;

type Parent = HashMap<u32, u32>;
type Agents = Vec<(u32, AgentKind)>;

pub fn classify_panes(pane_pids: &HashSet<u32>) -> HashMap<u32, AgentKind> {
    let mut parent: Parent = HashMap::with_capacity(256);
    let mut agents: Agents = Vec::new();
    collect_processes(&mut parent, &mut agents);

    agents.sort_unstable_by_key(|(pid, _)| *pid);
    let mut owners = HashMap::new();
    for (pid, kind) in agents {
        if let Some(pane_pid) = climb_to_pane(pid, pane_pids, &parent) {
            owners.entry(pane_pid).or_insert(kind);
        }
    }
    owners
}

#[cfg_attr(not(any(target_os = "linux", target_os = "macos")), allow(dead_code))]
fn record(
    pid: u32,
    ppid: u32,
    pgrp: u32,
    tpgid: i64,
    comm: &str,
    parent: &mut Parent,
    agents: &mut Agents,
) {
    parent.insert(pid, ppid);
    if let Some(kind) = AgentKind::from_name(comm)
        && i64::from(pgrp) == tpgid
    {
        agents.push((pid, kind));
    }
}

#[cfg(target_os = "linux")]
fn collect_processes(parent: &mut Parent, agents: &mut Agents) {
    use std::fmt::Write as _;
    use std::fs::File;
    use std::io::Read as _;

    let Ok(entries) = std::fs::read_dir("/proc") else {
        return;
    };
    let mut path = String::new();
    let mut buf = Vec::with_capacity(512);
    for entry in entries.flatten() {
        let Some(pid) = entry
            .file_name()
            .to_str()
            .and_then(|name| name.parse::<u32>().ok())
        else {
            continue;
        };
        path.clear();
        let _ = write!(path, "/proc/{pid}/stat");
        buf.clear();
        let Ok(mut file) = File::open(&path) else {
            continue;
        };
        if file.read_to_end(&mut buf).is_err() {
            continue;
        }
        if let Some(stat) = parse_stat(&String::from_utf8_lossy(&buf)) {
            record(
                pid, stat.ppid, stat.pgrp, stat.tpgid, stat.comm, parent, agents,
            );
        }
    }
}

#[cfg(target_os = "macos")]
fn collect_processes(parent: &mut Parent, agents: &mut Agents) {
    use std::process::Command;

    let Ok(output) = Command::new("ps")
        .args(["-axo", "pid=,ppid=,pgid=,tpgid=,ucomm="])
        .output()
    else {
        return;
    };
    if !output.status.success() {
        return;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    for line in text.lines() {
        if let Some((pid, ppid, pgrp, tpgid, comm)) = parse_ps_line(line) {
            record(pid, ppid, pgrp, tpgid, comm, parent, agents);
        }
    }
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn collect_processes(_parent: &mut Parent, _agents: &mut Agents) {}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
struct Stat<'a> {
    comm: &'a str,
    ppid: u32,
    pgrp: u32,
    tpgid: i64,
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn parse_stat(stat: &str) -> Option<Stat<'_>> {
    let open = stat.find('(')?;
    let close = stat.rfind(')')?;
    let comm = stat.get(open + 1..close)?;
    let after = stat.get(close + 1..)?;
    let mut fields = after.split_whitespace();
    let _state = fields.next()?;
    let ppid = fields.next()?.parse().ok()?;
    let pgrp = fields.next()?.parse().ok()?;
    let _session = fields.next()?;
    let _tty_nr = fields.next()?;
    let tpgid = fields.next()?.parse().ok()?;
    Some(Stat {
        comm,
        ppid,
        pgrp,
        tpgid,
    })
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn parse_ps_line(line: &str) -> Option<(u32, u32, u32, i64, &str)> {
    let mut fields = line.split_whitespace();
    let pid = fields.next()?.parse().ok()?;
    let ppid = fields.next()?.parse().ok()?;
    let pgrp = fields.next()?.parse().ok()?;
    let tpgid = fields.next()?.parse().ok()?;
    let comm = fields.next()?;
    Some((pid, ppid, pgrp, tpgid, comm))
}

fn climb_to_pane(start: u32, pane_pids: &HashSet<u32>, parent: &Parent) -> Option<u32> {
    let mut pid = start;
    for _ in 0..64 {
        if pane_pids.contains(&pid) {
            return Some(pid);
        }
        let next = *parent.get(&pid)?;
        if next == pid || next == 0 {
            return None;
        }
        pid = next;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_stat_extracts_comm_ppid_pgrp_tpgid() {
        let stat = parse_stat("2776949 (codex) S 2776942 2776929 2776929 34850 2776929 0").unwrap();
        assert_eq!(
            (stat.comm, stat.ppid, stat.pgrp, stat.tpgid),
            ("codex", 2776942, 2776929, 2776929)
        );
    }

    #[test]
    fn parse_stat_handles_spaces_and_parens_in_comm() {
        let stat = parse_stat("42 (npm exec @opena) S 7 9 9 0 9 0").unwrap();
        assert_eq!((stat.comm, stat.ppid), ("npm exec @opena", 7));
        let stat = parse_stat("42 (weird (x)) R 7 9 9 0 9 0").unwrap();
        assert_eq!(stat.comm, "weird (x)");
    }

    #[test]
    fn parse_stat_keeps_processes_with_no_controlling_terminal() {
        let stat = parse_stat("3 (kthreadd) S 2 0 0 0 -1 0").unwrap();
        assert_eq!((stat.ppid, stat.pgrp, stat.tpgid), (2, 0, -1));
    }

    #[test]
    fn parse_ps_line_reads_macos_columns() {
        assert_eq!(
            parse_ps_line("  501   499   501   501 codex"),
            Some((501, 499, 501, 501, "codex"))
        );
        assert_eq!(
            parse_ps_line("  502     1   502    -1 launchd"),
            Some((502, 1, 502, -1, "launchd"))
        );
        assert_eq!(parse_ps_line("garbage"), None);
    }

    #[test]
    fn climbs_from_agent_leaf_to_owning_pane() {
        let parent: Parent = [
            (2776949, 2776942),
            (2776942, 2776941),
            (2776941, 2776929),
            (2776929, 2776867),
            (2776867, 1),
        ]
        .into();
        let panes: HashSet<u32> = [2776867].into();
        assert_eq!(climb_to_pane(2776949, &panes, &parent), Some(2776867));
    }

    #[test]
    fn climb_returns_none_without_a_pane_ancestor() {
        let parent: Parent = [(50, 40), (40, 1)].into();
        let panes: HashSet<u32> = [999].into();
        assert_eq!(climb_to_pane(50, &panes, &parent), None);
    }

    #[test]
    fn climb_terminates_on_a_cycle() {
        let parent: Parent = [(10, 20), (20, 10)].into();
        let panes: HashSet<u32> = [999].into();
        assert_eq!(climb_to_pane(10, &panes, &parent), None);
    }
}
