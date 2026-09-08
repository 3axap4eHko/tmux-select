#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AgentKind {
    Claude,
    Codex,
    Kimi,
    Pi,
    OpenCode,
    Qwen,
    Grok,
    Muse,
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
            "muse" => Some(AgentKind::Muse),
            _ if muse_is_binary_name(name) => Some(AgentKind::Muse),
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
            AgentKind::Muse => "muse",
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
        components
            .find_map(|name| AgentKind::from_name(name).filter(|kind| *kind != AgentKind::Muse))
    }
}

fn muse_is_binary_name(name: &str) -> bool {
    let Some((version, release)) = name
        .strip_prefix("muse-bin-")
        .and_then(|version| version.split_once("-R"))
    else {
        return false;
    };
    let mut components = version.split('.');
    (0..3).all(|_| components.next().is_some_and(is_decimal_component))
        && components.next().is_none()
        && match release.split_once('.') {
            Some((build, revision)) => {
                is_decimal_component(build) && is_decimal_component(revision)
            }
            None => is_decimal_component(release),
        }
}

fn is_decimal_component(text: &str) -> bool {
    !text.is_empty() && text.bytes().all(|byte| byte.is_ascii_digit())
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

#[cfg(test)]
mod tests {
    use super::*;

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
    fn muse_names_and_paths_follow_the_launcher_release_format() {
        for name in ["muse", "muse-bin-1.0.3-R2198.1", "muse-bin-1.2.34-R5678"] {
            assert_eq!(AgentKind::from_name(name), Some(AgentKind::Muse));
            for directory in ["/home/me/.local/bin", "/Users/me/.local/bin"] {
                assert_eq!(
                    AgentKind::from_path(&format!("{directory}/{name}")),
                    Some(AgentKind::Muse)
                );
            }
        }
        assert_eq!(AgentKind::Muse.label(), "muse");
    }

    #[test]
    fn muse_rejects_truncated_and_malformed_executable_names() {
        for name in [
            "muse-bin-1.0.3-",
            "muse-bin-1.0.3-R",
            "muse-bin-1.0.3-R2198.",
            "muse-bin-1.0.3-R2198.1.2",
            "muse-bin-1.0.3-R2198.1-backup",
            "muse-bin-1.0.3-Rx",
            "muse-bin-1.0-R2198",
            "muse-bin-1.0.3.4-R2198",
            "muse-bin-1..3-R2198",
            "muse-bin-a.0.3-R2198",
            "muse-code",
        ] {
            assert_eq!(AgentKind::from_name(name), None, "{name}");
            assert_eq!(AgentKind::from_path(&format!("/opt/bin/{name}")), None);
        }
        assert_eq!(AgentKind::from_path("/opt/muse/versions/1.0.3"), None);
        assert_eq!(AgentKind::from_path("/opt/muse/bin/node"), None);
    }
}
