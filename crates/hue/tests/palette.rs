//! That a palette holds what the terminal answered, and only that.

use hue::{Ansi, Palette, Rgb, Slot};
use ratatui::style::Color;

#[test]
fn a_terminal_that_answers_every_query_reports_its_colours() {
    let replies = b"\x1b]10;rgb:c0c0/caca/f5f5\x1b\\\
        \x1b]11;rgb:1a/1b/26\x07\
        \x1b]4;1;rgb:f7f7/7676/8e8e\x1b\\\
        \x1b]4;5;rgb:bbbb/9a9a/f7f7\x1b\\\
        \x1b[?62;22c";

    let palette = Palette::from_replies(replies);

    assert_eq!(palette.foreground(), Some(Rgb::new(0xc0, 0xca, 0xf5)));
    assert_eq!(palette.background(), Some(Rgb::new(0x1a, 0x1b, 0x26)));
    assert_eq!(palette.ansi(Ansi::Red), Some(Rgb::new(0xf7, 0x76, 0x8e)));
    assert_eq!(palette.ansi(Ansi::Magenta), Some(Rgb::new(0xbb, 0x9a, 0xf7)));
}

#[test]
fn channels_of_any_width_scale_to_eight_bits() {
    let palette = Palette::from_replies(b"\x1b]10;rgb:f/80/fff\x07");

    assert_eq!(palette.foreground().map(|color| color.to_string()).as_deref(), Some("#ff80ff"));
}

#[test]
fn a_colour_the_terminal_leaves_unanswered_is_missing() {
    let palette = Palette::from_replies(b"\x1b]4;1;rgb:ff/00/00\x07\x1b[?62c");

    assert_eq!(palette.ansi(Ansi::Red), Some(Rgb::new(0xff, 0, 0)));
    assert_eq!(palette.ansi(Ansi::Green), None);
    assert_eq!(palette.background(), None);
}

#[test]
fn every_slot_reads_the_reply_to_its_own_query() {
    let palette = Palette::from_replies(
        b"\x1b]10;rgb:01/01/01\x07\x1b]11;rgb:02/02/02\x07\x1b]12;rgb:03/03/03\x07\x1b]4;6;rgb:04/04/04\x07",
    );

    assert_eq!(palette.get(Slot::Foreground), Some(Rgb::new(1, 1, 1)));
    assert_eq!(palette.get(Slot::Background), Some(Rgb::new(2, 2, 2)));
    assert_eq!(palette.get(Slot::Cursor), Some(Rgb::new(3, 3, 3)));
    assert_eq!(palette.get(Slot::Ansi(Ansi::Cyan)), Some(Rgb::new(4, 4, 4)));
    assert_eq!(palette.cursor(), palette.get(Slot::Cursor));
}

#[test]
fn a_bright_colour_lands_apart_from_its_plain_one() {
    let palette = Palette::from_replies(b"\x1b]4;13;rgb:ff/00/ff\x07");

    assert_eq!(palette.ansi(Ansi::BrightMagenta), Some(Rgb::new(0xff, 0, 0xff)));
    assert_eq!(palette.ansi(Ansi::Magenta), None);
}

#[test]
fn an_index_past_the_sixteen_named_colours_is_ignored() {
    assert_eq!(Palette::from_replies(b"\x1b]4;200;rgb:ff/00/ff\x07"), Palette::default());
}

#[test]
fn a_colour_drawn_through_ratatui_keeps_its_channels() {
    assert_eq!(Color::from(Rgb::new(1, 2, 3)), Color::Rgb(1, 2, 3));
}
