#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AgentKind {
    Claude,
    Codex,
    Kimi,
    Pi,
    OpenCode,
    Qwen,
    Grok,
}

impl AgentKind {
    pub fn from_name(name: &str) -> Option<AgentKind> {
        match name {
            "claude" => Some(AgentKind::Claude),
            "codex" => Some(AgentKind::Codex),
            "kimi" => Some(AgentKind::Kimi),
            "pi" => Some(AgentKind::Pi),
            "opencode" => Some(AgentKind::OpenCode),
            "qwen" => Some(AgentKind::Qwen),
            "grok" => Some(AgentKind::Grok),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            AgentKind::Claude => "claude",
            AgentKind::Codex => "codex",
            AgentKind::Kimi => "kimi",
            AgentKind::Pi => "pi",
            AgentKind::OpenCode => "opencode",
            AgentKind::Qwen => "qwen",
            AgentKind::Grok => "grok",
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
        if path.ends_with("/qwen-code/node/bin/node") || is_qwen_entry_path(path) {
            return Some(AgentKind::Qwen);
        }
        if !is_version_shaped(basename) {
            return None;
        }
        components.find_map(AgentKind::from_name)
    }
}

pub(crate) fn is_qwen_entry_path(path: &str) -> bool {
    path.ends_with("/qwen-code/lib/cli-entry.js")
        || path.ends_with("/qwen-code/lib/cli.js")
        || path.ends_with("/@qwen-code/qwen-code/cli-entry.js")
        || path.ends_with("/@qwen-code/qwen-code/cli.js")
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

pub fn match_state(kind: AgentKind, screen: &str) -> AgentState {
    let live = bottom_lines(screen, LIVE_LINES);

    if live.iter().any(|&line| is_blocked(kind, line))
        || (kind == AgentKind::Codex
            && codex_has_menu_options(
                &live,
                "Keep current model",
                "Keep current model (never show again)",
            ))
    {
        return AgentState::Blocked;
    }
    if live.iter().any(|&line| is_working(kind, line))
        || (kind == AgentKind::Codex
            && codex_has_menu_options(&live, "Dismiss and keep waiting", "Learn more"))
        || (kind == AgentKind::Kimi && kimi_has_active_swarm(&live))
    {
        return AgentState::Working;
    }
    AgentState::Idle
}

fn codex_has_menu_options(lines: &[&str], first: &str, second: &str) -> bool {
    let mut options = lines.iter().copied().filter_map(codex_menu_option);
    options.any(|option| option == first) && options.any(|option| option == second)
}

fn codex_menu_option(line: &str) -> Option<&str> {
    let head = line.trim();
    let head = head.strip_prefix("\u{203a} ").unwrap_or(head);
    let (number, text) = head.split_once(". ")?;
    if number.is_empty() || !number.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    Some(text.split_once("  ").map_or(text, |(label, _)| label))
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
        AgentKind::Pi => pi_is_working(line),
        AgentKind::OpenCode => opencode_is_working(line),
        AgentKind::Qwen => qwen_is_working(line),
        AgentKind::Grok => grok_is_working(line),
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
        AgentKind::Pi => pi_is_blocked(line),
        AgentKind::OpenCode => opencode_is_blocked(line),
        AgentKind::Qwen => qwen_is_blocked(line),
        AgentKind::Grok => grok_is_blocked(line),
    }
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
        assert_eq!(AgentKind::from_name("pi"), Some(AgentKind::Pi));
        assert_eq!(AgentKind::from_name("opencode"), Some(AgentKind::OpenCode));
        assert_eq!(AgentKind::from_name("qwen"), Some(AgentKind::Qwen));
        assert_eq!(AgentKind::from_name("grok"), Some(AgentKind::Grok));
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
        assert_eq!(
            AgentKind::from_path("/home/me/.opencode/bin/opencode"),
            Some(AgentKind::OpenCode)
        );
        assert_eq!(
            AgentKind::from_path("/home/me/.local/bin/qwen"),
            Some(AgentKind::Qwen)
        );
        assert_eq!(
            AgentKind::from_path("/home/me/.local/bin/grok"),
            Some(AgentKind::Grok)
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
    fn from_path_matches_qwen_node_launchers() {
        for path in [
            "/home/me/.local/lib/qwen-code/node/bin/node",
            "/home/me/.local/lib/qwen-code/lib/cli-entry.js",
            "/usr/lib/node_modules/@qwen-code/qwen-code/lib/cli.js",
            "/usr/lib/node_modules/@qwen-code/qwen-code/cli-entry.js",
            "/usr/lib/node_modules/@qwen-code/qwen-code/cli.js",
        ] {
            assert_eq!(AgentKind::from_path(path), Some(AgentKind::Qwen), "{path}");
        }
    }

    #[test]
    fn from_path_rejects_version_binaries_outside_agent_directories_and_vice_versa() {
        assert_eq!(AgentKind::from_path("/opt/foo/versions/2.1.173"), None);
        assert_eq!(
            AgentKind::from_path("/Users/me/projects/claude/target/debug/mytool"),
            None
        );
        assert_eq!(AgentKind::from_path("/opt/claude/versions/2.1.173b"), None);
        assert_eq!(
            AgentKind::from_path("/Users/me/projects/qwen-code/test.js"),
            None
        );
        assert_eq!(
            AgentKind::from_path("/usr/lib/node_modules/@qwen-code/qwen-code/scripts/test.js"),
            None
        );
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

    // Preserve upstream wrapping and column spacing so fixtures catch label/description overlap.
    // https://github.com/openai/codex/tree/b1a547b1f73ce86205d9222ac19cff334b3b7a2e/codex-rs/tui/src/chatwidget/snapshots
    const CODEX_WAITING_WITH_RETRY: &str = concat!(
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
    const CODEX_WAITING_WITHOUT_RETRY: &str = concat!(
        "  Our systems are thinking a bit more about this request before responding.\n",
        "\n",
        "\u{203a} 1. Dismiss and keep waiting\n",
        "  2. Learn more\n",
        "\n",
        "  No action is required. Codex will keep waiting, and this menu will close when\n",
        "  the response is ready.\n",
    );
    const CODEX_RATE_LIMIT_SWITCH: &str = concat!(
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
    fn pi_multiline_editor_prompt_wins_over_spinner() {
        let screen = "\u{280b} Working...\nEdit response\n> draft\nenter submit  shift+enter newline  esc cancel  ctrl+g external editor\n";
        assert_eq!(match_state(AgentKind::Pi, screen), AgentState::Blocked);
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
    fn grok_idle_edit_prompt_is_idle() {
        assert_eq!(
            match_state(
                AgentKind::Grok,
                "\u{25c6} agent idle - waiting on your edit\n"
            ),
            AgentState::Idle
        );
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
        assert_eq!(match_state(AgentKind::Pi, ""), AgentState::Idle);
        assert_eq!(match_state(AgentKind::OpenCode, ""), AgentState::Idle);
        assert_eq!(match_state(AgentKind::Qwen, ""), AgentState::Idle);
        assert_eq!(match_state(AgentKind::Grok, ""), AgentState::Idle);
    }
}
