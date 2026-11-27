//! Every command the diff understands, and the keys it comes with.

use keys::{Keymap, keymap};

use crate::ui::{PEEKING, SELECTING};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Command {
    NextFile,
    PreviousFile,
    ToggleFileList,
    Copy,
    CopyPath,
    CopyLocation,
    Select,
    SelectDown,
    SelectUp,
    ToggleViewed,
    NextUnviewed,
    PreviousUnviewed,
    NextHunk,
    PreviousHunk,
    ExpandFold,
    CloseFold,
    ToggleFold,
    OpenEverything,
    FoldEverything,
    NextLine,
    PreviousLine,
    ScrollLineDown,
    ScrollLineUp,
    HalfPageDown,
    HalfPageUp,
    PageDown,
    PageUp,
    Top,
    Bottom,
    CursorLineToMiddle,
    CursorLineToTop,
    CursorLineToBottom,
    OldSide,
    NewSide,
    ScrollLeft,
    ScrollRight,
    ToggleLayout,
    ToggleWrapping,
    ToggleWhitespace,
    NextAnnotation,
    PreviousAnnotation,
    ClearSelection,
    ClosePeek,
}

impl Command {
    pub fn moves(self) -> bool {
        matches!(
            self,
            Self::NextFile
                | Self::PreviousFile
                | Self::NextUnviewed
                | Self::PreviousUnviewed
                | Self::NextHunk
                | Self::PreviousHunk
                | Self::NextLine
                | Self::PreviousLine
                | Self::HalfPageDown
                | Self::HalfPageUp
                | Self::PageDown
                | Self::PageUp
                | Self::Top
                | Self::Bottom
                | Self::NextAnnotation
                | Self::PreviousAnnotation
        )
    }

    pub fn repeats(self) -> bool {
        matches!(
            self,
            Self::NextFile
                | Self::PreviousFile
                | Self::NextUnviewed
                | Self::PreviousUnviewed
                | Self::NextHunk
                | Self::PreviousHunk
                | Self::NextLine
                | Self::PreviousLine
                | Self::ScrollLineDown
                | Self::ScrollLineUp
                | Self::HalfPageDown
                | Self::HalfPageUp
                | Self::PageDown
                | Self::PageUp
                | Self::SelectDown
                | Self::SelectUp
                | Self::ScrollLeft
                | Self::ScrollRight
                | Self::NextAnnotation
                | Self::PreviousAnnotation
        )
    }
}

pub const ANNOTATIONS: &str = "Annotations";

/// The diff's own commands and the keys they come with: moving, folding,
/// selecting, copying, viewed marks, the view, and annotations.
pub fn keymap() -> Keymap<Command> {
    use Command as C;

    keymap! {
        "Files" {
            "next-file" => C::NextFile, "Next file", ["Tab", "]f"], hint: "file";
            "previous-file" => C::PreviousFile, "Previous file", ["S-Tab", "[f"];
            "toggle-file-list" => C::ToggleFileList, "Toggle file list", ["f", "C-b"], hint: "files";
            "copy" => C::Copy, "Copy the line", ["yy", "C-S-c"];
            "copy-selection" => C::Copy, "Copy the selected lines", ["y", "C-c", "C-S-c"], when: SELECTING;
            "copy-path" => C::CopyPath, "Copy path", ["yp"];
            "copy-location" => C::CopyLocation, "Copy path:line or lines", ["yl", "Y"];
            "toggle-viewed" => C::ToggleViewed, "Mark viewed, go to next", ["x"], hint: "viewed";
            "next-unviewed" => C::NextUnviewed, "Next unviewed file", ["]u"];
            "previous-unviewed" => C::PreviousUnviewed, "Previous unviewed file", ["[u"];
            "close-file" => C::ClosePeek, "Close the file outside the review", ["Esc"], when: PEEKING;
        }
        "Diff" {
            "next-hunk" => C::NextHunk, "Next hunk", ["n", "]c", "F7"], hint: "hunk";
            "previous-hunk" => C::PreviousHunk, "Previous hunk", ["N", "[c", "S-F7"];
            "next-line" => C::NextLine, "Down a line", ["j", "↓"];
            "previous-line" => C::PreviousLine, "Up a line", ["k", "↑"];
            "scroll-line-down" => C::ScrollLineDown, "Scroll down a line", ["C-e", "C-↓"];
            "scroll-line-up" => C::ScrollLineUp, "Scroll up a line", ["C-y", "C-↑"];
            "half-page-down" => C::HalfPageDown, "Down half a page", ["C-d"];
            "half-page-up" => C::HalfPageUp, "Up half a page", ["C-u"];
            "page-down" => C::PageDown, "Down a page", ["PgDn", "Space"];
            "page-up" => C::PageUp, "Up a page", ["PgUp"];
            "top" => C::Top, "Top, or the line counted", ["gg", "Home", "C-Home"];
            "bottom" => C::Bottom, "Bottom, or the line counted", ["G", "End", "C-End"];
            "cursor-line-to-middle" => C::CursorLineToMiddle, "Cursor line to the middle", ["zz"];
            "cursor-line-to-top" => C::CursorLineToTop, "Cursor line to the top", ["zt"];
            "cursor-line-to-bottom" => C::CursorLineToBottom, "Cursor line to the bottom", ["zb"];
            "old-side" => C::OldSide, "Cursor to the old side", ["H", "S-←"];
            "new-side" => C::NewSide, "Cursor to the new side", ["L", "S-→"];
            "scroll-left" => C::ScrollLeft, "Scroll left", ["h", "←"];
            "scroll-right" => C::ScrollRight, "Scroll right", ["l", "→"];
            "select" => C::Select, "Select lines", ["V", "v"];
            "select-down" => C::SelectDown, "Select down a line", ["S-↓"];
            "select-up" => C::SelectUp, "Select up a line", ["S-↑"];
            "clear-selection" => C::ClearSelection, "Let go of the selection", ["Esc"], when: SELECTING;
            "expand-fold" => C::ExpandFold, "Unfold, or toggle annotation", ["Enter", "o", "zo"];
            "close-fold" => C::CloseFold, "Fold back up", ["zc"];
            "toggle-fold" => C::ToggleFold, "Unfold or fold back up", ["za"];
            "open-everything" => C::OpenEverything, "Open every fold and annotation", ["zR"];
            "fold-everything" => C::FoldEverything, "Fold everything up again", ["zM"];
        }
        "View" {
            "toggle-layout" => C::ToggleLayout, "Split or unified", ["s"], hint: "layout";
            "toggle-wrapping" => C::ToggleWrapping, "Wrap long lines", ["w", "A-z"];
            "toggle-whitespace" => C::ToggleWhitespace, "Ignore whitespace", ["W"];
        }
        "Annotations" {
            "next-annotation" => C::NextAnnotation, "Next annotation", ["]d", "F8"];
            "previous-annotation" => C::PreviousAnnotation, "Previous annotation", ["[d", "S-F8"];
        }
    }
    .expect("the diff's own keys are spelt right and do not clash")
}
