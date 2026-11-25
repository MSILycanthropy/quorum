//! Host text made safe to draw, then wrapped into the rows of an annotation.

use unicode_segmentation::UnicodeSegmentation;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

pub fn clean(text: &str, tab_width: usize) -> String {
    let stop = tab_width.max(1);
    let mut cleaned = String::with_capacity(text.len());
    let mut column = 0;
    let mut characters = text.chars().peekable();

    while let Some(character) = characters.next() {
        match character {
            '\r' => {
                characters.next_if_eq(&'\n');
                cleaned.push('\n');
                column = 0;
            }
            '\n' => {
                cleaned.push('\n');
                column = 0;
            }
            '\t' => {
                let spaces = stop - column % stop;
                cleaned.extend(std::iter::repeat_n(' ', spaces));
                column += spaces;
            }
            character if character.is_control() => {}
            character => {
                cleaned.push(character);
                column += character.width().unwrap_or(0);
            }
        }
    }

    cleaned.truncate(cleaned.trim_end_matches('\n').len());
    cleaned
}

pub fn heading(text: &str, tab_width: usize) -> String {
    clean(text, tab_width).replace('\n', " ")
}

pub fn has_body(body: &str) -> bool {
    body.chars().any(|character| character == '\t' || !character.is_control())
}

pub fn cut(text: &str, width: usize) -> String {
    if text.width() <= width {
        return text.to_owned();
    }
    if width == 0 {
        return String::new();
    }

    let mut kept = String::new();
    let mut used = 0;
    for grapheme in text.graphemes(true) {
        let advance = grapheme.width();
        if used + advance + 1 > width {
            break;
        }
        kept.push_str(grapheme);
        used += advance;
    }
    kept.push('…');

    kept
}

pub fn wrap(text: &str, width: usize) -> Vec<String> {
    if text.is_empty() {
        return Vec::new();
    }

    text.split('\n').flat_map(|line| Wrapper::new(line, width).rows()).collect()
}

/// One line of text being broken into rows no wider than `width`, each row
/// after the first starting with the line's own indentation.
struct Wrapper<'a> {
    rest: &'a str,
    width: usize,
    hanging: &'a str,
    rows: Vec<String>,
    row: String,
    used: usize,
    fresh: bool,
}

impl<'a> Wrapper<'a> {
    fn new(line: &'a str, width: usize) -> Self {
        let indent = &line[..line.len() - line.trim_start_matches(' ').len()];
        let hanging = if indent.width() < width { indent } else { "" };

        Self {
            rest: &line[indent.len()..],
            width,
            hanging,
            rows: Vec::new(),
            row: hanging.to_owned(),
            used: hanging.width(),
            fresh: true,
        }
    }

    fn rows(mut self) -> Vec<String> {
        if self.width == 0 {
            return vec![String::new()];
        }

        let mut pending = "";
        for (word, spaces) in words(self.rest) {
            if word.is_empty() {
                pending = spaces;
                continue;
            }
            let wanted = pending.width() + word.width();
            if self.used + wanted <= self.width {
                self.row.push_str(pending);
                self.row.push_str(word);
                self.used += wanted;
                self.fresh = false;
            } else {
                self.place(word);
            }
            pending = spaces;
        }
        self.rows.push(self.row);

        self.rows
    }

    fn place(&mut self, word: &str) {
        if !self.fresh {
            self.break_row();
        }
        if self.used + word.width() <= self.width {
            self.row.push_str(word);
            self.used += word.width();
            self.fresh = false;
            return;
        }

        for grapheme in word.graphemes(true) {
            let advance = grapheme.width();
            if self.used + advance > self.width && !self.fresh {
                self.break_row();
            }
            self.row.push_str(grapheme);
            self.used += advance;
            self.fresh = false;
        }
    }

    fn break_row(&mut self) {
        let finished = std::mem::replace(&mut self.row, self.hanging.to_owned());
        self.rows.push(finished);
        self.used = self.hanging.width();
        self.fresh = true;
    }
}

fn words(text: &str) -> impl Iterator<Item = (&str, &str)> {
    let mut rest = text;

    std::iter::from_fn(move || {
        if rest.is_empty() {
            return None;
        }
        let word_end = rest.find(char::is_whitespace).unwrap_or(rest.len());
        let (word, after) = rest.split_at(word_end);
        let spaces_end =
            after.find(|character: char| !character.is_whitespace()).unwrap_or(after.len());
        let (spaces, next) = after.split_at(spaces_end);
        rest = next;

        Some((word, spaces))
    })
}
