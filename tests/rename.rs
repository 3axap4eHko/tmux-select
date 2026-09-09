use std::path::PathBuf;
use std::process::{Command, Output};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

fn cli(args: &[&str], tmux: Option<&str>, pane: Option<&str>) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_tmux-select"));
    command
        .args(args)
        .env_remove("TMUX")
        .env_remove("TMUX_PANE");
    if let Some(value) = tmux {
        command.env("TMUX", value);
    }
    if let Some(value) = pane {
        command.env("TMUX_PANE", value);
    }
    command.output().expect("run tmux-select")
}

fn assert_error(output: Output, expected: &str) {
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains(expected), "{stderr:?} lacks {expected:?}");
}

#[test]
fn cli_rejects_invalid_arguments_and_missing_context_without_a_terminal() {
    for args in [
        vec!["unknown"],
        vec!["rename"],
        vec!["rename", "one", "two"],
    ] {
        assert_error(cli(&args, None, None), "usage:");
    }
    assert_error(cli(&["rename", ""], None, None), "nonempty");
    assert_error(
        cli(&["rename", "ISSUE-123"], None, Some("%0")),
        "TMUX is not set",
    );
    assert_error(
        cli(
            &["rename", "ISSUE-123"],
            Some("/tmp/agents/unused,123,0"),
            None,
        ),
        "TMUX_PANE is not set",
    );
    assert_error(
        cli(
            &["rename", "ISSUE-123"],
            Some("/tmp/agents/unused,123,0"),
            Some("@0"),
        ),
        "TMUX_PANE must be a pane ID",
    );
    assert_error(
        cli(&["rename", "ISSUE-123"], Some("invalid"), Some("%0")),
        "cannot resolve the calling server",
    );
}

struct Server {
    directory: PathBuf,
    socket: PathBuf,
}

fn start_server(socket_name: &str) -> Server {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let directory = PathBuf::from(format!("/tmp/agents/rename-{}-{nonce}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let server = Server {
        socket: directory.join(socket_name),
        directory,
    };
    server.run(&[
        "-f",
        "/dev/null",
        "new-session",
        "-d",
        "-s",
        "caller",
        "-n",
        "original",
        "sh",
    ]);
    server
}

impl Server {
    fn run(&self, args: &[&str]) -> String {
        let output = Command::new("tmux")
            .arg("-S")
            .arg(&self.socket)
            .args(args)
            .env_remove("TMUX")
            .env_remove("TMUX_PANE")
            .output()
            .expect("run isolated tmux (tmux must be installed)");
        assert!(
            output.status.success(),
            "tmux {args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout)
            .unwrap()
            .trim_end_matches('\n')
            .to_owned()
    }

    fn context(&self) -> String {
        format!(
            "{},{},0",
            self.socket.display(),
            self.run(&["display-message", "-p", "#{pid}"])
        )
    }

    fn read(&self, pane: &str, format: &str) -> String {
        self.run(&["display-message", "-p", "-t", pane, format])
    }

    fn type_text(&self, pane: &str, text: &str) {
        let escaped = if let Some(prefix) = text.strip_suffix(';') {
            format!("{prefix}\\;")
        } else {
            text.to_owned()
        };
        self.run(&["send-keys", "-t", pane, "-l", "--", &escaped]);
    }

    fn wait_for(&self, pane: &str, expected: &str) {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let screen = self.run(&["capture-pane", "-p", "-t", pane]);
            if screen.contains(expected) {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "screen lacks {expected:?}: {screen}"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        match Command::new("tmux")
            .arg("-S")
            .arg(&self.socket)
            .arg("kill-server")
            .output()
        {
            Ok(output) if output.status.success() => {}
            Ok(output) => eprintln!(
                "test server cleanup: {}",
                String::from_utf8_lossy(&output.stderr)
            ),
            Err(error) => eprintln!("test server cleanup: {error}"),
        }
        if let Err(error) = std::fs::remove_dir_all(&self.directory) {
            eprintln!("test directory cleanup: {error}");
        }
    }
}

#[test]
#[ignore = "requires tmux; run with cargo test --test rename -- --include-ignored"]
fn rename_targets_the_calling_window_and_preserves_literal_names() {
    let server = start_server("tmux,socket");
    let other_server = start_server("tmux,socket");
    let context = server.context();
    let original = server.read("caller:", "#{pane_id}");
    let caller = server.run(&[
        "split-window",
        "-d",
        "-P",
        "-F",
        "#{pane_id}",
        "-t",
        &original,
        "sh",
    ]);
    let other = server.run(&[
        "new-window",
        "-P",
        "-F",
        "#{pane_id}",
        "-t",
        "caller:",
        "-n",
        "other",
        "sh",
    ]);
    let separate = server.run(&[
        "new-session",
        "-d",
        "-P",
        "-F",
        "#{pane_id}",
        "-s",
        "project",
        "-n",
        "separate",
        "sh",
    ]);
    server.run(&["set-option", "-w", "-t", &caller, "automatic-rename", "on"]);

    for name in [
        "ISSUE-123 fix login",
        "-leading",
        "#{window_name}",
        "#(echo injected)",
        "$(echo shell)",
        "a; b",
        ";",
        "trailing;",
        "quote'\"",
        "  padded  ",
    ] {
        let output = cli(&["rename", name], Some(&context), Some(&caller));
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(output.stdout.is_empty());
        assert!(output.stderr.is_empty());
        assert_eq!(server.read(&caller, "#{window_name}"), name);
        assert_eq!(server.read(&caller, "#{automatic-rename}"), "0");
        assert_eq!(server.read(&caller, "#{window_active}"), "0");
        assert_eq!(server.read(&original, "#{pane_active}"), "1");
        assert_eq!(
            server.read(&other, "#{window_name}|#{window_active}"),
            "other|1"
        );
        assert_eq!(server.read(&separate, "#{window_name}"), "separate");
        assert_eq!(other_server.read("caller:", "#{window_name}"), "original");
    }
    assert_error(
        cli(&["rename", "wrong"], Some(&context), Some("%999999")),
        "failed to rename the window",
    );
    let missing_server = format!("{},123,0", server.directory.join("missing").display());
    assert_error(
        cli(&["rename", "wrong"], Some(&missing_server), Some(&caller)),
        "failed to rename the window",
    );
    assert_eq!(server.read(&caller, "#{window_name}"), "  padded  ");
    assert_eq!(server.read(&other, "#{window_name}"), "other");
}

#[test]
#[ignore = "requires tmux; run with cargo test --test rename -- --include-ignored"]
fn ctrl_r_renames_the_selected_window_and_updates_the_open_picker() {
    let server = start_server("tmux.socket");
    let target = server.read("caller:", "#{pane_id}");
    server.run(&[
        "set-option",
        "-w",
        "-t",
        &target,
        "automatic-rename-format",
        "original",
    ]);
    server.run(&["set-option", "-w", "-t", &target, "automatic-rename", "on"]);
    let path = server.read(&target, "#{pane_current_path}");
    let driver = server.run(&["new-window", "-P", "-F", "#{pane_id}", "-n", "picker", "sh"]);
    let binary = env!("CARGO_BIN_EXE_tmux-select").replace('\'', "'\\''");
    server.type_text(
        &driver,
        &format!("'{binary}'; printf 'picker-done:%s\\n' \"$?\""),
    );
    server.run(&["send-keys", "-t", &driver, "Enter"]);
    server.wait_for(&driver, "Ctrl-R: rename");
    server.type_text(&driver, &path);
    server.run(&["send-keys", "-t", &driver, "C-r"]);
    server.wait_for(&driver, "Rename> original");
    server.run(&["send-keys", "-t", &driver, "C-u"]);
    server.type_text(&driver, "cancelled");
    server.run(&["send-keys", "-t", &driver, "Escape"]);
    server.wait_for(&driver, "Ctrl-R: rename");
    assert_eq!(server.read(&target, "#{window_name}"), "original");

    server.run(&["send-keys", "-t", &driver, "C-r", "C-u", "Enter"]);
    server.wait_for(&driver, "window name must be nonempty");
    let name = "ISSUE-123 #{window_name};";
    server.type_text(&driver, name);
    server.run(&["send-keys", "-t", &driver, "Enter"]);
    server.wait_for(&driver, "0/2");
    assert_eq!(
        server.read(
            &target,
            "#{window_name}|#{automatic-rename}|#{window_active}"
        ),
        format!("{name}|0|0")
    );
    assert_eq!(server.read(&driver, "#{window_active}"), "1");
    server.run(&["send-keys", "-t", &driver, "C-u"]);
    server.type_text(&driver, "ISSUE");
    server.wait_for(&driver, name);
    server.run(&["send-keys", "-t", &driver, "Enter"]);
    server.wait_for(&driver, "picker-done:0");
    assert_eq!(server.read(&target, "#{window_active}"), "1");

    server.type_text(
        &driver,
        &format!("'{binary}'; printf 'picker-failed-save:%s\\n' \"$?\""),
    );
    server.run(&["send-keys", "-t", &driver, "Enter"]);
    server.wait_for(&driver, "Ctrl-R: rename");
    server.type_text(&driver, "ISSUE");
    server.run(&["send-keys", "-t", &driver, "C-r"]);
    server.wait_for(&driver, "Rename>");
    server.run(&["kill-window", "-t", &target]);
    server.run(&["send-keys", "-t", &driver, "Enter"]);
    server.wait_for(&driver, "failed to rename the window");
    server.run(&["send-keys", "-t", &driver, "Escape"]);
    server.wait_for(&driver, "Ctrl-R: rename");
    server.run(&["send-keys", "-t", &driver, "Escape"]);
    server.wait_for(&driver, "picker-failed-save:0");
}

#[test]
#[ignore = "requires tmux; run with cargo test --test rename -- --include-ignored"]
fn ctrl_r_preserves_backslashes_across_saves_and_edits() {
    let server = start_server("tmux.socket");
    let target = server.read("caller:", "#{pane_id}");
    let name = r"roundtrip a\b \n #{window_name};";
    let output = cli(&["rename", name], Some(&server.context()), Some(&target));
    assert!(output.status.success());
    let stored = server.read(&target, "#{window_name}");
    let driver = server.run(&["new-window", "-P", "-F", "#{pane_id}", "-n", "picker", "sh"]);
    let binary = env!("CARGO_BIN_EXE_tmux-select").replace('\'', "'\\''");
    server.type_text(&driver, &format!("'{binary}'"));
    server.run(&["send-keys", "-t", &driver, "Enter"]);
    server.wait_for(&driver, "Ctrl-R: rename");
    server.type_text(&driver, "roundtrip");

    for suffix in ["", " edited", ""] {
        let before = server.read(&target, "#{window_name}");
        server.run(&["send-keys", "-t", &driver, "C-r"]);
        server.wait_for(&driver, "Rename>");
        if !suffix.is_empty() {
            server.type_text(&driver, suffix);
        }
        server.run(&["send-keys", "-t", &driver, "Enter"]);
        server.wait_for(&driver, "Ctrl-R: rename");
        assert_eq!(
            server.read(&target, "#{window_name}"),
            format!("{before}{suffix}")
        );
        assert_eq!(server.read(&driver, "#{window_active}"), "1");
    }
    assert_eq!(
        server.read(&target, "#{window_name}"),
        format!("{stored} edited")
    );
    server.run(&["send-keys", "-t", &driver, "Escape"]);
}
