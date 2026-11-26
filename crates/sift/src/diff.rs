//! Structured text diffs with intraline emphasis. No I/O, no git.

mod align;
mod flow;
mod model;

pub use align::{Align, Cell, Emphasis, Row, RowKind, Side, Similarity, Span, Strategy, unified};
pub use flow::{Cells, Highlight, Highlighting, Layout, Metric, Painted, Run, ScreenRow, Viewport};
pub use model::{Change, ChangeKind, Counts, FileDiff, Source, Whitespace};
