//! That the three columns share the width evenly and follow a dragged divider.

use quorum::ui::{Columns, Split};
use ratatui::layout::{Position, Rect};

const AREA: Rect = Rect { x: 0, y: 0, width: 180, height: 40 };

fn widths(columns: Columns) -> [u16; 3] {
    [columns.tree.width, columns.diff.width, columns.conversation.width]
}

fn dragged(split: &mut Split, from: u16, to: u16) -> Columns {
    assert!(split.grab(Position::new(from, 10)), "the divider is at column {from}");
    split.drag(Position::new(to, 10));
    split.release();

    split.columns(AREA, [true, true, true])
}

#[test]
fn three_columns_share_the_width_evenly() {
    let mut split = Split::default();

    assert_eq!(widths(split.columns(AREA, [true, true, true])), [60, 60, 60]);
}

#[test]
fn the_columns_stay_side_by_side_with_no_gap() {
    let mut split = Split::default();
    let columns = split.columns(AREA, [true, true, true]);

    assert_eq!(columns.tree.right(), columns.diff.x);
    assert_eq!(columns.diff.right(), columns.conversation.x);
    assert_eq!(columns.conversation.right(), AREA.right());
}

#[test]
fn a_hidden_column_gives_its_share_to_the_rest() {
    let mut split = Split::default();

    assert_eq!(widths(split.columns(AREA, [false, true, true])), [0, 90, 90]);
}

#[test]
fn a_column_hidden_and_shown_again_comes_back_the_width_it_was() {
    let mut split = Split::default();

    split.columns(AREA, [false, true, true]);

    assert_eq!(widths(split.columns(AREA, [true, true, true])), [60, 60, 60]);
}

#[test]
fn dragging_the_listing_divider_moves_width_between_the_tree_and_the_diff() {
    let mut split = Split::default();
    split.columns(AREA, [true, true, true]);

    assert_eq!(widths(dragged(&mut split, 59, 39)), [40, 80, 60]);
}

#[test]
fn dragging_the_conversation_divider_moves_width_between_the_diff_and_the_agent() {
    let mut split = Split::default();
    split.columns(AREA, [true, true, true]);

    assert_eq!(widths(dragged(&mut split, 120, 100)), [60, 40, 80]);
}

#[test]
fn a_divider_dragged_past_a_minimum_stops_at_it() {
    let mut split = Split::default();
    split.columns(AREA, [true, true, true]);

    assert_eq!(widths(dragged(&mut split, 59, 0)), [24, 96, 60]);
}

#[test]
fn a_divider_dragged_into_the_diff_leaves_it_its_minimum() {
    let mut split = Split::default();
    split.columns(AREA, [true, true, true]);

    assert_eq!(widths(dragged(&mut split, 59, 179)), [80, 40, 60]);
}

#[test]
fn a_drag_survives_a_resize_as_a_proportion() {
    let mut split = Split::default();
    split.columns(AREA, [true, true, true]);
    dragged(&mut split, 59, 39);

    let narrower = Rect { width: 160, ..AREA };

    assert_eq!(widths(split.columns(narrower, [true, true, true])), [35, 72, 53]);
}

#[test]
fn a_terminal_too_narrow_for_every_minimum_takes_from_the_most_generous_column() {
    let mut split = Split::default();
    let squeezed = Rect { width: 90, ..AREA };

    assert_eq!(widths(split.columns(squeezed, [true, true, true])), [24, 40, 26]);
}

#[test]
fn evening_the_columns_undoes_every_drag() {
    let mut split = Split::default();
    split.columns(AREA, [true, true, true]);
    dragged(&mut split, 59, 39);
    split.even();

    assert_eq!(widths(split.columns(AREA, [true, true, true])), [60, 60, 60]);
}

#[test]
fn a_click_that_misses_every_divider_grabs_nothing() {
    let mut split = Split::default();
    split.columns(AREA, [true, true, true]);

    assert!(!split.grab(Position::new(30, 10)));
    assert!(!split.held());
}

#[test]
fn the_divider_of_a_hidden_column_cannot_be_grabbed() {
    let mut split = Split::default();
    split.columns(AREA, [false, true, true]);

    assert!(!split.grab(Position::new(0, 10)));
}
