use super::identity::AgentKind;

const CLAUDE_SPINNER: [char; 7] = ['·', '✢', '✳', '✶', '✻', '✽', '*'];
const KIMI_MOON_SPINNER: [char; 8] = ['🌑', '🌒', '🌓', '🌔', '🌕', '🌖', '🌗', '🌘'];
const KIMI_BRAILLE_SPINNER: [char; 10] = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];
const PI_BRAILLE_SPINNER: [char; 10] = [
    '\u{280b}', '\u{2819}', '\u{2839}', '\u{2838}', '\u{283c}', '\u{2834}', '\u{2826}', '\u{2827}',
    '\u{2807}', '\u{280f}',
];
const GROK_BRAILLE_SPINNER: [char; 8] = [
    '\u{280b}', '\u{2819}', '\u{2839}', '\u{2838}', '\u{283c}', '\u{2834}', '\u{2826}', '\u{2827}',
];
const KIMI_SWARM_ACTIVITY: [&str; 5] = [
    "Orchestrating...",
    "Prompting...",
    "Working...",
    "Queued...",
    "Rate limited...",
];

pub(crate) fn is_working(kind: AgentKind, line: &str) -> bool {
    match kind {
        AgentKind::Claude => {
            let head = line.trim_start();
            (head.starts_with(CLAUDE_SPINNER) && head.contains("… (")) || head.starts_with('◯')
        }
        AgentKind::Codex => line.contains("to interrupt)"),
        AgentKind::Kimi => kimi_is_working(line),
        AgentKind::Pi => pi_is_working(line),
        AgentKind::OpenCode => opencode_is_working(line),
        AgentKind::Qwen => qwen_is_working(line),
        AgentKind::Grok => grok_is_working(line),
        AgentKind::Muse => {
            let head = line.trim();
            (head.starts_with("\u{25c6} ")
                || head.starts_with("\u{25c7} ")
                || head.starts_with("\u{25c8} "))
                && head.contains(" (")
                && head.ends_with(" \u{b7} esc to interrupt)")
        }
    }
}

fn pi_is_working(line: &str) -> bool {
    let head = line.trim_start();
    let mut chars = head.chars();
    let Some(frame) = chars.next() else {
        return false;
    };
    if !PI_BRAILLE_SPINNER.contains(&frame) {
        return false;
    }
    let status = chars.as_str().trim_start();
    status.starts_with("Working...")
        || status.starts_with("Retrying (")
        || status.starts_with("Compacting context...")
        || status.starts_with("Auto-compacting...")
        || status.starts_with("Context overflow detected, Auto-compacting...")
        || status.starts_with("Summarizing branch...")
}

fn opencode_is_working(line: &str) -> bool {
    line.contains("esc interrupt") || line.contains("esc again to interrupt")
}

fn qwen_is_working(line: &str) -> bool {
    let Some((_, footer)) = line.rsplit_once('(') else {
        return false;
    };
    let Some(status) = footer.strip_suffix(" \u{b7} esc to cancel)") else {
        return false;
    };
    let elapsed = status
        .split_once(" \u{b7} ")
        .map_or(status, |(time, _)| time);
    let mut parts = elapsed.split_whitespace();
    let Some(first) = parts.next() else {
        return false;
    };
    qwen_is_duration_part(first) && parts.all(qwen_is_duration_part)
}

fn qwen_is_duration_part(part: &str) -> bool {
    let bytes = part.as_bytes();
    bytes.len() > 1
        && matches!(bytes.last(), Some(b'h' | b'm' | b's'))
        && bytes[..bytes.len() - 1].iter().all(u8::is_ascii_digit)
}

fn grok_is_working(line: &str) -> bool {
    let head = line.trim_start();
    if head.contains("[stop]")
        || head.contains(" still running")
        || head.contains("send a message to interrupt")
    {
        return true;
    }
    head.chars()
        .next()
        .is_some_and(|frame| GROK_BRAILLE_SPINNER.contains(&frame))
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

pub(crate) fn kimi_has_active_swarm(lines: &[&str]) -> bool {
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

#[cfg(test)]
pub(crate) fn claude_frame(body: &str) -> String {
    format!("{body}\n────────────\n❯ \n────────────\n  ~/projects/x  claude\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{AgentKind, AgentState, match_state};

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
    fn frozen_transcript_is_idle() {
        let screen = "✻ Crunched for 6m 41s\n✻ Waiting for 1 dynamic workflow to finish\n● Dynamic workflow \"x\" completed · 11s\n✻ Churned for 43s\n  7 tasks (6 done, 1 open)\n  ◻ a\n  ✔ b\n  ✔ c\n  … +2 completed\n────────────\n❯ sequence the fixes, critical first\n────────────\n  ~/projects/arpg  claude  75%\n  ⏵⏵ auto mode on\n";
        assert_eq!(match_state(AgentKind::Claude, screen), AgentState::Idle);
    }

    #[test]
    fn codex_working_line_is_working() {
        for line in [
            "• Working (13s • esc to interrupt) · 1 background terminal running · /ps to view · /stop to close",
            "Working (0s • esc to interrupt)",
            "• Thinking (5s • esc to interrupt)",
            "• Reviewing approval request (2s • esc to interrupt)",
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
    fn codex_background_terminals_do_not_determine_turn_state() {
        for footer in [
            "1 background terminal running \u{b7} /ps to view \u{b7} /stop to close",
            "2 background terminals running \u{b7} /ps to view \u{b7} /stop to close",
        ] {
            for (status, expected) in [
                ("", AgentState::Idle),
                ("\u{2500} Worked for 1h 06m 28s \u{2500}", AgentState::Idle),
                ("Working (13s \u{b7} esc to interrupt)", AgentState::Working),
                (
                    "Press enter to confirm or esc to cancel",
                    AgentState::Blocked,
                ),
            ] {
                let screen = format!("{status}\n\n  {footer}\n\u{203a} \n");
                assert_eq!(match_state(AgentKind::Codex, &screen), expected, "{screen}");
            }
        }
    }

    #[test]
    fn codex_plain_prompt_is_idle() {
        let screen = "$ cargo build\nbuild succeeded\n› \nctrl+t to view transcript\n";
        assert_eq!(match_state(AgentKind::Codex, screen), AgentState::Idle);
    }

    #[test]
    fn codex_generic_settings_popup_is_idle() {
        let screen = "Select model\n  gpt-5\n  o3\nPress enter to confirm or esc to go back\n";
        assert_eq!(match_state(AgentKind::Codex, screen), AgentState::Idle);
    }

    #[test]
    fn codex_ignores_a_claude_spinner() {
        let screen = "· Channeling… (1s · ↓ 21 tokens)\n› \n";
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
    fn pi_loader_activity_is_working() {
        for frame in PI_BRAILLE_SPINNER {
            for status in [
                "Working...",
                "Working... (esc to interrupt)",
                "Retrying (1/3) in 2s...",
                "Compacting context...",
                "Auto-compacting...",
                "Context overflow detected, Auto-compacting... (esc to cancel)",
                "Summarizing branch...",
            ] {
                let screen = format!("tool output\n  {frame} {status}\n> \n");
                assert_eq!(
                    match_state(AgentKind::Pi, &screen),
                    AgentState::Working,
                    "{frame} {status}"
                );
            }
        }
    }

    #[test]
    fn pi_unrelated_loader_is_idle() {
        let screen = format!("{} Downloading update\n> \n", PI_BRAILLE_SPINNER[0]);
        assert_eq!(match_state(AgentKind::Pi, &screen), AgentState::Idle);
    }

    #[test]
    fn pi_overflow_compaction_text_without_spinner_is_idle() {
        let screen = "Context overflow detected, Auto-compacting... (esc to cancel)\n> \n";
        assert_eq!(match_state(AgentKind::Pi, screen), AgentState::Idle);
    }

    #[test]
    fn opencode_busy_prompt_is_working() {
        for footer in ["esc interrupt", "esc again to interrupt"] {
            let screen = format!("Build project\n{footer}\n");
            assert_eq!(
                match_state(AgentKind::OpenCode, &screen),
                AgentState::Working,
                "{footer}"
            );
        }
    }

    #[test]
    fn opencode_plain_prompt_is_idle() {
        assert_eq!(
            match_state(AgentKind::OpenCode, "Ask anything\nctrl+x leader\n"),
            AgentState::Idle
        );
    }

    #[test]
    fn qwen_responding_status_is_working() {
        for status in [
            ". Working (4s \u{b7} esc to cancel)",
            ".. Responding (12s \u{b7} \u{2193} 641 tokens \u{b7} esc to cancel)",
            ".. Responding (1m 12s \u{b7} \u{2193} 1.2k tokens \u{b7} 16 t/s \u{b7} esc to cancel)",
        ] {
            assert_eq!(
                match_state(AgentKind::Qwen, &format!("{status}\n")),
                AgentState::Working,
                "{status}"
            );
        }
    }

    #[test]
    fn qwen_non_progress_status_is_idle() {
        for status in [". Waiting", "(Esc to cancel)"] {
            assert_eq!(
                match_state(AgentKind::Qwen, &format!("{status}\n")),
                AgentState::Idle,
                "{status}"
            );
        }
    }

    #[test]
    fn grok_turn_activity_is_working() {
        for line in [
            format!("{} Thinking... 2s [stop]", GROK_BRAILLE_SPINNER[0]),
            format!("{} Starting session... 0:01", GROK_BRAILLE_SPINNER[1]),
            "waiting - send a message to interrupt".to_string(),
            "1 command still running".to_string(),
        ] {
            assert_eq!(
                match_state(AgentKind::Grok, &format!("{line}\n")),
                AgentState::Working,
                "{line}"
            );
        }
    }

    #[test]
    fn grok_idle_edit_prompt_is_idle() {
        assert_eq!(
            match_state(
                AgentKind::Grok,
                "\u{25c6} agent idle - waiting on your edit\n"
            ),
            AgentState::Idle
        );
    }
}
