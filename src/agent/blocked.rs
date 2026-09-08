use super::identity::AgentKind;
use super::menu::{has_menu_options, numbered_menu_option};

pub(crate) fn is_blocked(kind: AgentKind, line: &str) -> bool {
    match kind {
        AgentKind::Claude => line.contains("Esc to cancel") && line.contains('·'),
        AgentKind::Codex => {
            line.contains("to submit answer")
                || line.contains("to submit all")
                || (line.contains("to confirm or") && line.contains("to cancel"))
        }
        AgentKind::Kimi => kimi_is_blocked(line),
        AgentKind::Pi => pi_is_blocked(line),
        AgentKind::OpenCode => opencode_is_blocked(line),
        AgentKind::Qwen => qwen_is_blocked(line),
        AgentKind::Grok => grok_is_blocked(line),
        AgentKind::Muse => line.trim() == "Use Up/Down or 1/2, then Enter. Esc quits.",
    }
}

pub(crate) fn muse_is_blocked(lines: &[&str]) -> bool {
    let question = lines
        .iter()
        .position(|line| {
            let head = line.trim_start();
            head.starts_with("Enter to select \u{b7} ")
                || head.starts_with("Enter to toggle \u{b7} ")
        })
        .is_some_and(|start| {
            let hints = lines
                .iter()
                .skip(start)
                .map(|line| line.trim())
                .collect::<Vec<_>>()
                .join(" ");
            hints.contains("Tab for an optional note") && hints.contains("Esc to interrupt")
        });
    if question
        || has_menu_options(
            lines,
            "Allow this stage once (y)",
            "Abort the entire command (esc)",
        )
    {
        return true;
    }
    let mut options = lines.iter().copied().filter_map(numbered_menu_option);
    // Muse truncates this rejection label at 48 columns instead of wrapping it.
    options.any(|option| option == "Yes, proceed (y)")
        && options.any(|option| option.starts_with("No, and tell Muse Code "))
}

fn pi_is_blocked(line: &str) -> bool {
    let head = line.trim();
    (head.starts_with("\u{2191}\u{2193} navigate")
        && head.contains(" select")
        && head.ends_with(" cancel"))
        || (head.contains(" submit  ") && head.ends_with(" cancel"))
        || head
            .split_once(" submit  ")
            .and_then(|(_, hints)| hints.split_once(" newline  "))
            .and_then(|(_, hints)| hints.split_once(" cancel  "))
            .is_some_and(|(_, hint)| hint.ends_with(" external editor"))
}

fn opencode_is_blocked(line: &str) -> bool {
    let head = line.trim();
    head.contains("Permission required")
        || head.contains("Reject permission")
        || head == "Always allow"
        || (head.contains("enter ") && head.contains("esc dismiss"))
}

fn qwen_is_blocked(line: &str) -> bool {
    line.contains("No, suggest changes (esc)")
        || line.contains("No, keep planning (esc)")
        || line.contains("Save and close external editor to continue")
        || line.contains("A potential loop was detected")
        || (line.contains("Enter to confirm") && line.contains("Esc to cancel"))
        || (line.contains("Enter: Select") && line.contains("Esc: Cancel"))
        || (line.contains("Enter: Confirm") && line.contains("Esc: Cancel"))
}

fn grok_is_blocked(line: &str) -> bool {
    let head = line.trim_start();
    let waiting_diamond = head.starts_with('\u{25c6}') || head.starts_with('\u{2666}');
    (waiting_diamond && head.contains("[stop]"))
        || head.contains("Waiting on plan approval")
        || head.contains("No plan written: approve or request changes")
        || (head.contains("always-approve") && head.contains("cancel"))
        || (head.contains("next answer") && head.contains("dismiss"))
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

#[cfg(test)]
mod tests {
    use crate::agent::{AgentKind, AgentState, match_state};
    use super::super::state::MUSE_CAPTURES;
    use super::super::working::claude_frame;

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
    fn pi_extension_prompts_are_blocked() {
        for footer in [
            "\u{2191}\u{2193} navigate  enter select  esc cancel",
            "enter submit  esc cancel",
            "enter submit  shift+enter newline  esc cancel  ctrl+g external editor",
            "ctrl+s submit  ctrl+j newline  ctrl+c cancel  ctrl+e external editor",
        ] {
            let screen = format!("Extension prompt\n  option\n{footer}\n");
            assert_eq!(
                match_state(AgentKind::Pi, &screen),
                AgentState::Blocked,
                "{footer}"
            );
        }
    }

    #[test]
    fn opencode_permission_and_question_prompts_are_blocked() {
        for marker in [
            "Permission required",
            "Reject permission",
            "Always allow",
            "enter submit  esc dismiss",
            "enter toggle  esc dismiss",
            "enter confirm  esc dismiss",
        ] {
            let screen = format!("prompt\n{marker}\n");
            assert_eq!(
                match_state(AgentKind::OpenCode, &screen),
                AgentState::Blocked,
                "{marker}"
            );
        }
    }

    #[test]
    fn qwen_confirmation_prompts_are_blocked() {
        for marker in [
            "No, suggest changes (esc)",
            "No, keep planning (esc)",
            "Save and close external editor to continue",
            "A potential loop was detected",
            "Enter to confirm, Esc to cancel",
            "Enter to confirm \u{b7} Esc to cancel",
            "Up/Down: Navigate | Enter: Select | Esc: Cancel",
            "Up/Down: Navigate | Enter: Confirm | Esc: Cancel",
        ] {
            assert_eq!(
                match_state(AgentKind::Qwen, &format!("prompt\n{marker}\n")),
                AgentState::Blocked,
                "{marker}"
            );
        }
    }

    #[test]
    fn grok_user_waits_are_blocked() {
        for line in [
            "\u{25c6} Run command 2s [stop]",
            "Waiting on plan approval",
            "No plan written: approve or request changes",
            "1-4 select  Ctrl+O always-approve  Ctrl+C cancel",
            "Tab next answer  Esc back  X dismiss",
        ] {
            assert_eq!(
                match_state(AgentKind::Grok, &format!("{line}\n")),
                AgentState::Blocked,
                "{line}"
            );
        }
    }

    #[test]
    fn muse_approval_selection_does_not_change_state() {
        for &(name, expected, screen) in MUSE_CAPTURES {
            if expected != AgentState::Blocked
                || !(name.starts_with("approval") || name.starts_with("escalation"))
            {
                continue;
            }
            let unselected = screen.replace('\u{203a}', " ");
            for selected in 1..=3 {
                let selected_screen = unselected.replacen(
                    &format!("  {selected}. "),
                    &format!("\u{203a} {selected}. "),
                    1,
                );
                assert_eq!(
                    match_state(AgentKind::Muse, &selected_screen),
                    expected,
                    "{name}, selection {selected}"
                );
            }
        }
    }

    #[test]
    fn muse_approval_requires_paired_numbered_options_in_order() {
        for screen in [
            "1. Yes, proceed (y)",
            "2. No, and tell Muse Code what to do differently (esc)",
            "1. Allow this stage once (y)",
            "3. Abort the entire command (esc)",
            "Yes, proceed (y)\nNo, and tell Muse Code what to do differently (esc)",
            "1. No, and tell Muse Code what to do differently (esc)\n2. Yes, proceed (y)",
            "1. Abort the entire command (esc)\n2. Allow this stage once (y)",
            "x. Yes, proceed (y)\n2. No, and tell Muse Code what to do differently (esc)",
            "1. Yes, proceed (y) later\n2. No, and tell Muse Code what to do differently (esc)",
        ] {
            assert_eq!(
                match_state(AgentKind::Muse, screen),
                AgentState::Idle,
                "{screen}"
            );
        }
    }

    #[test]
    fn muse_incomplete_hints_and_completed_diamonds_are_idle() {
        for screen in [
            "\u{25c6} Local probe complete.",
            "\u{25c7} Thinking",
            "Thinking (0s \u{b7} esc to interrupt)",
            "\u{25c6} esc to interrupt)",
            "Enter to select \u{b7} Esc to interrupt",
            "Tab for an optional note \u{b7} Esc to interrupt",
            "Enter to select \u{b7} Tab for an optional note",
            "Select model\nPress enter to confirm or esc to go back",
            "",
        ] {
            assert_eq!(
                match_state(AgentKind::Muse, screen),
                AgentState::Idle,
                "{screen}"
            );
        }
    }
}
