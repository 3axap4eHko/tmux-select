#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AgentKind {
    Claude,
    Codex,
    Kimi,
}

impl AgentKind {
    pub fn from_name(name: &str) -> Option<AgentKind> {
        match name {
            "claude" => Some(AgentKind::Claude),
            "codex" => Some(AgentKind::Codex),
            "kimi" => Some(AgentKind::Kimi),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            AgentKind::Claude => "claude",
            AgentKind::Codex => "codex",
            AgentKind::Kimi => "kimi",
        }
    }

    // The Claude installer symlinks ~/.local/bin/claude to a binary literally named
    // by its version (e.g. .../share/claude/versions/2.1.173), and macOS records the
    // resolved file's name as the process name, so the basename alone cannot identify
    // the agent there.
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    pub fn from_path(path: &str) -> Option<AgentKind> {
        let mut components = path.split('/').rev();
        let basename = components.next()?;
        if let Some(kind) = AgentKind::from_name(basename) {
            return Some(kind);
        }
        if !is_version_shaped(basename) {
            return None;
        }
        components.find_map(AgentKind::from_name)
    }
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn is_version_shaped(name: &str) -> bool {
    let bytes = name.as_bytes();
    match (bytes.first(), bytes.last()) {
        (Some(first), Some(last)) if first.is_ascii_digit() && last.is_ascii_digit() => {
            bytes.iter().all(|b| b.is_ascii_digit() || *b == b'.')
        }
        _ => false,
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AgentState {
    Idle,
    Working,
    Blocked,
}

impl AgentState {
    pub fn label(self) -> &'static str {
        match self {
            AgentState::Idle => "idle",
            AgentState::Working => "working",
            AgentState::Blocked => "blocked",
        }
    }
}

const LIVE_LINES: usize = 8;
const CLAUDE_SPINNER: [char; 7] = ['·', '✢', '✳', '✶', '✻', '✽', '*'];
const KIMI_MOON_SPINNER: [char; 8] = ['🌑', '🌒', '🌓', '🌔', '🌕', '🌖', '🌗', '🌘'];
const KIMI_BRAILLE_SPINNER: [char; 10] = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];
const KIMI_SWARM_ACTIVITY: [&str; 5] = [
    "Orchestrating...",
    "Prompting...",
    "Working...",
    "Queued...",
    "Rate limited...",
];

pub fn match_state(kind: AgentKind, screen: &str) -> AgentState {
    let live = bottom_lines(screen, LIVE_LINES);

    if live.iter().any(|&line| is_blocked(kind, line)) {
        return AgentState::Blocked;
    }
    if live.iter().any(|&line| is_working(kind, line))
        || (kind == AgentKind::Kimi && kimi_has_active_swarm(&live))
    {
        return AgentState::Working;
    }
    AgentState::Idle
}

fn is_working(kind: AgentKind, line: &str) -> bool {
    match kind {
        AgentKind::Claude => {
            let head = line.trim_start();
            (head.starts_with(CLAUDE_SPINNER) && head.contains("… (")) || head.starts_with('◯')
        }
        AgentKind::Codex => {
            line.contains("to interrupt)") || line.contains("background terminal running")
        }
        AgentKind::Kimi => kimi_is_working(line),
    }
}

fn kimi_is_working(line: &str) -> bool {
    let head = line.trim_start();
    let mut chars = head.chars();
    let Some(frame) = chars.next() else {
        return false;
    };
    if KIMI_MOON_SPINNER.contains(&frame) {
        return true;
    }
    if KIMI_BRAILLE_SPINNER.contains(&frame) {
        let status = chars.as_str().trim_start();
        if status.starts_with("thinking...")
            || status.starts_with("working...")
            || status.contains("Agent Starting")
            || status.contains("Agent Queued")
            || status.contains("Agent Running")
        {
            return true;
        }
    }
    false
}

fn kimi_has_active_swarm(lines: &[&str]) -> bool {
    let has_header = lines
        .iter()
        .any(|line| line.trim_start().starts_with("─ Agent Swarm"));
    has_header
        && lines.iter().any(|line| {
            let head = line.trim_start();
            KIMI_SWARM_ACTIVITY
                .iter()
                .any(|status| kimi_is_swarm_activity_line(head, status))
        })
}

fn kimi_is_swarm_activity_line(line: &str, status: &str) -> bool {
    match status {
        "Orchestrating..." => line == status,
        "Prompting..." => line == status || line.starts_with("Prompting... "),
        "Working..." | "Rate limited..." => {
            line == status
                || line
                    .strip_prefix(status)
                    .is_some_and(|tail| tail.starts_with("  ━"))
        }
        "Queued..." => line.split_once(' ').is_some_and(|(id, text)| {
            id.len() == 3 && id.bytes().all(|byte| byte.is_ascii_digit()) && text == "Queued..."
        }),
        _ => false,
    }
}

fn is_blocked(kind: AgentKind, line: &str) -> bool {
    match kind {
        AgentKind::Claude => line.contains("Esc to cancel") && line.contains('·'),
        AgentKind::Codex => {
            line.contains("to submit answer")
                || line.contains("to submit all")
                || (line.contains("to confirm or") && line.contains("to cancel"))
        }
        AgentKind::Kimi => kimi_is_blocked(line),
    }
}

fn kimi_is_blocked(line: &str) -> bool {
    let head = line.trim();
    (head.starts_with("↑/↓ select · ") && head.contains(" choose · ↵ confirm"))
        || head == "Type feedback · ↵ submit."
        || (head.starts_with("↑↓ select")
            && (head.contains("↵ choose")
                || head.contains("↵ toggle")
                || head.contains("↵ confirm")))
        || (head.starts_with("type answer") && head.contains("↵ save"))
}

fn bottom_lines(screen: &str, n: usize) -> Vec<&str> {
    let mut lines: Vec<&str> = screen
        .lines()
        .rev()
        .filter(|line| !line.trim().is_empty())
        .take(n)
        .collect();
    lines.reverse();
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    fn claude_frame(body: &str) -> String {
        format!("{body}\n────────────\n❯ \n────────────\n  ~/projects/x  claude\n")
    }

    #[test]
    fn from_name_matches_only_known_agents() {
        assert_eq!(AgentKind::from_name("claude"), Some(AgentKind::Claude));
        assert_eq!(AgentKind::from_name("codex"), Some(AgentKind::Codex));
        assert_eq!(AgentKind::from_name("kimi"), Some(AgentKind::Kimi));
        assert_eq!(AgentKind::from_name("npm"), None);
        assert_eq!(AgentKind::from_name("node"), None);
    }

    #[test]
    fn from_path_matches_agent_basenames() {
        assert_eq!(
            AgentKind::from_path("/opt/homebrew/bin/codex"),
            Some(AgentKind::Codex)
        );
        assert_eq!(
            AgentKind::from_path("/Users/me/.local/bin/claude"),
            Some(AgentKind::Claude)
        );
        assert_eq!(AgentKind::from_path("claude"), Some(AgentKind::Claude));
        assert_eq!(
            AgentKind::from_path("/home/me/.kimi-code/bin/kimi"),
            Some(AgentKind::Kimi)
        );
        assert_eq!(AgentKind::from_path("/usr/local/bin/node"), None);
    }

    #[test]
    fn from_path_matches_a_version_named_binary_under_an_agent_directory() {
        assert_eq!(
            AgentKind::from_path("/Users/izakharchanka/.local/share/claude/versions/2.1.173"),
            Some(AgentKind::Claude)
        );
        assert_eq!(
            AgentKind::from_path("/opt/codex/versions/9.9"),
            Some(AgentKind::Codex)
        );
    }

    #[test]
    fn from_path_rejects_version_binaries_outside_agent_directories_and_vice_versa() {
        assert_eq!(AgentKind::from_path("/opt/foo/versions/2.1.173"), None);
        assert_eq!(
            AgentKind::from_path("/Users/me/projects/claude/target/debug/mytool"),
            None
        );
        assert_eq!(AgentKind::from_path("/opt/claude/versions/2.1.173b"), None);
        assert_eq!(AgentKind::from_path(""), None);
    }

    #[test]
    fn claude_spinner_is_working() {
        for status in [
            "· Channeling… (1s · ↓ 21 tokens · thinking)",
            "✢ Forging… (2s · thinking with high effort)",
            "✻ Sublimating… (5s · ↓ 146 tokens · thought for 2s)",
        ] {
            assert_eq!(
                match_state(AgentKind::Claude, &claude_frame(status)),
                AgentState::Working,
                "{status}"
            );
        }
    }

    #[test]
    fn claude_spinner_with_a_non_gerund_title_is_working() {
        for status in [
            "✻ Phase B: baby form from Quaternius pack… (12m 47s · ↓ 30.8k tokens)",
            "✶ Yak-shaving… (3s · ↓ 1.2k tokens)",
            "* Cooking dinner… (1s)",
        ] {
            assert_eq!(
                match_state(AgentKind::Claude, &claude_frame(status)),
                AgentState::Working,
                "{status}"
            );
        }
    }

    #[test]
    fn claude_running_workflow_is_working() {
        for footer in [
            "  ◯ sleep-2min  Spawn one subagent ... 0/1 agents done · 49s · ↓ 16.0k tokens",
            "  ◯ review-codebase  Review ... 6/7 agents done · 2m 54s · ↓ 429.1k tokens",
        ] {
            assert_eq!(
                match_state(AgentKind::Claude, &claude_frame(footer)),
                AgentState::Working,
                "{footer}"
            );
        }
    }

    #[test]
    fn claude_idle_prompt_with_a_typed_gerund_is_idle() {
        assert_eq!(
            match_state(AgentKind::Claude, &claude_frame("❯ fix parsing…")),
            AgentState::Idle
        );
    }

    #[test]
    fn claude_selection_prompt_is_blocked() {
        let screen = "Do you want to make this edit to file.rs?\n  1. Yes\n  2. No\nEnter to select · ↑/↓ to navigate · Esc to cancel\n";
        assert_eq!(match_state(AgentKind::Claude, screen), AgentState::Blocked);
    }

    #[test]
    fn claude_bash_approval_dialog_is_blocked() {
        let screen = "\
● Thinking for 13s, listing 2 directories… (ctrl+o to expand)
  ⎿  $ ls /home/zenpie/projects/prmt/.github/workflows/ 2>/dev/null &&
     echo \"===\" && wc -l /home/zenpie/projects/prmt/.github/workflows/*
────────────────────────────────────────
 Bash command

   ls /home/zenpie/projects/prmt/.github/workflows/ 2>/dev/null
   List workflows in prmt and gwt

 Do you want to proceed?
 ❯ 1. Yes
   2. Yes, allow reading from workflows/ from this project
   3. No

 Esc to cancel · Tab to amend · ctrl+e to explain
";
        assert_eq!(match_state(AgentKind::Claude, screen), AgentState::Blocked);
    }

    #[test]
    fn claude_single_action_esc_hint_is_idle() {
        assert_eq!(
            match_state(
                AgentKind::Claude,
                &claude_frame("Edit and press Enter to retry, or Esc to cancel")
            ),
            AgentState::Idle
        );
    }

    #[test]
    fn codex_working_line_is_working() {
        for line in [
            "• Working (13s • esc to interrupt) · 1 background terminal running · /ps to view · /stop to close",
            "Working (0s • esc to interrupt)",
            "• Thinking (5s • esc to interrupt)",
            "• Reviewing approval request (2s • esc to interrupt)",
            "1 background terminal running · /ps to view · /stop to close",
        ] {
            let screen = format!("$ cargo build\n{line}\n› \nctrl+t to view transcript\n");
            assert_eq!(
                match_state(AgentKind::Codex, &screen),
                AgentState::Working,
                "{line}"
            );
        }
    }

    #[test]
    fn codex_question_prompt_is_blocked() {
        for footer in [
            "tab to add notes | enter to submit answer | esc to interrupt",
            "enter to submit all · esc to interrupt",
            "ctrl + j to submit answer · esc to interrupt",
        ] {
            let screen = format!("Which option do you prefer?\n  1. A\n  2. B\n{footer}\n");
            assert_eq!(
                match_state(AgentKind::Codex, &screen),
                AgentState::Blocked,
                "{footer}"
            );
        }
    }

    #[test]
    fn codex_approval_overlay_is_blocked() {
        let screen = "Would you like to run the following command?\n$ rm -rf build\n  Yes\n  No\nPress enter to confirm or esc to cancel\n";
        assert_eq!(match_state(AgentKind::Codex, screen), AgentState::Blocked);
    }

    #[test]
    fn codex_generic_settings_popup_is_idle() {
        let screen = "Select model\n  gpt-5\n  o3\nPress enter to confirm or esc to go back\n";
        assert_eq!(match_state(AgentKind::Codex, screen), AgentState::Idle);
    }

    #[test]
    fn codex_plain_prompt_is_idle() {
        let screen = "$ cargo build\nbuild succeeded\n› \nctrl+t to view transcript\n";
        assert_eq!(match_state(AgentKind::Codex, screen), AgentState::Idle);
    }

    #[test]
    fn kimi_moon_spinner_is_working() {
        for frame in KIMI_MOON_SPINNER {
            let screen = format!("tool output\n  {frame} Running command\n> \n");
            assert_eq!(
                match_state(AgentKind::Kimi, &screen),
                AgentState::Working,
                "{frame}"
            );
        }
    }

    #[test]
    fn kimi_braille_activity_is_working() {
        for frame in KIMI_BRAILLE_SPINNER {
            for status in [
                "thinking...",
                "working...",
                "Reviewer Agent Starting · 0 tools · 0s",
                "Reviewer Agent Queued · 0 tools · 1s",
                "Reviewer Agent Running · 2 tools · 3s",
            ] {
                let screen = format!("tool output\n  {frame} {status}\n> \n");
                assert_eq!(
                    match_state(AgentKind::Kimi, &screen),
                    AgentState::Working,
                    "{frame} {status}"
                );
            }
        }
    }

    #[test]
    fn kimi_swarm_activity_is_working() {
        for status in KIMI_SWARM_ACTIVITY {
            let status_line = match status {
                "Orchestrating..." => status.to_string(),
                "Prompting..." => format!("{status} review each module"),
                "Queued..." => format!("001 {status}"),
                _ => format!("{status}  ━━━━━"),
            };
            let screen = format!("─ Agent Swarm ─ review\n\n{status_line}\n> \n");
            assert_eq!(
                match_state(AgentKind::Kimi, &screen),
                AgentState::Working,
                "{status}"
            );
        }
    }

    #[test]
    fn kimi_approval_prompt_is_blocked() {
        let screen = "\
────────────────────────────────────────
  ▶ Run this command?

  $ cargo test

  ▶ 1. Approve
    2. Approve for this session
    3. Reject

  ↑/↓ select · 1/2/3 choose · ↵ confirm
────────────────────────────────────────
";
        assert_eq!(match_state(AgentKind::Kimi, screen), AgentState::Blocked);
    }

    #[test]
    fn kimi_approval_feedback_is_blocked() {
        let screen = "\
  ▶ 3. Reject

  Type feedback · ↵ submit.
────────────────────────────────────────
";
        assert_eq!(match_state(AgentKind::Kimi, screen), AgentState::Blocked);
    }

    #[test]
    fn kimi_question_prompt_is_blocked() {
        for footer in [
            "  ↑↓ select  1-3 / ↵ choose  ←/→/tab switch  esc cancel",
            "  ↑↓ select  1-3 / ↵ toggle  ←/→/tab switch  esc cancel",
            "  ↑↓ select  1/2 choose  ↵ confirm  ←/→/tab switch  esc cancel",
            "  type answer  ↵ save  tab switch  esc cancel",
        ] {
            let screen = format!(" question\n\n ? Which option?\n\n{footer}\n────────────\n");
            assert_eq!(
                match_state(AgentKind::Kimi, &screen),
                AgentState::Blocked,
                "{footer}"
            );
        }
    }

    #[test]
    fn kimi_blocked_prompt_wins_over_spinner() {
        let screen = "\
🌗
  ▶ Run this command?
  ▶ 1. Approve
    2. Reject
  ↑/↓ select · 1/2 choose · ↵ confirm
────────────────────────────────────────
";
        assert_eq!(match_state(AgentKind::Kimi, screen), AgentState::Blocked);
    }

    #[test]
    fn kimi_ambiguous_spinner_symbols_are_idle() {
        for line in [
            "⠋ Downloading update",
            "⣀⣄⣤⣦⣶⣷⣿",
            "◐ backgrounded",
            "◓",
            "Working... final response",
            "Queued...",
            "Completed.  ━━━━━",
        ] {
            assert_eq!(
                match_state(AgentKind::Kimi, &format!("{line}\n> \n")),
                AgentState::Idle,
                "{line}"
            );
        }
    }

    #[test]
    fn kimi_completed_swarm_with_response_text_is_idle() {
        let screen = "\
─ Agent Swarm ─ review

 Completed.  ━━━━━

Working... final response
>
";
        assert_eq!(match_state(AgentKind::Kimi, screen), AgentState::Idle);
    }

    #[test]
    fn kimi_plain_prompt_is_idle() {
        let screen = "Welcome to Kimi Code\n\n> \n~/projects/tmux-select\n";
        assert_eq!(match_state(AgentKind::Kimi, screen), AgentState::Idle);
    }

    #[test]
    fn frozen_transcript_is_idle() {
        let screen = "✻ Crunched for 6m 41s\n✻ Waiting for 1 dynamic workflow to finish\n● Dynamic workflow \"x\" completed · 11s\n✻ Churned for 43s\n  7 tasks (6 done, 1 open)\n  ◻ a\n  ✔ b\n  ✔ c\n  … +2 completed\n────────────\n❯ sequence the fixes, critical first\n────────────\n  ~/projects/arpg  claude  75%\n  ⏵⏵ auto mode on\n";
        assert_eq!(match_state(AgentKind::Claude, screen), AgentState::Idle);
    }

    #[test]
    fn spinner_outside_the_live_region_stays_idle() {
        let mut screen = String::from("✻ Sublimating… (1s · ↓ 9 tokens)\n");
        for _ in 0..24 {
            screen.push_str("filler transcript line\n");
        }
        screen.push_str("────────────\n❯ \n────────────\n  ~/p  claude\n");
        assert_eq!(match_state(AgentKind::Claude, &screen), AgentState::Idle);
    }

    #[test]
    fn codex_ignores_a_claude_spinner() {
        let screen = "· Channeling… (1s · ↓ 21 tokens)\n› \n";
        assert_eq!(match_state(AgentKind::Codex, screen), AgentState::Idle);
    }

    #[test]
    fn empty_screen_is_idle() {
        assert_eq!(match_state(AgentKind::Claude, ""), AgentState::Idle);
        assert_eq!(match_state(AgentKind::Codex, ""), AgentState::Idle);
        assert_eq!(match_state(AgentKind::Kimi, ""), AgentState::Idle);
    }
}
