//! Whether span layers survive being cut apart by wrapping and scrolling.

use std::fmt::Write;
use std::path::Path;

use sift::diff::{
    Align, Cells, FileDiff, Highlighting, Layout, Painted, ScreenRow, Source, Viewport,
};
use sift::syntax::Syntax;

fn screen(old: &str, new: &str, viewport: Viewport) -> String {
    let diff = FileDiff::compute(Source::new(old), Source::new(new));
    let rows = Align::default().rows(&diff);
    let layout = Layout {
        viewport,
        metric: &Cells,
        old_syntax: &Highlighting::EMPTY,
        new_syntax: &Highlighting::EMPTY,
    };
    render(&layout.screen_rows(&diff, &rows))
}

fn render(rows: &[ScreenRow]) -> String {
    let mut rendered = String::new();
    for row in rows {
        writeln!(
            rendered,
            "{:>2}{} │{:<26}│{}│",
            row.row,
            if row.continuation { " ↳" } else { "  " },
            painted(&row.old),
            painted(&row.new),
        )
        .expect("writing to a String cannot fail");
    }
    rendered
}

fn painted(side: &Painted) -> String {
    let mut rendered = String::new();
    for run in &side.runs {
        if run.emphasis {
            rendered.push('⟦');
            rendered.push_str(&run.text);
            rendered.push('⟧');
        } else {
            rendered.push_str(&run.text);
        }
    }
    rendered
}

fn wrapped(width: usize) -> Viewport {
    Viewport { width, wrap: true, ..Viewport::default() }
}

const LONG_CALL_OLD: &str = "let result = compute(alpha, beta, gamma, delta, epsilon);\n";
const LONG_CALL_NEW: &str = "let result = compute(alpha, beta, GAMMA, delta, epsilon);\n";

#[test]
fn emphasis_survives_a_wrap_that_cuts_through_it() {
    let old = "let message = format!(\"hello there {name}\");\n";
    let new = "let message = format!(\"goodbye for now {name}\");\n";
    insta::assert_snapshot!(screen(old, new, wrapped(26)));
}

#[test]
fn tabs_expand_to_stops_and_carry_their_spans_with_them() {
    let old = "\tif ok {\n\t\treturn one;\n\t}\n";
    let new = "\tif ok {\n\t\treturn two;\n\t}\n";
    insta::assert_snapshot!(screen(old, new, wrapped(26)));
}

#[test]
fn wide_glyphs_are_never_split_across_a_break() {
    let old = "label = \"日本語のテキスト\"\n";
    let new = "label = \"日本語のテキスト!\"\n";
    insta::assert_snapshot!(screen(old, new, wrapped(20)));
}

#[test]
fn the_two_sides_can_wrap_to_different_heights() {
    let old = "short\n";
    let new = "a much longer replacement line that needs several visual rows\n";
    insta::assert_snapshot!(screen(old, new, wrapped(20)));
}

#[test]
fn horizontal_scroll_clips_emphasis_to_the_window() {
    let viewport = Viewport { width: 24, scroll: 26, ..Viewport::default() };
    insta::assert_snapshot!(screen(LONG_CALL_OLD, LONG_CALL_NEW, viewport));
}

#[test]
fn without_wrapping_a_long_line_is_one_row_clipped_to_width() {
    let viewport = Viewport { width: 24, ..Viewport::default() };
    insta::assert_snapshot!(screen(LONG_CALL_OLD, LONG_CALL_NEW, viewport));
}

fn highlighted(old: &str, new: &str, viewport: Viewport) -> String {
    let diff = FileDiff::compute(Source::new(old), Source::new(new));
    let rows = Align::default().rows(&diff);
    let syntax = Syntax::default();
    let old_syntax = syntax.highlight(&diff.old, Path::new("demo.rs")).expect("valid source");
    let new_syntax = syntax.highlight(&diff.new, Path::new("demo.rs")).expect("valid source");

    let layout =
        Layout { viewport, metric: &Cells, old_syntax: &old_syntax, new_syntax: &new_syntax };

    let mut rendered = String::new();
    for row in layout.screen_rows(&diff, &rows) {
        writeln!(rendered, "{:>2}{} old {}", row.row, mark(&row), runs(&row.old))
            .expect("writing to a String cannot fail");
        writeln!(rendered, "{:>2}{} new {}", row.row, mark(&row), runs(&row.new))
            .expect("writing to a String cannot fail");
    }
    rendered
}

fn mark(row: &ScreenRow) -> &'static str {
    if row.continuation { " ↳" } else { "  " }
}

fn runs(side: &Painted) -> String {
    side.runs
        .iter()
        .map(|run| {
            let color = run.color.map_or_else(|| "········".to_owned(), |rgb| rgb.to_string());
            let emphasis = if run.emphasis { "⟦⟧" } else { "  " };
            format!("{color}{emphasis}{:?}", run.text)
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[test]
fn a_run_breaks_wherever_the_colour_or_the_emphasis_changes() {
    insta::assert_snapshot!(highlighted(
        "let total = 1;\n",
        "let total = 42;\n",
        Viewport::default()
    ));
}

#[test]
fn colours_survive_a_wrap_alongside_emphasis() {
    insta::assert_snapshot!(highlighted(
        "let message = format!(\"hello there\");\n",
        "let message = format!(\"goodbye now\");\n",
        wrapped(24)
    ));
}
