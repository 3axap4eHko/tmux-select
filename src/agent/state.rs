use super::blocked::{is_blocked, muse_is_blocked};
use super::identity::AgentKind;
use super::menu::has_menu_options;
use super::viewport::{LIVE_LINES, bottom_lines};
use super::working::{is_working, kimi_has_active_swarm};

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

pub fn match_state(kind: AgentKind, screen: &str) -> AgentState {
    let live = bottom_lines(screen, LIVE_LINES);

    if live.iter().any(|&line| is_blocked(kind, line))
        || (kind == AgentKind::Codex
            && has_menu_options(
                &live,
                "Keep current model",
                "Keep current model (never show again)",
            ))
        || (kind == AgentKind::Muse && muse_is_blocked(&live))
    {
        return AgentState::Blocked;
    }
    if live.iter().any(|&line| is_working(kind, line))
        || (kind == AgentKind::Codex
            && has_menu_options(&live, "Dismiss and keep waiting", "Learn more"))
        || (kind == AgentKind::Kimi && kimi_has_active_swarm(&live))
    {
        return AgentState::Working;
    }
    AgentState::Idle
}

// Preserve native Muse 1.0.3-R2198.1 wrapping and live-line counts: narrow
// dialogs hide their headers. ASCII borders keep the escaped fixtures readable.
#[cfg(test)]
pub(crate) const MUSE_CAPTURES: &[(&str, AgentState, &str)] = &[
    (
        "approval-48.txt",
        AgentState::Blocked,
        concat!(
            "  $ pwd\n",
            "  Stage 1/1\n",
            "  Current argv: [\"pwd\"]\n",
            "\u{203a} 1. Allow this stage once (y)\n",
            "  2. Always allow in this workspace: pwd ... (p)\n",
            "  3. Abort the entire command (esc)\n",
            "------------------------------------------------\n",
            "  muse-spark-1.2 \u{b7} high \u{b7} \u{2026}/workspace\n",
        ),
    ),
    (
        "approval-80.txt",
        AgentState::Blocked,
        concat!(
            "  $ pwd\n",
            "  Stage 1/1\n",
            "  Current argv: [\"pwd\"]\n",
            "\u{203a} 1. Allow this stage once (y)\n",
            "  2. Always allow in this workspace: pwd ... (p)\n",
            "  3. Abort the entire command (esc)\n",
            "--------------------------------------------------------------------------------\n",
            "  muse-spark-1.2 \u{b7} high \u{b7} /t/a/muse-dialog-captures-459c3eb1b710fed1/workspace\n",
        ),
    ),
    (
        "approval-interaction-48.txt",
        AgentState::Idle,
        concat!(
            "\u{27e9} Display the local approval probe\n",
            "\u{25c6} Ran pwd \u{2014} denied\n",
            "  \u{2514} approval aborted\n",
            "\u{25c6} Local probe complete.\n",
            "------------------------------------------------\n",
            "\u{27e9}\n",
            "------------------------------------------------\n",
            "  muse-spark-1.2 \u{b7} high \u{b7} \u{2026}/workspace\n",
        ),
    ),
    (
        "approval-interaction-80.txt",
        AgentState::Idle,
        concat!(
            "\u{27e9} Display the local approval probe\n",
            "\u{25c6} Ran pwd \u{2014} denied\n",
            "  \u{2514} approval aborted\n",
            "\u{25c6} Local probe complete.\n",
            "--------------------------------------------------------------------------------\n",
            "\u{27e9}\n",
            "--------------------------------------------------------------------------------\n",
            "  muse-spark-1.2 \u{b7} high \u{b7} /t/a/muse-dialog-captures-459c3eb1b710fed1/workspace\n",
        ),
    ),
    (
        "escalation-48.txt",
        AgentState::Blocked,
        concat!(
            "\u{25c6} Calling tools (0s \u{b7} esc to interrupt)\n",
            "------------------------------------------------\n",
            "Would you like to run the following command?\n",
            "  $ pwd\n",
            "\u{203a} 1. Yes, proceed (y)\n",
            "  2. No, and tell Muse Code what to do different\n",
            "------------------------------------------------\n",
            "  muse-spark-1.2 \u{b7} high \u{b7} \u{2026}/workspace\n",
        ),
    ),
    (
        "escalation-80.txt",
        AgentState::Blocked,
        concat!(
            "\u{25c7} Calling tools (0s \u{b7} esc to interrupt)\n",
            "--------------------------------------------------------------------------------\n",
            "Would you like to run the following command?\n",
            "  $ pwd\n",
            "\u{203a} 1. Yes, proceed (y)\n",
            "  2. No, and tell Muse Code what to do differently (esc)\n",
            "--------------------------------------------------------------------------------\n",
            "  muse-spark-1.2 \u{b7} high \u{b7} /t/a/muse-dialog-captures-459c3eb1b710fed1/workspace\n",
        ),
    ),
    (
        "escalation-interaction-48.txt",
        AgentState::Idle,
        concat!(
            "\u{27e9} Display the local escalation probe\n",
            "\u{25c6} Ran pwd \u{2014} denied\n",
            "  \u{2514} approval aborted\n",
            "\u{25c6} Local probe complete.\n",
            "------------------------------------------------\n",
            "\u{27e9}\n",
            "------------------------------------------------\n",
            "  muse-spark-1.2 \u{b7} high \u{b7} \u{2026}/workspace\n",
        ),
    ),
    (
        "escalation-interaction-80.txt",
        AgentState::Idle,
        concat!(
            "\u{27e9} Display the local escalation probe\n",
            "\u{25c6} Ran pwd \u{2014} denied\n",
            "  \u{2514} approval aborted\n",
            "\u{25c6} Local probe complete.\n",
            "--------------------------------------------------------------------------------\n",
            "\u{27e9}\n",
            "--------------------------------------------------------------------------------\n",
            "  muse-spark-1.2 \u{b7} high \u{b7} /t/a/muse-dialog-captures-459c3eb1b710fed1/workspace\n",
        ),
    ),
    (
        "multiple-48.txt",
        AgentState::Blocked,
        concat!(
            "    4. Submit answer (0 checked)\n",
            "  Enter to toggle \u{b7} Submit row to continue \u{b7} \u{2191}/\u{2193}\n",
            "  to move \u{b7} Tab for an optional note \u{b7} Esc to\n",
            "  interrupt\n",
            "------------------------------------------------\n",
            "\u{27e9}\n",
            "------------------------------------------------\n",
            "  muse-spark-1.2 \u{b7} high \u{b7} \u{2026}/workspace\n",
        ),
    ),
    (
        "multiple-80.txt",
        AgentState::Blocked,
        concat!(
            "    3. [ ] None of the above  Optionally, add details in notes (tab).\n",
            "    4. Submit answer (0 checked)\n",
            "  Enter to toggle \u{b7} Submit row to continue \u{b7} \u{2191}/\u{2193} to move \u{b7} Tab for an optional\n",
            "  note \u{b7} Esc to interrupt\n",
            "--------------------------------------------------------------------------------\n",
            "\u{27e9}\n",
            "--------------------------------------------------------------------------------\n",
            "  muse-spark-1.2 \u{b7} high \u{b7} /t/a/muse-dialog-captures-459c3eb1b710fed1/workspace\n",
        ),
    ),
    (
        "questions-48.txt",
        AgentState::Blocked,
        concat!(
            "  1 of 2\n",
            "  Enter to select \u{b7} \u{2191}/\u{2193} to move \u{b7} \u{2190}/\u{2192} to switch\n",
            "  question \u{b7} Tab for an optional note \u{b7} Esc to\n",
            "  interrupt\n",
            "------------------------------------------------\n",
            "\u{27e9}\n",
            "------------------------------------------------\n",
            "  muse-spark-1.2 \u{b7} high \u{b7} \u{2026}/workspace\n",
        ),
    ),
    (
        "questions-80.txt",
        AgentState::Blocked,
        concat!(
            "    3. None of the above  Optionally, add details in notes (tab).\n",
            "  1 of 2\n",
            "  Enter to select \u{b7} \u{2191}/\u{2193} to move \u{b7} \u{2190}/\u{2192} to switch question \u{b7} Tab for an optional\n",
            "  note \u{b7} Esc to interrupt\n",
            "--------------------------------------------------------------------------------\n",
            "\u{27e9}\n",
            "--------------------------------------------------------------------------------\n",
            "  muse-spark-1.2 \u{b7} high \u{b7} /t/a/muse-dialog-captures-459c3eb1b710fed1/workspace\n",
        ),
    ),
    (
        "running-48",
        AgentState::Working,
        concat!(
            "  Muse Code 1.0.3\n",
            "\u{27e9} status detection probe\n",
            "\u{25c7} Thinking (1s \u{b7} esc to interrupt)\n",
            "------------------------------------------------\n",
            "\u{27e9}\n",
            "------------------------------------------------\n",
            "  echo \u{b7} \u{2026}/workspace\n",
        ),
    ),
    (
        "running-80",
        AgentState::Working,
        concat!(
            "  Muse Code 1.0.3\n",
            "\u{27e9} status detection probe\n",
            "\u{25c6} Thinking (0s \u{b7} esc to interrupt)\n",
            "--------------------------------------------------------------------------------\n",
            "\u{27e9}\n",
            "--------------------------------------------------------------------------------\n",
            "  echo \u{b7} /tmp/agents/muse-echo-captures-c7025c1179c99bb4/workspace\n",
        ),
    ),
    (
        "running-third-diamond-80",
        AgentState::Working,
        concat!(
            "  Including your Claude Code personal rules and 19 skills \u{2014} manage with\n",
            "  /settings.\n",
            "\u{27e9} Local spinner probe\n",
            "\u{25c8} Thinking (0s \u{b7} esc to interrupt)\n",
            "--------------------------------------------------------------------------------\n",
            "\u{27e9}\n",
            "--------------------------------------------------------------------------------\n",
            "  muse-spark-1.2 \u{b7} high \u{b7} /t/a/muse-dialog-captures-eda35ac8e55b9f0d/workspace\n",
        ),
    ),
    (
        "settled-48",
        AgentState::Idle,
        concat!(
            "  Muse Code 1.0.3\n",
            "\u{27e9} status detection probe\n",
            "\u{25c6} echo: status detection probe\n",
            "------------------------------------------------\n",
            "\u{27e9}\n",
            "------------------------------------------------\n",
            "  echo \u{b7} \u{2026}/workspace\n",
        ),
    ),
    (
        "settled-80",
        AgentState::Idle,
        concat!(
            "  Muse Code 1.0.3\n",
            "\u{27e9} status detection probe\n",
            "\u{25c6} echo: status detection probe\n",
            "--------------------------------------------------------------------------------\n",
            "\u{27e9}\n",
            "--------------------------------------------------------------------------------\n",
            "  echo \u{b7} /tmp/agents/muse-echo-captures-c7025c1179c99bb4/workspace\n",
        ),
    ),
    (
        "single-48.txt",
        AgentState::Blocked,
        concat!(
            "                          details in notes\n",
            "                          (tab).\n",
            "  Enter to select \u{b7} \u{2191}/\u{2193} to move \u{b7} Tab for an\n",
            "  optional note \u{b7} Esc to interrupt\n",
            "------------------------------------------------\n",
            "\u{27e9}\n",
            "------------------------------------------------\n",
            "  muse-spark-1.2 \u{b7} high \u{b7} \u{2026}/workspace\n",
        ),
    ),
    (
        "single-80.txt",
        AgentState::Blocked,
        concat!(
            "  \u{203a} 1. Alpha              First local test choice\n",
            "    2. Beta               Second local test choice\n",
            "    3. None of the above  Optionally, add details in notes (tab).\n",
            "  Enter to select \u{b7} \u{2191}/\u{2193} to move \u{b7} Tab for an optional note \u{b7} Esc to interrupt\n",
            "--------------------------------------------------------------------------------\n",
            "\u{27e9}\n",
            "--------------------------------------------------------------------------------\n",
            "  muse-spark-1.2 \u{b7} high \u{b7} /t/a/muse-dialog-captures-459c3eb1b710fed1/workspace\n",
        ),
    ),
    (
        "single-interaction-80.txt",
        AgentState::Blocked,
        concat!(
            "      Note (optional): \u{258c}\n",
            "    2. Beta               Second local test choice\n",
            "    3. None of the above  Optionally, add details in notes (tab).\n",
            "  Enter to select \u{b7} \u{2191}/\u{2193} to move \u{b7} Tab for an optional note \u{b7} Esc to interrupt\n",
            "--------------------------------------------------------------------------------\n",
            "\u{27e9}\n",
            "--------------------------------------------------------------------------------\n",
            "  muse-spark-1.2 \u{b7} high \u{b7} /t/a/muse-dialog-captures-459c3eb1b710fed1/workspace\n",
        ),
    ),
    (
        "trust-80",
        AgentState::Blocked,
        concat!(
            "Do you trust this workspace?\n",
            "Workspace: /tmp/agents/tmux-select-muse-research-bcd48735accbd020/workspace\n",
            "Trusting allows project-local skills, rules, hooks, and plugin config to load\n",
            "before the model runs.\n",
            "Only trust this workspace when you trust its contents.\n",
            "> 1  Trust and continue\n",
            "  2  Quit\n",
            "Use Up/Down or 1/2, then Enter. Esc quits.\n",
        ),
    ),
];

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::menu::{
        CODEX_RATE_LIMIT_SWITCH, CODEX_WAITING_WITHOUT_RETRY, CODEX_WAITING_WITH_RETRY,
    };

    #[test]
    fn muse_native_captures_match_state() {
        for &(name, expected, screen) in MUSE_CAPTURES {
            assert_eq!(match_state(AgentKind::Muse, screen), expected, "{name}");
        }
    }

    #[test]
    fn muse_blocked_options_win_over_working() {
        let screen = concat!(
            "\u{25c6} Calling tools (0s \u{b7} esc to interrupt)\n",
            "\u{203a} 1. Yes, proceed (y)\n",
            "  2. No, and tell Muse Code what to do differently (esc)\n",
        );
        assert_eq!(match_state(AgentKind::Muse, screen), AgentState::Blocked);
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
    fn pi_multiline_editor_prompt_wins_over_spinner() {
        let screen = "\u{280b} Working...\nEdit response\n> draft\nenter submit  shift+enter newline  esc cancel  ctrl+g external editor\n";
        assert_eq!(match_state(AgentKind::Pi, screen), AgentState::Blocked);
    }

    #[test]
    fn codex_waiting_menus_are_working() {
        for screen in [CODEX_WAITING_WITH_RETRY, CODEX_WAITING_WITHOUT_RETRY] {
            assert_eq!(match_state(AgentKind::Codex, screen), AgentState::Working);
        }
    }

    #[test]
    fn codex_rate_limit_switch_is_blocked() {
        assert_eq!(
            match_state(AgentKind::Codex, CODEX_RATE_LIMIT_SWITCH),
            AgentState::Blocked
        );
    }

    #[test]
    fn codex_menu_selection_does_not_change_state() {
        for (screen, count, expected) in [
            (CODEX_WAITING_WITH_RETRY, 3, AgentState::Working),
            (CODEX_WAITING_WITHOUT_RETRY, 2, AgentState::Working),
            (CODEX_RATE_LIMIT_SWITCH, 3, AgentState::Blocked),
        ] {
            let unselected = screen.replace('\u{203a}', " ");
            assert_eq!(match_state(AgentKind::Codex, &unselected), expected);
            for selected in 1..=count {
                let screen = unselected.replacen(
                    &format!("  {selected}. "),
                    &format!("\u{203a} {selected}. "),
                    1,
                );
                assert_eq!(match_state(AgentKind::Codex, &screen), expected, "{screen}");
            }
        }
    }

    #[test]
    fn codex_rate_limit_switch_accepts_other_models() {
        let screen = CODEX_RATE_LIMIT_SWITCH.replace("gpt-5.6-luna", "another-model");
        assert_eq!(match_state(AgentKind::Codex, &screen), AgentState::Blocked);
    }

    #[test]
    fn codex_blocked_menus_win_over_working() {
        let switching_while_working = CODEX_RATE_LIMIT_SWITCH.replace(
            "  Approaching rate limits",
            "Working (1s - esc to interrupt)",
        );
        let switching_while_waiting =
            format!("{CODEX_RATE_LIMIT_SWITCH}  1. Dismiss and keep waiting\n  2. Learn more\n");
        let approval_while_waiting =
            format!("{CODEX_WAITING_WITH_RETRY}Press enter to confirm or esc to cancel\n");
        for screen in [
            switching_while_working,
            switching_while_waiting,
            approval_while_waiting,
        ] {
            assert_eq!(match_state(AgentKind::Codex, &screen), AgentState::Blocked);
        }
    }

    #[test]
    fn empty_screen_is_idle() {
        assert_eq!(match_state(AgentKind::Claude, ""), AgentState::Idle);
        assert_eq!(match_state(AgentKind::Codex, ""), AgentState::Idle);
        assert_eq!(match_state(AgentKind::Kimi, ""), AgentState::Idle);
        assert_eq!(match_state(AgentKind::Pi, ""), AgentState::Idle);
        assert_eq!(match_state(AgentKind::OpenCode, ""), AgentState::Idle);
        assert_eq!(match_state(AgentKind::Qwen, ""), AgentState::Idle);
        assert_eq!(match_state(AgentKind::Grok, ""), AgentState::Idle);
    }
}
