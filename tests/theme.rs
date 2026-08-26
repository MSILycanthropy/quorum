//! That Tokyo Night resolves the colours Quorum expects of it.

use std::str::FromStr;

use quorum::theme::TokyoNight;
use two_face::re_exports::syntect::highlighting::{Color, Highlighter};
use two_face::re_exports::syntect::parsing::ScopeStack;

fn foreground(scope: &str) -> Color {
    let theme = TokyoNight::theme();
    let highlighter = Highlighter::new(&theme);
    let stack = ScopeStack::from_str(scope).expect("test scopes are valid");
    highlighter.style_for_stack(stack.as_slice()).foreground
}

fn hex(color: Color) -> String {
    format!("#{:02x}{:02x}{:02x}", color.r, color.g, color.b)
}

#[test]
fn every_scope_selector_in_the_theme_parses() {
    assert!(TokyoNight::theme().scopes.len() > 30);
}

#[test]
fn source_scopes_take_their_tokyo_night_colours() {
    assert_eq!(hex(foreground("comment.line.double-slash.rust")), "#565f89");
    assert_eq!(hex(foreground("string.quoted.double.rust")), "#9ece6a");
    assert_eq!(hex(foreground("keyword.control.rust")), "#bb9af7");
    assert_eq!(hex(foreground("storage.type.rust")), "#bb9af7");
    assert_eq!(hex(foreground("constant.numeric.integer.rust")), "#ff9e64");
    assert_eq!(hex(foreground("entity.name.function.rust")), "#7aa2f7");
    assert_eq!(hex(foreground("entity.name.struct.rust")), "#2ac3de");
    assert_eq!(hex(foreground("variable.parameter.rust")), "#e0af68");
}

#[test]
fn a_scope_the_theme_does_not_name_falls_back_to_the_default_foreground() {
    assert_eq!(hex(foreground("source.rust")), "#c0caf5");
}

#[test]
fn a_more_specific_scope_wins_over_a_less_specific_one() {
    assert_eq!(hex(foreground("keyword.rust")), "#bb9af7");
    assert_eq!(hex(foreground("keyword.operator.arithmetic.rust")), "#89ddff");
}

#[test]
fn emphasis_tints_lean_from_the_line_tint_toward_their_accent() {
    let tints = TokyoNight::TINTS;

    assert_eq!(tints.added.to_string(), "#20303b");
    assert_eq!(tints.added_emphasis.to_string(), "#3f5746");
    assert_eq!(tints.removed.to_string(), "#37222c");
    assert_eq!(tints.removed_emphasis.to_string(), "#673744");
}
