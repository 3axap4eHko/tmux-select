use std::env;
use std::error::Error;
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

pub type Result<T> = std::result::Result<T, Box<dyn Error + Send + Sync>>;

const SENTINEL: &str = "TMUXSELECT_SENTINEL";
const PANE_FORMAT: &str = "#{pane_id}|#{window_id}|#{window_index}|#{pane_active}|#{pane_index}|#{pane_pid}|#{pane_current_command}|#{pane_current_path}";
const PANE_FIELDS: usize = 8;

pub struct Pane {
    pub pane_id: String,
    pub window_id: String,
    pub window_index: u32,
    pub pane_active: bool,
    pub pane_index: u32,
    pub pane_pid: u32,
    pub current_command: String,
    pub current_path: String,
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

pub fn current_session_id() -> Result<String> {
    let tmux = env::var("TMUX").map_err(|_| "TMUX is not set; tmux-select must run inside tmux")?;
    let number = tmux
        .split(',')
        .nth(2)
        .filter(|field| !field.is_empty() && field.bytes().all(|b| b.is_ascii_digit()))
        .ok_or("TMUX has an unexpected format; cannot resolve the current session id")?;
    Ok(format!("${number}"))
}

impl ControlClient {
    pub fn attach(session_id: &str) -> Result<Self> {
        let mut child = Command::new("tmux")
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
        client.synchronize()?;
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
        let body = self.command(&format!(
            "list-panes -s -t '{session_id}' -F '{PANE_FORMAT}'"
        ))?;
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
        while self.reader.read_line(&mut sink)? != 0 {
            sink.clear();
        }
        self.child.wait()?;
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

fn parse_pane(line: &str) -> Result<Pane> {
    let fields: Vec<&str> = line.splitn(PANE_FIELDS, '|').collect();
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
        pane_active: fields[3] == "1",
        pane_index: fields[4].parse()?,
        pane_pid: fields[5].parse()?,
        current_command: fields[6].to_string(),
        current_path: fields[7].to_string(),
    })
}

pub fn switch_to(window_id: &str, pane_id: Option<&str>) -> Result<()> {
    let mut command = Command::new("tmux");
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

    fn body(block: Block) -> String {
        match block {
            Block::End(text) => text,
            Block::Error(message) => panic!("expected %end block, got error: {message}"),
        }
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
        let pane = parse_pane("%2|@0|12|1|3|2776867|npm|/home/me/a|b").unwrap();
        assert_eq!(pane.pane_id, "%2");
        assert_eq!(pane.window_id, "@0");
        assert_eq!(pane.window_index, 12);
        assert!(pane.pane_active);
        assert_eq!(pane.pane_index, 3);
        assert_eq!(pane.pane_pid, 2776867);
        assert_eq!(pane.current_command, "npm");
        assert_eq!(pane.current_path, "/home/me/a|b");
    }

    #[test]
    fn command_number_reads_the_second_token() {
        assert_eq!(command_number("1780806595 1400 1"), Some(1400));
        assert_eq!(command_number("nonsense"), None);
    }
}
