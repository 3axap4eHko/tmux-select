#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AgentKind {
    Claude,
    Codex,
}

impl AgentKind {
    pub fn from_name(name: &str) -> Option<AgentKind> {
        match name {
            "claude" => Some(AgentKind::Claude),
            "codex" => Some(AgentKind::Codex),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            AgentKind::Claude => "claude",
            AgentKind::Codex => "codex",
        }
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

const LIVE_LINES: usize = 20;

pub fn match_state(kind: AgentKind, screen: &str) -> AgentState {
    let live = bottom_lines(screen, LIVE_LINES);

    if live.iter().any(|&line| is_blocked(kind, line)) {
        return AgentState::Blocked;
    }
    if live.iter().any(|&line| is_working(kind, line)) {
        return AgentState::Working;
    }
    AgentState::Idle
}

fn is_working(kind: AgentKind, line: &str) -> bool {
    match kind {
        AgentKind::Claude => line.contains("ing… (") || line.trim_start().starts_with('◯'),
        AgentKind::Codex => {
            line.contains("to interrupt)") || line.contains("background terminal running")
        }
    }
}

fn is_blocked(kind: AgentKind, line: &str) -> bool {
    match kind {
        AgentKind::Claude => line.contains("Enter to select"),
        AgentKind::Codex => {
            line.contains("to submit answer")
                || line.contains("to submit all")
                || (line.contains("to confirm or") && line.contains("to cancel"))
        }
    }
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
        assert_eq!(AgentKind::from_name("npm"), None);
        assert_eq!(AgentKind::from_name("node"), None);
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
    }
}
