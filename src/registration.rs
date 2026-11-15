use std::convert::Infallible;
use std::io::Read;
use std::iter::Peekable;
use std::os::unix::process::CommandExt;
use std::process::Command;
use std::str::Chars;

use crate::agent::AgentKind;
use crate::tmux::Result;

const MAX_HOOK_BYTES: u64 = 1024 * 1024;

#[derive(Debug, PartialEq, Eq)]
pub struct Registration {
    agent: AgentKind,
    session_id: String,
}

pub fn agent_kind(name: &str) -> Result<AgentKind> {
    match name {
        "claude" => Ok(AgentKind::Claude),
        "codex" => Ok(AgentKind::Codex),
        _ => Err("registration supports only claude and codex".into()),
    }
}

impl Registration {
    pub fn new(agent: AgentKind, session_id: String) -> Result<Self> {
        agent_kind(agent.label())?;
        if session_id.len() != 36
            || !session_id.bytes().enumerate().all(|(index, byte)| {
                if matches!(index, 8 | 13 | 18 | 23) {
                    byte == b'-'
                } else {
                    byte.is_ascii_hexdigit()
                }
            })
        {
            return Err("session ID must be a complete UUID".into());
        }
        Ok(Self { agent, session_id })
    }

    pub fn from_hook(agent: AgentKind, input: impl Read) -> Result<Self> {
        let mut text = String::new();
        input.take(MAX_HOOK_BYTES + 1).read_to_string(&mut text)?;
        if text.len() as u64 > MAX_HOOK_BYTES {
            return Err("hook input exceeds 1 MiB".into());
        }
        let session_id = HookJson {
            chars: text.chars().peekable(),
        }
        .session_id()?;
        Self::new(agent, session_id)
    }

    pub fn from_value(value: &str) -> Result<Self> {
        let (agent, session_id) = value
            .split_once(':')
            .ok_or("pane has no valid tmux-select registration")?;
        Self::new(agent_kind(agent)?, session_id.to_owned())
    }

    pub fn value(&self) -> String {
        format!("{}:{}", self.agent.label(), self.session_id)
    }

    pub fn resume(&self) -> Result<Infallible> {
        let option = match self.agent {
            AgentKind::Claude => "--resume",
            AgentKind::Codex => "resume",
            _ => return Err("registration supports only claude and codex".into()),
        };
        let error = Command::new(self.agent.label())
            .args([option, &self.session_id])
            .exec();
        Err(format!("failed to start {}: {error}", self.agent.label()).into())
    }
}

struct HookJson<'a> {
    chars: Peekable<Chars<'a>>,
}

impl HookJson<'_> {
    fn session_id(mut self) -> Result<String> {
        self.expect('{')?;
        let mut session_id = None;
        if !self.consume('}') {
            loop {
                let key = self.string()?;
                self.expect(':')?;
                if key == "session_id" {
                    if session_id.is_some() {
                        return Err("hook input contains duplicate session_id fields".into());
                    }
                    session_id = Some(self.string()?);
                } else {
                    self.skip_value(0)?;
                }
                if self.consume('}') {
                    break;
                }
                self.expect(',')?;
            }
        }
        self.whitespace();
        if self.chars.next().is_some() {
            return Err("unexpected data after hook JSON object".into());
        }
        session_id.ok_or_else(|| "hook input is missing session_id".into())
    }

    fn skip_value(&mut self, depth: u8) -> Result<()> {
        if depth == 64 {
            return Err("hook JSON nesting exceeds 64 levels".into());
        }
        self.whitespace();
        match self.chars.peek().copied() {
            Some('"') => {
                self.string()?;
            }
            Some('{') => {
                self.expect('{')?;
                if !self.consume('}') {
                    loop {
                        self.string()?;
                        self.expect(':')?;
                        self.skip_value(depth + 1)?;
                        if self.consume('}') {
                            break;
                        }
                        self.expect(',')?;
                    }
                }
            }
            Some('[') => {
                self.expect('[')?;
                if !self.consume(']') {
                    loop {
                        self.skip_value(depth + 1)?;
                        if self.consume(']') {
                            break;
                        }
                        self.expect(',')?;
                    }
                }
            }
            Some('t') => self.literal("true")?,
            Some('f') => self.literal("false")?,
            Some('n') => self.literal("null")?,
            Some('-' | '0'..='9') => self.number()?,
            _ => return Err("invalid value in hook JSON".into()),
        }
        Ok(())
    }

    fn string(&mut self) -> Result<String> {
        self.expect('"')?;
        let mut value = String::new();
        while let Some(character) = self.chars.next() {
            match character {
                '"' => return Ok(value),
                '\\' => {
                    let escaped = match self.chars.next() {
                        Some('"') => '"',
                        Some('\\') => '\\',
                        Some('/') => '/',
                        Some('b') => '\u{8}',
                        Some('f') => '\u{c}',
                        Some('n') => '\n',
                        Some('r') => '\r',
                        Some('t') => '\t',
                        Some('u') => self.unicode()?,
                        _ => return Err("invalid escape in hook JSON string".into()),
                    };
                    value.push(escaped);
                }
                character if character <= '\u{1f}' => {
                    return Err("unescaped control character in hook JSON string".into());
                }
                character => value.push(character),
            }
        }
        Err("unterminated hook JSON string".into())
    }

    fn unicode(&mut self) -> Result<char> {
        let first = self.hex_quad()?;
        let code = if (0xd800..=0xdbff).contains(&first) {
            if self.chars.next() != Some('\\') || self.chars.next() != Some('u') {
                return Err("missing low surrogate in hook JSON string".into());
            }
            let second = self.hex_quad()?;
            if !(0xdc00..=0xdfff).contains(&second) {
                return Err("invalid low surrogate in hook JSON string".into());
            }
            0x10000 + ((first - 0xd800) << 10) + second - 0xdc00
        } else {
            first
        };
        char::from_u32(code).ok_or_else(|| "invalid Unicode escape in hook JSON string".into())
    }

    fn hex_quad(&mut self) -> Result<u32> {
        let mut value = 0;
        for _ in 0..4 {
            let digit = self
                .chars
                .next()
                .and_then(|character| character.to_digit(16))
                .ok_or("invalid Unicode escape in hook JSON string")?;
            value = value * 16 + digit;
        }
        Ok(value)
    }

    fn number(&mut self) -> Result<()> {
        self.chars.next_if_eq(&'-');
        match self.chars.next() {
            Some('0') => {}
            Some('1'..='9') => {
                self.digits();
            }
            _ => return Err("invalid number in hook JSON".into()),
        }
        if self.chars.next_if_eq(&'.').is_some() && !self.digits() {
            return Err("invalid fraction in hook JSON".into());
        }
        if self.chars.next_if(|c| matches!(c, 'e' | 'E')).is_some() {
            self.chars.next_if(|c| matches!(c, '+' | '-'));
            if !self.digits() {
                return Err("invalid exponent in hook JSON".into());
            }
        }
        Ok(())
    }

    fn digits(&mut self) -> bool {
        let mut found = false;
        while self.chars.next_if(char::is_ascii_digit).is_some() {
            found = true;
        }
        found
    }

    fn literal(&mut self, value: &str) -> Result<()> {
        for expected in value.chars() {
            if self.chars.next() != Some(expected) {
                return Err("invalid literal in hook JSON".into());
            }
        }
        Ok(())
    }

    fn whitespace(&mut self) {
        while self
            .chars
            .next_if(|c| matches!(c, ' ' | '\t' | '\r' | '\n'))
            .is_some()
        {}
    }

    fn consume(&mut self, expected: char) -> bool {
        self.whitespace();
        self.chars.next_if_eq(&expected).is_some()
    }

    fn expect(&mut self, expected: char) -> Result<()> {
        if !self.consume(expected) {
            return Err(format!("expected {expected:?} in hook JSON").into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SESSION: &str = "019aabbc-1234-7000-8000-123456789abc";

    #[test]
    fn registration_round_trips_exact_ids_for_supported_agents() {
        for agent in [AgentKind::Claude, AgentKind::Codex] {
            let registration = Registration::new(agent, SESSION.into()).unwrap();
            assert_eq!(
                Registration::from_value(&registration.value()).unwrap(),
                registration
            );
        }
        assert_eq!(
            Registration::new(AgentKind::Codex, SESSION.into())
                .unwrap()
                .value(),
            format!("codex:{SESSION}")
        );
        for value in [
            "",
            "codex",
            "pi:019aabbc-1234-7000-8000-123456789abc",
            "codex:",
            "codex:--last",
            "codex:019aabbc-1234-7000-8000-123456789abc:extra",
            "tmux-select:v1:codex:019aabbc-1234-7000-8000-123456789abc",
        ] {
            assert!(Registration::from_value(value).is_err(), "{value}");
        }
    }

    #[test]
    fn hook_input_selects_only_the_top_level_session_id() {
        let input = format!(
            r#"{{"cwd":"/tmp/a\\b","nested":{{"session_id":"wrong"}},"values":[true,false,null,-1.25e+2,0,{{}}],"session_id":"{SESSION}","hook_event_name":"SessionStart"}}"#
        );
        let registration = Registration::from_hook(AgentKind::Codex, input.as_bytes()).unwrap();
        assert_eq!(registration.session_id, SESSION);
        let escaped = r#"{"session_\u0069d":"\u003019aabbc-1234-7000-8000-123456789abc","other":"\uD83D\uDE00"}"#;
        assert_eq!(
            Registration::from_hook(AgentKind::Claude, escaped.as_bytes())
                .unwrap()
                .session_id,
            SESSION
        );
    }

    #[test]
    fn hook_input_rejects_missing_ambiguous_or_malformed_data() {
        for input in [
            "",
            "[]",
            "{}",
            r#"{"session_id":null}"#,
            r#"{"session_id":12}"#,
            r#"{"session_id":"wrong"}"#,
            r#"{"nested":{"session_id":"wrong"}}"#,
            r#"{"session_id":"019aabbc-1234-7000-8000-123456789abc","session_id":"019aabbc-1234-7000-8000-123456789abc"}"#,
            r#"{"session_id":"019aabbc-1234-7000-8000-123456789abc",}"#,
            r#"{"session_id":"019aabbc-1234-7000-8000-123456789abc"} trailing"#,
            r#"{"other":"\q","session_id":"019aabbc-1234-7000-8000-123456789abc"}"#,
            r#"{"other":"\uD800","session_id":"019aabbc-1234-7000-8000-123456789abc"}"#,
            r#"{"other":01,"session_id":"019aabbc-1234-7000-8000-123456789abc"}"#,
            r#"{"other":1.,"session_id":"019aabbc-1234-7000-8000-123456789abc"}"#,
            r#"{"other":1e,"session_id":"019aabbc-1234-7000-8000-123456789abc"}"#,
        ] {
            assert!(
                Registration::from_hook(AgentKind::Codex, input.as_bytes()).is_err(),
                "{input}"
            );
        }
    }
}
