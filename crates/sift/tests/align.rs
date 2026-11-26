//! What the model and alignment layers decide, pinned as readable tables.

use std::fmt::Write;

use sift::diff::{Align, Cell, FileDiff, Row, RowKind, Source, Strategy, unified};

fn table(old: &str, new: &str) -> String {
    aligned(Align::default(), old, new)
}

fn aligned(align: Align, old: &str, new: &str) -> String {
    let diff = FileDiff::compute(Source::new(old), Source::new(new));
    table_of(&diff, align.rows(&diff))
}

fn stacked(old: &str, new: &str) -> String {
    let diff = FileDiff::compute(Source::new(old), Source::new(new));
    table_of(&diff, unified(&Align::default().rows(&diff)))
}

fn table_of(diff: &FileDiff, rows: Vec<Row>) -> String {
    let mut rendered = String::new();
    for row in rows {
        let sigil = match row.kind {
            RowKind::Equal => ' ',
            RowKind::Delete => '-',
            RowKind::Insert => '+',
            RowKind::Replace => '~',
        };
        writeln!(
            rendered,
            "{sigil} {:>3} {:>3} │ {:<38} │ {}",
            number(&row.old),
            number(&row.new),
            text(&diff.old, &row.old),
            text(&diff.new, &row.new),
        )
        .expect("writing to a String cannot fail");
    }
    rendered
}

fn number(cell: &Cell) -> String {
    cell.line.map_or_else(|| "·".to_owned(), |line| (line + 1).to_string())
}

fn text(source: &Source, cell: &Cell) -> String {
    let Some(line) = cell.line else {
        return String::new();
    };
    let line = source.line(line);

    let mut rendered = String::new();
    let mut at = 0;
    for span in &cell.emphasis {
        rendered.push_str(&line[at..span.start]);
        rendered.push('⟦');
        rendered.push_str(&line[span.start..span.end]);
        rendered.push('⟧');
        at = span.end;
    }
    rendered.push_str(&line[at..]);
    rendered
}

const MODIFIED_BLOCK_OLD: &str = "alpha = 1\nbeta = 2\ngamma = 3\n";
const MODIFIED_BLOCK_NEW: &str = "zeta = 9\nalpha = 10\nbeta = 20\ngamma = 30\n";

#[test]
fn emphasis_marks_only_what_changed_within_a_line() {
    let old = "fn total(cart: &Cart) -> u32 {\n    let total = price * quantity;\n    total\n}\n";
    let new =
        "fn total(cart: &Cart) -> u32 {\n    let total = price * quantity + tax;\n    total\n}\n";
    insta::assert_snapshot!(table(old, new));
}

#[test]
fn a_line_inserted_into_a_modified_block_does_not_shift_every_pairing() {
    insta::assert_snapshot!(table(MODIFIED_BLOCK_OLD, MODIFIED_BLOCK_NEW));
}

#[test]
fn positional_strategy_shows_what_best_match_pairing_is_avoiding() {
    let align = Align { strategy: Strategy::Positional, ..Align::default() };
    insta::assert_snapshot!(aligned(align, MODIFIED_BLOCK_OLD, MODIFIED_BLOCK_NEW));
}

#[test]
fn unrelated_lines_share_a_row_but_are_never_emphasised() {
    let old = "use std::io;\nlet x = 1;\n";
    let new = "use std::io;\n#[derive(Debug, Clone, Copy, PartialEq)]\n";
    insta::assert_snapshot!(table(old, new));
}

#[test]
fn a_barely_similar_pair_shares_a_row_but_gets_no_emphasis() {
    let old = "    let handler = Registry::new(config);\n";
    let new = "    let handler = Dispatcher::build(cfg, opts, &mut log);\n";
    insta::assert_snapshot!(table(old, new));
}

#[test]
fn a_longer_new_side_pads_with_filler() {
    let old = "one\ntwo\nthree\n";
    let new = "one\ninserted\nthree\nalso inserted\n";
    insta::assert_snapshot!(table(old, new));
}

#[test]
fn a_trailing_newline_opens_a_final_empty_line_as_an_editor_shows_it() {
    let terminated = Source::new("alpha\nbeta\n");
    let unterminated = Source::new("alpha\nbeta");

    assert_eq!(terminated.lines().collect::<Vec<_>>(), ["alpha", "beta", ""]);
    assert_eq!(unterminated.lines().collect::<Vec<_>>(), ["alpha", "beta"]);
    assert!(Source::new("alpha\n").final_newline());
}

#[test]
fn carriage_returns_and_a_missing_final_newline_are_stripped_from_lines() {
    let source = Source::new("alpha\r\nbeta\r\ngamma");
    let lines: Vec<&str> = source.lines().collect();

    assert_eq!(lines, ["alpha", "beta", "gamma"]);
    assert!(!source.final_newline());
    assert!(Source::new("").is_empty());
}

#[test]
fn a_row_is_identified_by_the_lines_it_shows_rather_than_its_position() {
    let diff = FileDiff::compute(Source::new("one\ntwo\n"), Source::new("one\n"));
    let rows: Vec<Row> = Align::default().rows(&diff);

    assert_eq!(rows[0].anchor(), (Some(0), Some(0)));
    assert_eq!(rows[1].anchor(), (Some(1), None));
}

#[test]
fn unified_splits_a_modified_row_into_a_deletion_above_its_insertion() {
    insta::assert_snapshot!(stacked(MODIFIED_BLOCK_OLD, MODIFIED_BLOCK_NEW));
}

#[test]
fn unified_keeps_the_intraline_emphasis_of_the_row_it_split() {
    let old = "let total = price * quantity;\n";
    let new = "let total = price * quantity + tax;\n";
    insta::assert_snapshot!(stacked(old, new));
}

#[test]
fn unified_never_emits_a_row_with_both_sides_filled_except_where_they_agree() {
    let diff = FileDiff::compute(Source::new(MODIFIED_BLOCK_OLD), Source::new(MODIFIED_BLOCK_NEW));

    for row in unified(&Align::default().rows(&diff)) {
        assert_ne!(row.kind, RowKind::Replace);
        if row.kind != RowKind::Equal {
            assert!(row.old.line.is_none() || row.new.line.is_none());
        }
    }
}
