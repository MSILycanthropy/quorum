//! That a theme resolves the tints Sift expects of it, from Tokyo Night or from the terminal.

use std::fmt::Write;

use sift::color::Rgb;
use sift::syntax::Syntax;
use sift::theme::{TerminalColors, Tints, TokyoNight};

const DARK: TerminalColors = TerminalColors {
    background: Rgb::new(0x1a, 0x1b, 0x26),
    foreground: Rgb::new(0xc0, 0xca, 0xf5),
    red: Rgb::new(0xf7, 0x76, 0x8e),
    green: Rgb::new(0x9e, 0xce, 0x6a),
    yellow: Rgb::new(0xe0, 0xaf, 0x68),
};

const LIGHT: TerminalColors = TerminalColors {
    background: Rgb::new(0xfd, 0xf6, 0xe3),
    foreground: Rgb::new(0x65, 0x7b, 0x83),
    red: Rgb::new(0xdc, 0x32, 0x2f),
    green: Rgb::new(0x85, 0x99, 0x00),
    yellow: Rgb::new(0xb5, 0x89, 0x00),
};

const BLACK: TerminalColors = TerminalColors {
    background: Rgb::new(0, 0, 0),
    foreground: Rgb::new(0xff, 0xff, 0xff),
    red: Rgb::new(0xff, 0x00, 0x00),
    green: Rgb::new(0x00, 0xff, 0x00),
    yellow: Rgb::new(0xff, 0xff, 0x00),
};

fn listed(tints: Tints) -> String {
    let mut listing = String::new();
    for (name, color) in [
        ("background", tints.background),
        ("foreground", tints.foreground),
        ("gutter", tints.gutter),
        ("filler", tints.filler),
        ("muted", tints.muted),
        ("attention", tints.attention),
        ("added", tints.added),
        ("added emphasis", tints.added_emphasis),
        ("added text", tints.added_text),
        ("removed", tints.removed),
        ("removed emphasis", tints.removed_emphasis),
        ("removed text", tints.removed_text),
        ("cursor", tints.cursor),
    ] {
        writeln!(listing, "{name:>16}  {color}").expect("writing to a String cannot fail");
    }

    listing
}

#[test]
fn emphasis_tints_lean_from_the_line_tint_toward_their_accent() {
    let tints = TokyoNight::TINTS;

    assert_eq!(tints.added.to_string(), "#20303b");
    assert_eq!(tints.added_emphasis.to_string(), "#3f5746");
    assert_eq!(tints.removed.to_string(), "#37222c");
    assert_eq!(tints.removed_emphasis.to_string(), "#673744");
}

#[test]
fn a_terminal_that_answers_every_query_reports_its_colours() {
    let replies = b"\x1b]10;rgb:c0c0/caca/f5f5\x1b\\\
        \x1b]11;rgb:1a/1b/26\x07\
        \x1b]4;1;rgb:f7f7/7676/8e8e\x1b\\\
        \x1b]4;2;rgb:9e9e/cece/6a6a\x1b\\\
        \x1b]4;3;rgb:e0e0/afaf/6868\x1b\\\
        \x1b[?62;22c";

    assert_eq!(TerminalColors::from_replies(replies), Some(DARK));
}

#[test]
fn channels_of_any_width_scale_to_eight_bits() {
    let replies = b"\x1b]10;rgb:f/80/fff\x07\x1b]11;rgb:0/0/0\x07\
        \x1b]4;1;rgb:ffff/0/0\x07\x1b]4;2;rgb:0/ffff/0\x07\x1b]4;3;rgb:ff/ff/0\x07";

    let colors = TerminalColors::from_replies(replies).expect("every colour was answered");

    assert_eq!(colors.foreground.to_string(), "#ff80ff");
}

#[test]
fn a_terminal_that_leaves_a_colour_unanswered_reports_nothing() {
    let replies = b"\x1b]10;rgb:c0c0/caca/f5f5\x1b\\\x1b]11;rgb:1a1a/1b1b/2626\x1b\\\x1b[?62c";

    assert_eq!(TerminalColors::from_replies(replies), None);
}

#[test]
fn a_dark_terminal_derives_tints_that_lean_from_its_background() {
    insta::assert_snapshot!(listed(DARK.tints()));
}

#[test]
fn a_light_terminal_derives_tints_that_lean_from_its_background() {
    insta::assert_snapshot!(listed(LIGHT.tints()));
}

#[test]
fn a_black_terminal_lifts_its_filler_rather_than_darkening_it() {
    insta::assert_snapshot!(listed(BLACK.tints()));
}

#[test]
fn a_light_terminal_reads_code_in_a_light_syntax_theme() {
    assert_eq!(LIGHT.theme().syntax, Syntax::named("github_light").expect("bundled"));
    assert_eq!(DARK.theme().syntax, Syntax::default());
}

#[test]
fn every_bundled_syntax_theme_parses() {
    for name in Syntax::names() {
        assert!(Syntax::named(name).is_ok(), "{name} does not parse");
    }
}

#[test]
fn an_unknown_syntax_theme_is_refused_by_name() {
    let refused = Syntax::named("no-such-theme").expect_err("no theme has that name");

    assert!(refused.to_string().starts_with("no syntax theme is named no-such-theme"));
}
