use std::cmp::Ordering;
use std::io::{self, Write};
use std::ops::Range;

use termion::event::Key;
use termion::input::TermRead;
use termion::raw::IntoRawMode;
use termion::screen::IntoAlternateScreen;
use termion::{clear, color, cursor};

use crate::tmux::Result;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SpanColor {
    Red,
    Green,
    Yellow,
}

pub struct Candidate {
    pub display: String,
    pub name: String,
    pub spans: Vec<(Range<usize>, SpanColor)>,
    pub window_id: String,
    pub pane_id: Option<String>,
}

pub struct Target {
    pub window_id: String,
    pub pane_id: Option<String>,
}

pub fn pick(
    mut candidates: Vec<Candidate>,
    default_index: usize,
    rename: impl FnMut(usize, &str) -> Result<Candidate>,
) -> Result<Option<Target>> {
    if candidates.is_empty() {
        return Ok(None);
    }
    let raw = io::stdout()
        .into_raw_mode()
        .map_err(|error| format!("failed to enter terminal raw mode (not a tty?): {error}"))?;
    let mut screen = raw.into_alternate_screen()?;
    run(
        &mut candidates,
        &mut screen,
        default_index,
        io::stdin().keys(),
        rename,
    )
}

fn run<W: Write>(
    candidates: &mut [Candidate],
    screen: &mut W,
    default_index: usize,
    mut keys: impl Iterator<Item = io::Result<Key>>,
    mut rename: impl FnMut(usize, &str) -> Result<Candidate>,
) -> Result<Option<Target>> {
    let mut query = String::new();
    let mut filtered = rank(candidates, &query);
    let mut selected = default_index.min(filtered.len().saturating_sub(1));

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
            Key::Ctrl('r') => {
                if let Some(&index) = filtered.get(selected) {
                    let candidate = candidates
                        .get_mut(index)
                        .ok_or("selected window is missing")?;
                    if let Some(replacement) =
                        edit_name(screen, &mut keys, &candidate.name, |name| {
                            rename(index, name)
                        })?
                    {
                        *candidate = replacement;
                        filtered = rank(candidates, &query);
                        selected = filtered
                            .iter()
                            .position(|&item| item == index)
                            .unwrap_or_else(|| selected.min(filtered.len().saturating_sub(1)));
                    }
                }
            }
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

fn edit_name<W: Write>(
    screen: &mut W,
    keys: &mut impl Iterator<Item = io::Result<Key>>,
    initial_name: &str,
    mut rename: impl FnMut(&str) -> Result<Candidate>,
) -> Result<Option<Candidate>> {
    let mut name = initial_name.replace(r"\\", r"\");
    let mut error = String::new();
    loop {
        render_name(screen, &name, &error)?;
        let Some(key) = keys.next() else {
            return Ok(None);
        };
        match key? {
            Key::Char('\n') | Key::Char('\r') => match rename(&name) {
                Ok(candidate) => return Ok(Some(candidate)),
                Err(failure) => error = failure.to_string(),
            },
            Key::Esc | Key::Ctrl('c') | Key::Ctrl('g') => return Ok(None),
            Key::Ctrl('u') => {
                name.clear();
                error.clear();
            }
            Key::Backspace => {
                name.pop();
                error.clear();
            }
            Key::Char(ch) if !ch.is_control() => {
                name.push(ch);
                error.clear();
            }
            _ => {}
        }
    }
}

fn render_name<W: Write>(screen: &mut W, name: &str, error: &str) -> Result<()> {
    let (cols, _) = termion::terminal_size().unwrap_or((80, 24));
    let width = usize::from(cols.max(9));
    let tail: String = name
        .chars()
        .skip(name.chars().count().saturating_sub(width - 9))
        .collect();
    let message: String = error
        .chars()
        .map(|ch| if ch.is_control() { ' ' } else { ch })
        .take(width)
        .collect();
    write!(
        screen,
        "{}{}Rename> {tail}{}Enter: save | Esc: cancel | Ctrl-U: clear{}{}{message}{}{}",
        clear::All,
        cursor::Goto(1, 1),
        cursor::Goto(1, 2),
        cursor::Goto(1, 3),
        color::Fg(color::Red),
        color::Fg(color::Reset),
        cursor::Goto((tail.chars().count() + 9) as u16, 1)
    )?;
    screen.flush()?;
    Ok(())
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
    let info = format!("  {}/{}  Ctrl-R: rename ", filtered.len(), candidates.len());
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
                continue;
            }
            let span = candidates[index]
                .spans
                .iter()
                .find_map(|(range, color)| range.contains(&char_index).then_some(*color));
            match span {
                Some(SpanColor::Red) => write!(
                    screen,
                    "{}{ch}{}",
                    color::Fg(color::Red),
                    color::Fg(color::Reset)
                )?,
                Some(SpanColor::Green) => write!(
                    screen,
                    "{}{ch}{}",
                    color::Fg(color::Green),
                    color::Fg(color::Reset)
                )?,
                Some(SpanColor::Yellow) => write!(
                    screen,
                    "{}{ch}{}",
                    color::Fg(color::Yellow),
                    color::Fg(color::Reset)
                )?,
                None => write!(screen, "{ch}")?,
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
    let mut scored: Vec<(usize, f64, usize)> = candidates
        .iter()
        .enumerate()
        .filter_map(|(index, candidate)| {
            fuzzy_score(query, &candidate.display)
                .map(|(score, positions)| (index, score, positions.first().copied().unwrap_or(0)))
        })
        .collect();
    scored.sort_by(|a, b| {
        b.1.partial_cmp(&a.1)
            .unwrap_or(Ordering::Equal)
            .then(a.2.cmp(&b.2))
            .then(a.0.cmp(&b.0))
    });
    scored.into_iter().map(|(index, _, _)| index).collect()
}

const WORD_START: f64 = 1.0;
const CONSECUTIVE: f64 = 2.0;
const DECAY: f64 = 0.5;
const GAP_PENALTY: f64 = 0.1;

fn fuzzy_score(query: &str, text: &str) -> Option<(f64, Vec<usize>)> {
    if query.is_empty() {
        return Some((0.0, Vec::new()));
    }
    let needle: Vec<char> = query.chars().map(|c| c.to_ascii_lowercase()).collect();
    let hay: Vec<char> = text.chars().collect();
    let (n, w) = (needle.len(), hay.len());
    if n > w || !is_subsequence(&needle, &hay) {
        return None;
    }

    let mut word_start = vec![false; w];
    let mut prev_alnum = false;
    for (j, &ch) in hay.iter().enumerate() {
        let alnum = ch.is_alphanumeric();
        word_start[j] = alnum && !prev_alnum;
        prev_alnum = alnum;
    }

    let mut weight = vec![1.0; n];
    for i in 1..n {
        weight[i] = weight[i - 1] * DECAY;
    }

    let mut score = vec![f64::NEG_INFINITY; n * w];
    let mut back = vec![usize::MAX; n * w];
    for i in 0..n {
        for j in 0..w {
            if hay[j].to_ascii_lowercase() != needle[i] {
                continue;
            }
            if i == 0 {
                score[j] = weight[0] * if word_start[j] { WORD_START } else { 0.0 };
                continue;
            }
            let mut best = f64::NEG_INFINITY;
            let mut from = usize::MAX;
            for jp in 0..j {
                let prev = score[(i - 1) * w + jp];
                if prev == f64::NEG_INFINITY {
                    continue;
                }
                let gap = (j - jp - 1) as f64;
                let value = if jp + 1 == j {
                    CONSECUTIVE
                } else if word_start[j] {
                    WORD_START
                } else {
                    0.0
                };
                let candidate = prev + weight[i] * value - GAP_PENALTY * gap;
                if candidate > best {
                    best = candidate;
                    from = jp;
                }
            }
            if from != usize::MAX {
                score[i * w + j] = best;
                back[i * w + j] = from;
            }
        }
    }

    let mut best = f64::NEG_INFINITY;
    let mut end = usize::MAX;
    for j in 0..w {
        if score[(n - 1) * w + j] > best {
            best = score[(n - 1) * w + j];
            end = j;
        }
    }
    let mut positions = vec![0usize; n];
    let mut j = end;
    for i in (0..n).rev() {
        positions[i] = j;
        j = back[i * w + j];
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

    fn keys(input: &[Key]) -> impl Iterator<Item = io::Result<Key>> + '_ {
        input.iter().cloned().map(Ok)
    }

    #[test]
    fn rename_saves_the_filtered_window_and_preserves_its_pane_target() {
        let mut candidates = [candidate("notes"), candidate("api")];
        candidates[1].window_id = "@9".into();
        candidates[1].pane_id = Some("%5".into());
        let mut calls = Vec::new();
        let target = run(
            &mut candidates,
            &mut Vec::new(),
            0,
            keys(&[
                Key::Char('a'),
                Key::Char('p'),
                Key::Ctrl('r'),
                Key::Char('2'),
                Key::Char('\n'),
                Key::Char('\n'),
            ]),
            |index, name| {
                calls.push((index, name.to_owned()));
                let mut updated = candidate(name);
                updated.window_id = "@9".into();
                updated.pane_id = Some("%5".into());
                Ok(updated)
            },
        )
        .unwrap()
        .unwrap();
        assert_eq!(calls, [(1, "api2".into())]);
        assert_eq!(candidates[1].display, "api2");
        assert_eq!(target.window_id, "@9");
        assert_eq!(target.pane_id.as_deref(), Some("%5"));
    }

    #[test]
    fn cancelling_rename_preserves_the_filter_and_selected_window() {
        for cancel in [Key::Esc, Key::Ctrl('c'), Key::Ctrl('g')] {
            let mut candidates = [candidate("notes"), candidate("api")];
            candidates[1].window_id = "@9".into();
            let target = run(
                &mut candidates,
                &mut Vec::new(),
                0,
                keys(&[
                    Key::Char('a'),
                    Key::Ctrl('r'),
                    Key::Ctrl('u'),
                    Key::Char('x'),
                    cancel,
                    Key::Char('\n'),
                ]),
                |_, _| panic!("cancel must not rename"),
            )
            .unwrap()
            .unwrap();
            assert_eq!(target.window_id, "@9");
            assert_eq!(candidates[1].name, "api");
        }
    }

    #[test]
    fn failed_rename_stays_in_editor_and_can_be_retried() {
        let mut candidates = [candidate("api")];
        let mut calls = Vec::new();
        let mut screen = Vec::new();
        run(
            &mut candidates,
            &mut screen,
            0,
            keys(&[
                Key::Ctrl('r'),
                Key::Char('\n'),
                Key::Ctrl('u'),
                Key::Char('x'),
                Key::Char('\n'),
                Key::Esc,
            ]),
            |_, name| {
                calls.push(name.to_owned());
                if name == "api" {
                    return Err("rename denied".into());
                }
                Ok(candidate(name))
            },
        )
        .unwrap();
        assert_eq!(calls, ["api", "x"]);
        assert_eq!(candidates[0].display, "x");
        assert!(String::from_utf8(screen).unwrap().contains("rename denied"));
    }

    #[test]
    fn renaming_can_remove_a_window_from_the_current_filter() {
        let mut candidates = [candidate("api"), candidate("notes")];
        candidates[1].window_id = "@7".into();
        let target = run(
            &mut candidates,
            &mut Vec::new(),
            0,
            keys(&[
                Key::Char('a'),
                Key::Ctrl('r'),
                Key::Ctrl('u'),
                Key::Char('x'),
                Key::Char('\n'),
                Key::Ctrl('r'),
                Key::Ctrl('u'),
                Key::Down,
                Key::Char('\n'),
            ]),
            |index, name| {
                assert_eq!(index, 0);
                assert_eq!(name, "x");
                Ok(candidate(name))
            },
        )
        .unwrap()
        .unwrap();
        assert_eq!(target.window_id, "@7");
    }

    #[test]
    fn rename_backspace_removes_a_unicode_character_and_eof_cancels() {
        let mut screen = Vec::new();
        let renamed = edit_name(
            &mut screen,
            &mut keys(&[Key::Backspace, Key::Char('\n')]),
            "name\u{e9}",
            |name| {
                assert_eq!(name, "name");
                Ok(candidate(name))
            },
        )
        .unwrap()
        .unwrap();
        assert_eq!(renamed.name, "name");
        assert!(
            edit_name(&mut screen, &mut keys(&[]), "name", |_: &str| panic!(
                "EOF must not rename"
            ))
            .unwrap()
            .is_none()
        );
    }

    fn candidate(display: &str) -> Candidate {
        Candidate {
            display: display.to_string(),
            name: display.to_string(),
            spans: Vec::new(),
            window_id: "@0".into(),
            pane_id: None,
        }
    }

    #[test]
    fn empty_query_matches_everything() {
        assert_eq!(fuzzy_score("", "anything"), Some((0.0, Vec::new())));
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
        assert_eq!(score, WORD_START);
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

    #[test]
    fn whole_word_match_beats_split_match() {
        let whole = fuzzy_score("abc", "~/.abc").unwrap().0;
        let split = fuzzy_score("abc", "~/ab/c").unwrap().0;
        assert!(whole > split);
    }

    #[test]
    fn early_word_start_beats_buried_run() {
        let early = fuzzy_score("abc", "~/ab/zc").unwrap().0;
        let buried = fuzzy_score("abc", "~/zabc").unwrap().0;
        assert!(early > buried);
    }

    #[test]
    fn contiguous_match_beats_word_starts_flung_across_an_annotation() {
        let contiguous = fuzzy_score("ai", "15: ~/projects/overlaid").unwrap().0;
        let scattered = fuzzy_score("ai", "13: ~/projects/arpg [claude: idle]")
            .unwrap()
            .0;
        assert!(contiguous > scattered);
    }

    #[test]
    fn ranks_word_start_and_consecutiveness_with_positional_decay() {
        let candidates = [
            candidate("~/abc"),
            candidate("~/.abc"),
            candidate("~/a-b-c"),
            candidate("~/a/b/c"),
            candidate("~/ab/c"),
            candidate("~/ab/zc"),
            candidate("~/zabc"),
            candidate("~/azbzc"),
        ];
        let ranked: Vec<&str> = rank(&candidates, "abc")
            .iter()
            .map(|&index| candidates[index].display.as_str())
            .collect();
        assert_eq!(
            ranked,
            vec![
                "~/abc", "~/.abc", "~/ab/c", "~/ab/zc", "~/a-b-c", "~/a/b/c", "~/zabc", "~/azbzc",
            ]
        );
    }
}
