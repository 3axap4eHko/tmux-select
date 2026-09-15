use std::error::Error;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

pub type Result<T> = std::result::Result<T, Box<dyn Error + Send + Sync>>;

const SENTINEL: &str = "TMUXSELECT_SENTINEL";
const PANE_FORMAT: &str = "#{pane_id}\x1f#{window_id}\x1f#{window_index}\x1f#{window_active}\x1f#{pane_active}\x1f#{pane_index}\x1f#{pane_pid}\x1f#{pane_current_command}\x1f#{?automatic-rename,#{s|\\n| |:pane_current_path},#{s|\\n| |:window_name}}\x1f#{s|\\n| |:window_name}";
// tmux octal-escapes non-printable bytes in control-mode command output.
const CONTROL_MODE_PANE_SEPARATOR: &str = r"\037";
const PANE_FIELDS: usize = 10;

pub struct Pane {
    pub pane_id: String,
    pub window_id: String,
    pub window_index: u32,
    pub window_active: bool,
    pub pane_active: bool,
    pub pane_index: u32,
    pub pane_pid: u32,
    pub current_command: String,
    pub label: String,
    pub window_name: String,
}

enum Block {
    End(String),
    Error(String),
}

pub struct ControlClient {
    child: Child,
    stdin: Option<ChildStdin>,
    reader: BufReader<ChildStdout>,
}

pub fn session_id_from_tmux_value(tmux: &str) -> Result<String> {
    let number = tmux
        .rsplit(',')
        .next()
        .filter(|field| !field.is_empty() && field.bytes().all(|b| b.is_ascii_digit()))
        .ok_or("TMUX has an unexpected format; cannot resolve the current session id")?;
    Ok(format!("${number}"))
}

pub fn rename_window(tmux: &str, pane_id: &str, name: &str) -> Result<()> {
    let socket = rename_socket(tmux, pane_id)?;
    run_rename(&mut rename_command(socket, pane_id, name)).map(|_| ())
}

pub fn rename_selected_window(tmux: &str, window_id: &str, name: &str) -> Result<String> {
    if !window_id.strip_prefix('@').is_some_and(is_decimal) {
        return Err("selected window must have an ID such as @0".into());
    }
    if name.trim().is_empty() || name.contains('\0') {
        return Err("window name must be nonempty and contain no NUL bytes".into());
    }
    let socket = socket_from_tmux_value(tmux)?;
    let mut command = rename_command(socket, window_id, name);
    command.args([
        ";",
        "display-message",
        "-p",
        "-t",
        window_id,
        "#{window_name}",
    ]);
    run_rename(&mut command)
}

fn rename_command(socket: &str, target: &str, name: &str) -> Command {
    let mut command = Command::new("tmux");
    command
        .args(["-S", socket, "rename-window", "-t", target, "--"])
        .arg(literal_window_name(name));
    command
}

fn run_rename(command: &mut Command) -> Result<String> {
    let output = command
        .output()
        .map_err(|error| format!("failed to start tmux: {error}"))?;
    if !output.status.success() {
        let detail = String::from_utf8_lossy(&output.stderr);
        return Err(format!("failed to rename the window: {}", detail.trim()).into());
    }
    let name = String::from_utf8(output.stdout)?;
    Ok(name.trim_end_matches('\n').to_owned())
}

fn rename_socket<'a>(tmux: &'a str, pane_id: &str) -> Result<&'a str> {
    if !pane_id.strip_prefix('%').is_some_and(is_decimal) {
        return Err("TMUX_PANE must be a pane ID such as %0".into());
    }
    socket_from_tmux_value(tmux)
}

pub fn socket_from_tmux_value(tmux: &str) -> Result<&str> {
    let mut fields = tmux.rsplitn(3, ',');
    let session = fields.next();
    let pid = fields.next();
    let socket = fields.next();
    match (session, pid, socket) {
        (Some(session), Some(pid), Some(socket))
            if (is_decimal(session) || session == "-1")
                && is_decimal(pid)
                && !socket.is_empty() =>
        {
            Ok(socket)
        }
        _ => Err("TMUX has an unexpected format; cannot resolve the calling server".into()),
    }
}

fn is_decimal(value: &str) -> bool {
    !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit())
}

fn literal_window_name(name: &str) -> String {
    // rename-window expands formats, and tmux's argv parser splits at a final semicolon.
    let mut escaped = name.replace('#', "##");
    if escaped.ends_with(';') {
        escaped.insert(escaped.len() - 1, '\\');
    }
    escaped
}

impl ControlClient {
    pub fn attach(socket: &Path, session_id: &str) -> Result<Self> {
        if !session_id.strip_prefix('$').is_some_and(is_decimal) {
            return Err("session target must be an ID such as $0".into());
        }
        let mut child = Command::new("tmux")
            .arg("-S")
            .arg(socket)
            .args([
                "-C",
                "attach-session",
                "-t",
                session_id,
                "-f",
                "no-output,ignore-size",
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|error| format!("failed to start the tmux control client: {error}"))?;
        let stdin = child
            .stdin
            .take()
            .ok_or("failed to capture control-client stdin")?;
        let stdout = child
            .stdout
            .take()
            .ok_or("failed to capture control-client stdout")?;
        let mut client = Self {
            child,
            stdin: Some(stdin),
            reader: BufReader::new(stdout),
        };
        if let Err(error) = client.synchronize() {
            if let Err(detach_error) = client.detach() {
                return Err(format!("{error}; failed to detach: {detach_error}").into());
            }
            return Err(error);
        }
        Ok(client)
    }

    fn synchronize(&mut self) -> Result<()> {
        self.send(&format!("display-message -p '{SENTINEL}'"))?;
        loop {
            if let Block::End(body) = read_block(&mut self.reader)?
                && body == SENTINEL
            {
                return Ok(());
            }
        }
    }

    fn send(&mut self, command: &str) -> Result<()> {
        let stdin = self
            .stdin
            .as_mut()
            .ok_or("control client already detached")?;
        stdin.write_all(command.as_bytes())?;
        stdin.write_all(b"\n")?;
        stdin.flush()?;
        Ok(())
    }

    fn command(&mut self, command: &str) -> Result<String> {
        self.send(command)?;
        match read_block(&mut self.reader)? {
            Block::End(body) => Ok(body),
            Block::Error(message) => Err(format!("tmux command failed: {message}").into()),
        }
    }

    pub fn enumerate(&mut self, session_id: &str) -> Result<Vec<Pane>> {
        let body = self.command(&list_panes_command(session_id))?;
        body.lines().map(parse_pane).collect()
    }

    pub fn capture(&mut self, pane_id: &str) -> Result<String> {
        let mut body = self.command(&format!("capture-pane -p -t '{pane_id}'"))?;
        body.truncate(body.trim_end().len());
        Ok(body)
    }

    pub fn detach(mut self) -> Result<()> {
        drop(self.stdin.take());
        let mut sink = String::new();
        let drained = loop {
            match self.reader.read_line(&mut sink) {
                Ok(0) => break Ok(()),
                Ok(_) => sink.clear(),
                Err(error) => break Err(error),
            }
        };
        let waited = self.child.wait();
        drained?;
        if !waited?.success() {
            return Err("the tmux control client exited unsuccessfully".into());
        }
        Ok(())
    }
}

fn read_block(reader: &mut impl BufRead) -> Result<Block> {
    let mut line = String::new();
    let number = loop {
        read_line_into(reader, &mut line)?;
        if let Some(rest) = line.strip_prefix("%begin ") {
            break command_number(rest).ok_or("malformed %begin line from tmux")?;
        }
        if line == "%exit" || line.starts_with("%exit ") {
            return Err("the tmux control client exited unexpectedly".into());
        }
    };
    let mut body = String::new();
    let mut first = true;
    loop {
        read_line_into(reader, &mut line)?;
        if let Some(rest) = line.strip_prefix("%end ")
            && command_number(rest) == Some(number)
        {
            return Ok(Block::End(body));
        }
        if let Some(rest) = line.strip_prefix("%error ")
            && command_number(rest) == Some(number)
        {
            return Ok(Block::Error(body));
        }
        if !first {
            body.push('\n');
        }
        body.push_str(&line);
        first = false;
    }
}

fn read_line_into(reader: &mut impl BufRead, line: &mut String) -> Result<()> {
    line.clear();
    if reader.read_line(line)? == 0 {
        return Err("the tmux control client closed its output stream".into());
    }
    let trimmed = line.trim_end_matches(['\n', '\r']).len();
    line.truncate(trimmed);
    Ok(())
}

fn command_number(rest: &str) -> Option<u64> {
    rest.split_whitespace().nth(1)?.parse().ok()
}

fn list_panes_command(session_id: &str) -> String {
    format!("list-panes -s -t '{session_id}' -F \"{PANE_FORMAT}\"")
}

fn parse_pane(line: &str) -> Result<Pane> {
    let fields: Vec<&str> = line
        .splitn(PANE_FIELDS, CONTROL_MODE_PANE_SEPARATOR)
        .collect();
    if fields.len() != PANE_FIELDS {
        return Err(format!(
            "list-panes line has {} fields, expected {PANE_FIELDS}: {line:?}",
            fields.len()
        )
        .into());
    }
    Ok(Pane {
        pane_id: fields[0].to_string(),
        window_id: fields[1].to_string(),
        window_index: fields[2].parse()?,
        window_active: fields[3] == "1",
        pane_active: fields[4] == "1",
        pane_index: fields[5].parse()?,
        pane_pid: fields[6].parse()?,
        current_command: fields[7].to_string(),
        label: fields[8].to_string(),
        window_name: fields[9].to_string(),
    })
}

pub fn switch_to(socket: &Path, window_id: &str, pane_id: Option<&str>) -> Result<()> {
    let mut command = Command::new("tmux");
    command.arg("-S").arg(socket);
    command.args(["select-window", "-t", window_id]);
    if let Some(pane) = pane_id {
        command.arg(";").args(["select-pane", "-t", pane]);
    }
    if !command.status()?.success() {
        return Err("tmux failed to switch to the selected window".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn rename_context_uses_the_calling_server_even_with_commas_in_its_path() {
        assert_eq!(
            rename_socket("/tmp/agents/a,b/socket,123,4", "%12").unwrap(),
            "/tmp/agents/a,b/socket"
        );
    }

    #[test]
    fn job_socket_is_independent_of_a_session_id() {
        assert_eq!(
            socket_from_tmux_value("/tmp/agents/a,b/socket,123,-1").unwrap(),
            "/tmp/agents/a,b/socket"
        );
        for value in ["socket,123,-2", "socket,123,", "socket,,0", ",123,-1"] {
            assert!(socket_from_tmux_value(value).is_err());
        }
        assert!(session_id_from_tmux_value("socket,123,-1").is_err());
    }

    #[test]
    fn rename_context_rejects_ambiguous_targets_and_malformed_servers() {
        for pane in ["", "%", "0", "@0", "name", "%1;", "%1:0", "% 1"] {
            assert!(rename_socket("/tmp/agents/socket,123,0", pane).is_err());
        }
        for tmux in [
            "",
            "0",
            "socket,0",
            ",123,0",
            "socket,,0",
            "socket,abc,0",
            "socket,123,x",
        ] {
            assert!(rename_socket(tmux, "%0").is_err());
        }
    }

    fn body(block: Block) -> String {
        match block {
            Block::End(text) => text,
            Block::Error(message) => panic!("expected %end block, got error: {message}"),
        }
    }

    #[test]
    fn session_id_from_tmux_value_reads_the_final_field() {
        assert_eq!(
            session_id_from_tmux_value("/tmp/tmux-1000/default,12345,0").unwrap(),
            "$0"
        );
    }

    #[test]
    fn session_id_from_tmux_value_allows_commas_in_the_socket_path() {
        assert_eq!(
            session_id_from_tmux_value("/tmp/agents/tmux-select,socket,67706,0").unwrap(),
            "$0"
        );
    }

    #[test]
    fn session_id_from_tmux_value_rejects_a_non_numeric_final_field() {
        assert!(session_id_from_tmux_value("/tmp/tmux-1000/default,12345,").is_err());
        assert!(session_id_from_tmux_value("/tmp/tmux-1000/default,12345,current").is_err());
    }

    #[test]
    fn list_panes_command_selects_labels_and_replaces_newlines_before_line_parsing() {
        assert!(PANE_FORMAT.contains(r"#{s|\n| |:pane_current_path}"));
        assert!(PANE_FORMAT.contains(r"#{s|\n| |:window_name}"));
        assert_eq!(
            list_panes_command("$0"),
            "list-panes -s -t '$0' -F \"#{pane_id}\u{1f}#{window_id}\u{1f}#{window_index}\u{1f}#{window_active}\u{1f}#{pane_active}\u{1f}#{pane_index}\u{1f}#{pane_pid}\u{1f}#{pane_current_command}\u{1f}#{?automatic-rename,#{s|\\n| |:pane_current_path},#{s|\\n| |:window_name}}\u{1f}#{s|\\n| |:window_name}\""
        );
    }

    #[test]
    fn parses_a_pane_line_after_newline_path_sanitization() {
        let pane =
            parse_pane(r"%2\037@0\03712\0371\0371\0373\0372776867\037npm\037/tmp/a b\037shell")
                .unwrap();
        assert_eq!(pane.label, "/tmp/a b");
        assert_eq!(pane.window_name, "shell");
    }

    #[test]
    fn parses_a_fixed_window_name_as_the_label() {
        let pane = parse_pane(
            r"%2\037@0\03712\0371\0371\0373\0372776867\037npm\037Fix issue #123 | allocator\037Fix issue #123 | allocator",
        )
        .unwrap();
        assert_eq!(pane.label, "Fix issue #123 | allocator");
    }

    #[test]
    fn reads_blocks_and_skips_interleaved_notifications() {
        let stream = "\
%begin 1780806595 1399 1
TMUXSELECT_SENTINEL
%end 1780806595 1399 1
%session-changed $0 main
%begin 1780806595 1400 1
%0|@0|1|0|1|bash|/tmp/agents
%2|@0|1|1|2|node|/home/me
%end 1780806595 1400 1
";
        let mut reader = Cursor::new(stream);
        assert_eq!(
            body(read_block(&mut reader).unwrap()),
            "TMUXSELECT_SENTINEL"
        );
        assert_eq!(
            body(read_block(&mut reader).unwrap()),
            "%0|@0|1|0|1|bash|/tmp/agents\n%2|@0|1|1|2|node|/home/me",
        );
    }

    #[test]
    fn error_block_carries_the_message() {
        let stream = "%begin 1 1404 1\ncan't find pane: %999\n%error 1 1404 1\n";
        let mut reader = Cursor::new(stream);
        match read_block(&mut reader).unwrap() {
            Block::Error(message) => assert_eq!(message, "can't find pane: %999"),
            Block::End(_) => panic!("expected an error block"),
        }
    }

    #[test]
    fn body_line_that_looks_like_a_terminator_with_a_different_number_is_kept() {
        let stream = "%begin 1 5 1\n%end 1 4 1\nreal content\n%end 1 5 1\n";
        let mut reader = Cursor::new(stream);
        assert_eq!(
            body(read_block(&mut reader).unwrap()),
            "%end 1 4 1\nreal content"
        );
    }

    #[test]
    fn parses_a_pane_line_with_a_pipe_in_the_path() {
        let pane =
            parse_pane(r"%2\037@0\03712\0371\0371\0373\0372776867\037npm\037/home/me/a|b\037shell")
                .unwrap();
        assert_eq!(pane.pane_id, "%2");
        assert_eq!(pane.window_id, "@0");
        assert_eq!(pane.window_index, 12);
        assert!(pane.window_active);
        assert!(pane.pane_active);
        assert_eq!(pane.pane_index, 3);
        assert_eq!(pane.pane_pid, 2776867);
        assert_eq!(pane.current_command, "npm");
        assert_eq!(pane.label, "/home/me/a|b");
    }

    #[test]
    fn parses_a_pane_line_with_a_pipe_in_the_command() {
        let pane =
            parse_pane(r"%2\037@0\03712\0370\0371\0373\0372776867\037we|ird\037/tmp/x\037shell")
                .unwrap();
        assert!(!pane.window_active);
        assert_eq!(pane.current_command, "we|ird");
        assert_eq!(pane.label, "/tmp/x");
    }

    #[test]
    fn command_number_reads_the_second_token() {
        assert_eq!(command_number("1780806595 1400 1"), Some(1400));
        assert_eq!(command_number("nonsense"), None);
    }
}
