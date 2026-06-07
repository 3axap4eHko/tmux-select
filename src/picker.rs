use std::io::{self, Write};

use termion::event::Key;
use termion::input::TermRead;
use termion::raw::IntoRawMode;
use termion::screen::IntoAlternateScreen;
use termion::{clear, color, cursor};

use crate::tmux::Result;

pub struct Candidate {
    pub display: String,
    pub window_id: String,
    pub pane_id: Option<String>,
}

pub struct Target {
    pub window_id: String,
    pub pane_id: Option<String>,
}

pub fn pick(candidates: Vec<Candidate>) -> Result<Option<Target>> {
    if candidates.is_empty() {
        return Ok(None);
    }
    let raw = io::stdout()
        .into_raw_mode()
        .map_err(|error| format!("failed to enter terminal raw mode (not a tty?): {error}"))?;
    let mut screen = raw.into_alternate_screen()?;
    run(&candidates, &mut screen)
}

fn run<W: Write>(candidates: &[Candidate], screen: &mut W) -> Result<Option<Target>> {
    let mut query = String::new();
    let mut filtered = rank(candidates, &query);
    let mut selected = 0usize;
    let mut keys = io::stdin().keys();

    loop {
        render(screen, candidates, &filtered, &query, selected)?;
        let Some(key) = keys.next() else {
            return Ok(None);
        };
        match key? {
            Key::Char('\n') | Key::Char('\r') => {
                return Ok(filtered.get(selected).map(|&index| Target {
                    window_id: candidates[index].window_id.clone(),
                    pane_id: candidates[index].pane_id.clone(),
                }));
            }
            Key::Esc | Key::Ctrl('c') | Key::Ctrl('g') => return Ok(None),
            Key::Up | Key::Ctrl('p') => selected = selected.saturating_sub(1),
            Key::Down | Key::Ctrl('n') => {
                if selected + 1 < filtered.len() {
                    selected += 1;
                }
            }
            Key::Ctrl('u') => {
                query.clear();
                filtered = rank(candidates, &query);
                selected = 0;
            }
            Key::Backspace => {
                query.pop();
                filtered = rank(candidates, &query);
                selected = 0;
            }
            Key::Char(c) if !c.is_control() => {
                query.push(c);
                filtered = rank(candidates, &query);
                selected = 0;
            }
            _ => {}
        }
    }
}

fn render<W: Write>(
    screen: &mut W,
    candidates: &[Candidate],
    filtered: &[usize],
    query: &str,
    selected: usize,
) -> Result<()> {
    let (cols, rows) = termion::terminal_size().unwrap_or((80, 24));
    let width = cols.max(4) as usize;
    let content_width = width - 2;
    let list_height = rows.saturating_sub(2) as usize;

    write!(
        screen,
        "{}{}{}>{} {query}",
        clear::All,
        cursor::Goto(1, 1),
        color::Fg(color::Cyan),
        color::Fg(color::Reset),
    )?;
    let info = format!("  {}/{} ", filtered.len(), candidates.len());
    let rule = width.saturating_sub(info.chars().count());
    write!(
        screen,
        "{}{}{info}{}{}",
        cursor::Goto(1, 2),
        color::Fg(color::LightBlack),
        "─".repeat(rule),
        color::Fg(color::Reset),
    )?;

    let total = filtered.len();
    let start = if list_height > 0 && selected >= list_height {
        selected - list_height + 1
    } else {
        0
    };
    let scrollbar = (total > list_height && list_height > 0).then(|| {
        let size = (list_height * list_height / total).max(1);
        let pos = start * list_height / total;
        pos..(pos + size).min(list_height)
    });

    for (offset, &index) in filtered.iter().enumerate().skip(start).take(list_height) {
        let line = offset - start;
        let row = (line + 3) as u16;
        let current = offset == selected;
        let positions = fuzzy_score(query, &candidates[index].display)
            .map(|(_, p)| p)
            .unwrap_or_default();
        write!(screen, "{}", cursor::Goto(1, row))?;

        if current {
            write!(
                screen,
                "{}{}▌ {}",
                color::Bg(color::LightBlack),
                color::Fg(color::Red),
                color::Fg(color::Reset)
            )?;
        } else {
            write!(screen, "  ")?;
        }

        for (char_index, ch) in candidates[index].display.chars().enumerate() {
            if char_index >= content_width {
                break;
            }
            if positions.contains(&char_index) {
                write!(
                    screen,
                    "{}{ch}{}",
                    color::Fg(color::Cyan),
                    color::Fg(color::Reset)
                )?;
            } else {
                write!(screen, "{ch}")?;
            }
        }

        if current {
            write!(screen, "{}{}", clear::UntilNewline, color::Bg(color::Reset))?;
        }
        if scrollbar
            .as_ref()
            .is_some_and(|thumb| thumb.contains(&line))
        {
            write!(
                screen,
                "{}{}│{}",
                cursor::Goto(width as u16, row),
                color::Fg(color::LightBlack),
                color::Fg(color::Reset)
            )?;
        }
    }

    let cursor_col = (query.chars().count() + 3).min(width) as u16;
    write!(screen, "{}", cursor::Goto(cursor_col, 1))?;
    screen.flush()?;
    Ok(())
}

fn rank(candidates: &[Candidate], query: &str) -> Vec<usize> {
    let mut scored: Vec<(usize, i32)> = candidates
        .iter()
        .enumerate()
        .filter_map(|(index, candidate)| {
            fuzzy_score(query, &candidate.display).map(|(score, _)| (index, score))
        })
        .collect();
    scored.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    scored.into_iter().map(|(index, _)| index).collect()
}

const SCORE_MIN: i32 = i32::MIN / 2;
const MATCH_CONSECUTIVE: i32 = 1000;
const BONUS_SLASH: i32 = 900;
const BONUS_DELIMITER: i32 = 850;
const BONUS_WORD: i32 = 800;
const BONUS_CAPITAL: i32 = 700;
const BONUS_DOT: i32 = 600;
const GAP_INNER: i32 = -10;
const GAP_EDGE: i32 = -5;

fn boundary_bonus(prev: char, cur: char) -> i32 {
    if !cur.is_alphanumeric() {
        return 0;
    }
    match prev {
        '/' => BONUS_SLASH,
        '-' | '_' | ' ' => BONUS_WORD,
        ':' | ';' | '|' | ',' | '[' | ']' | '(' | ')' => BONUS_DELIMITER,
        '.' => BONUS_DOT,
        p if p.is_ascii_lowercase() && cur.is_ascii_uppercase() => BONUS_CAPITAL,
        p if !p.is_ascii_digit() && cur.is_ascii_digit() => BONUS_CAPITAL,
        _ => 0,
    }
}

fn fuzzy_score(query: &str, text: &str) -> Option<(i32, Vec<usize>)> {
    if query.is_empty() {
        return Some((0, Vec::new()));
    }
    let needle: Vec<char> = query.chars().map(|c| c.to_ascii_lowercase()).collect();
    let hay: Vec<char> = text.chars().collect();
    let (n, w) = (needle.len(), hay.len());
    if n > w || !is_subsequence(&needle, &hay) {
        return None;
    }

    let mut bonus = vec![0; w];
    let mut prev = '/';
    for (j, &ch) in hay.iter().enumerate() {
        bonus[j] = boundary_bonus(prev, ch);
        prev = ch;
    }

    let mut d = vec![SCORE_MIN; n * w];
    let mut m = vec![SCORE_MIN; n * w];
    for i in 0..n {
        let gap = if i == n - 1 { GAP_EDGE } else { GAP_INNER };
        let mut running = SCORE_MIN;
        for j in 0..w {
            if hay[j].to_ascii_lowercase() == needle[i] {
                let score = if i == 0 {
                    (j as i32) * GAP_EDGE + bonus[j]
                } else if j > 0 {
                    let diag = (i - 1) * w + (j - 1);
                    (m[diag] + bonus[j]).max(d[diag] + MATCH_CONSECUTIVE)
                } else {
                    SCORE_MIN
                };
                d[i * w + j] = score;
                running = score.max(running.saturating_add(gap));
            } else {
                running = running.saturating_add(gap);
            }
            m[i * w + j] = running;
        }
    }

    let best = m[(n - 1) * w + (w - 1)];
    let mut positions = vec![0usize; n];
    let mut required = false;
    let mut upper = w;
    for i in (0..n).rev() {
        let mut j = upper;
        while j > 0 {
            j -= 1;
            let cell = i * w + j;
            if d[cell] != SCORE_MIN && (required || d[cell] == m[cell]) {
                required =
                    i > 0 && j > 0 && m[cell] == d[(i - 1) * w + (j - 1)] + MATCH_CONSECUTIVE;
                positions[i] = j;
                upper = j;
                break;
            }
        }
    }
    Some((best, positions))
}

fn is_subsequence(needle: &[char], hay: &[char]) -> bool {
    let mut i = 0;
    for &ch in hay {
        if i < needle.len() && ch.to_ascii_lowercase() == needle[i] {
            i += 1;
        }
    }
    i == needle.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate(display: &str) -> Candidate {
        Candidate {
            display: display.to_string(),
            window_id: "@0".into(),
            pane_id: None,
        }
    }

    #[test]
    fn empty_query_matches_everything() {
        assert_eq!(fuzzy_score("", "anything"), Some((0, Vec::new())));
    }

    #[test]
    fn non_subsequence_does_not_match() {
        assert!(fuzzy_score("xyz", "12: ~/api").is_none());
    }

    #[test]
    fn matches_are_case_insensitive_and_report_positions() {
        let (_, positions) = fuzzy_score("API", "12: ~/api [claude: idle]").unwrap();
        assert_eq!(positions, vec![6, 7, 8]);
    }

    #[test]
    fn boundary_and_consecutive_matches_score_higher() {
        let consecutive = fuzzy_score("api", "~/api").unwrap().0;
        let scattered = fuzzy_score("api", "a-p-i").unwrap().0;
        assert!(consecutive > scattered);
    }

    #[test]
    fn rank_orders_by_score_then_original_index() {
        let candidates = [
            candidate("12: ~/notes"),
            candidate("7: ~/api"),
            candidate("3: ~/api/app"),
        ];
        let order = rank(&candidates, "api");
        assert_eq!(order, vec![1, 2]);
    }

    #[test]
    fn picks_the_best_alignment_not_the_leftmost() {
        let (score, positions) = fuzzy_score("t", "23: ~/projects/tmux-select").unwrap();
        let hay: Vec<char> = "23: ~/projects/tmux-select".chars().collect();
        assert_eq!(hay[positions[0] - 1], '/');
        assert!(score > BONUS_SLASH / 2);
    }

    #[test]
    fn optimal_alignment_avoids_the_greedy_inversion() {
        let (_, positions) = fuzzy_score("st", "postgres/stage.rs").unwrap();
        let hay: Vec<char> = "postgres/stage.rs".chars().collect();
        assert_eq!(hay[positions[0] - 1], '/');
        assert_eq!(positions[1], positions[0] + 1);
    }

    #[test]
    fn both_tmux_windows_match_above_a_non_match() {
        let candidates = [
            candidate("21: ~/notes"),
            candidate("22: ~/.config/tmux"),
            candidate("23: ~/projects/tmux-select"),
        ];
        assert_eq!(rank(&candidates, "tm"), vec![1, 2]);
    }

    #[test]
    fn rank_with_empty_query_keeps_all_in_order() {
        let candidates = [candidate("a"), candidate("b"), candidate("c")];
        assert_eq!(rank(&candidates, ""), vec![0, 1, 2]);
    }
}
