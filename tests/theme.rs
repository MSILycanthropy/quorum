//! That Tokyo Night resolves the tints Quorum expects of it.

use quorum::theme::TokyoNight;

#[test]
fn emphasis_tints_lean_from_the_line_tint_toward_their_accent() {
    let tints = TokyoNight::TINTS;

    assert_eq!(tints.added.to_string(), "#20303b");
    assert_eq!(tints.added_emphasis.to_string(), "#3f5746");
    assert_eq!(tints.removed.to_string(), "#37222c");
    assert_eq!(tints.removed_emphasis.to_string(), "#673744");
}
