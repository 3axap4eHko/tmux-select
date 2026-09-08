pub(crate) fn has_menu_options(lines: &[&str], first: &str, second: &str) -> bool {
    let mut options = lines.iter().copied().filter_map(numbered_menu_option);
    options.any(|option| option == first) && options.any(|option| option == second)
}

pub(crate) fn numbered_menu_option(line: &str) -> Option<&str> {
    let head = line.trim();
    let head = head.strip_prefix("\u{203a} ").unwrap_or(head);
    let (number, text) = head.split_once(". ")?;
    if number.is_empty() || !number.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    Some(text.split_once("  ").map_or(text, |(label, _)| label))
}

// Preserve upstream wrapping and column spacing so fixtures catch label/description overlap.
// https://github.com/openai/codex/tree/b1a547b1f73ce86205d9222ac19cff334b3b7a2e/codex-rs/tui/src/chatwidget/snapshots
#[cfg(test)]
pub(crate) const CODEX_WAITING_WITH_RETRY: &str = concat!(
    "  Our systems are thinking a bit more about this request before responding.\n",
    "  Hang tight or retry with a faster model for a quicker response, though it\n",
    "  may be less capable of handling complex requests.\n",
    "\n",
    "\u{203a} 1. Retry with a faster model\n",
    "  2. Dismiss and keep waiting\n",
    "  3. Learn more\n",
    "\n",
    "  No action is required. Codex will keep waiting, and this menu will close when\n",
    "  the response is ready.\n",
);
#[cfg(test)]
pub(crate) const CODEX_WAITING_WITHOUT_RETRY: &str = concat!(
    "  Our systems are thinking a bit more about this request before responding.\n",
    "\n",
    "\u{203a} 1. Dismiss and keep waiting\n",
    "  2. Learn more\n",
    "\n",
    "  No action is required. Codex will keep waiting, and this menu will close when\n",
    "  the response is ready.\n",
);
#[cfg(test)]
pub(crate) const CODEX_RATE_LIMIT_SWITCH: &str = concat!(
    "  Approaching rate limits\n",
    "  Switch to gpt-5.6-luna for lower credit usage?\n",
    "\n",
    "\u{203a} 1. Switch to gpt-5.6-luna                 Fast and affordable agentic coding\n",
    "                                            model.\n",
    "  2. Keep current model\n",
    "  3. Keep current model (never show again)  Hide future rate limit reminders\n",
    "                                            about switching models.\n",
    "\n",
    "  Press enter to confirm or esc to go back\n",
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{AgentKind, AgentState, match_state};

    #[test]
    fn codex_menus_require_complete_numbered_labels_in_order() {
        for screen in [
            "  1. Learn more\n  2. Dismiss and keep waiting\n",
            "  2. Keep current model (never show again)\n  3. Keep current model\n",
            "  2. Keep current model (never show again)\n  3. Keep current model (never show again)\n",
            "  1. Dismiss and keep waiting now\n  2. Learn more\n",
            "  1. Dismiss and keep waiting\n  2. Learn more about models\n",
            "  2. Keep current model settings\n  3. Keep current model (never show again)\n",
            "  2. Keep current model\n  3. Keep current model (never show again) please\n",
            "  x. Dismiss and keep waiting\n  2. Learn more\n",
            "  . Dismiss and keep waiting\n  2. Learn more\n",
            "  2. Keep current model\n  x. Keep current model (never show again)\n",
        ] {
            assert_eq!(
                match_state(AgentKind::Codex, screen),
                AgentState::Idle,
                "{screen}"
            );
        }
    }

    #[test]
    fn codex_isolated_menu_lines_and_unnumbered_prose_are_idle() {
        for screen in [
            CODEX_WAITING_WITH_RETRY,
            CODEX_WAITING_WITHOUT_RETRY,
            CODEX_RATE_LIMIT_SWITCH,
        ] {
            for line in screen.lines() {
                assert_eq!(
                    match_state(AgentKind::Codex, line),
                    AgentState::Idle,
                    "{line}"
                );
            }
            let prose = screen
                .replace("\u{203a} 1. ", "")
                .replace("  2. ", "")
                .replace("  3. ", "");
            assert_eq!(match_state(AgentKind::Codex, &prose), AgentState::Idle);
        }
    }
}
