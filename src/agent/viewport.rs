pub(crate) const LIVE_LINES: usize = 8;

pub(crate) fn bottom_lines(screen: &str, n: usize) -> Vec<&str> {
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
    use crate::agent::{AgentKind, AgentState, match_state};
    use super::super::state::MUSE_CAPTURES;

    #[test]
    fn muse_history_outside_the_live_region_is_idle() {
        for &(name, _, screen) in MUSE_CAPTURES {
            let screen = format!("{screen}{}", "transcript line\n\n".repeat(LIVE_LINES));
            assert_eq!(
                match_state(AgentKind::Muse, &screen),
                AgentState::Idle,
                "{name}"
            );
        }
        let options =
            "1. Yes, proceed (y)\n2. No, and tell Muse Code what to do differently (esc)\n";
        for (padding, expected) in [(6, AgentState::Blocked), (7, AgentState::Idle)] {
            let screen = format!("{options}{}", "transcript line\n".repeat(padding));
            assert_eq!(match_state(AgentKind::Muse, &screen), expected);
        }
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
    fn codex_menu_options_must_both_fit_in_the_live_region() {
        for (options, expected) in [
            (
                "  1. Dismiss and keep waiting\n  2. Learn more\n",
                AgentState::Working,
            ),
            (
                "  2. Keep current model\n  3. Keep current model (never show again)\n",
                AgentState::Blocked,
            ),
        ] {
            for (filler_count, state) in [(6, expected), (7, AgentState::Idle)] {
                let screen = format!("{options}{}", "transcript line\n\n".repeat(filler_count));
                assert_eq!(match_state(AgentKind::Codex, &screen), state);
            }
        }
    }
}
