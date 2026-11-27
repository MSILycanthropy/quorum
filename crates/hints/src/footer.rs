//! The footer, and how it fits its hints into one row.

use keys::{Keymap, Pending};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::Widget;

use crate::{Hint, Styles};

const MARGIN: u16 = 1;
const GAP: usize = 2;

/// The bottom row of a screen: the status on the left and the hints on the
/// right, or the hints alone from the left when there is no status. A notice
/// takes the status's place, and keys pressed so far take the notice's. Hints are in order of importance,
/// so a row too narrow for all of them drops them from the end, pinned ones
/// last, and cuts the status short before it drops a pinned one.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Footer {
    pub status: Line<'static>,
    pub notice: Option<String>,
    pub pending: String,
    pub hints: Vec<Hint>,
    pub styles: Styles,
}

impl Footer {
    pub fn of<C: Clone>(keymap: &Keymap<C>, pending: &Pending, holding: &[&str]) -> Self {
        let following = keymap.continuations(pending, holding);
        let hints = if following.is_empty() {
            keymap.bindings().iter().filter_map(|binding| Hint::of(binding, holding)).collect()
        } else {
            following.into_iter().map(Hint::from).collect()
        };

        Self { pending: pending.to_string(), hints, ..Self::default() }
    }

    pub fn picking<C: Clone>(
        keymap: &Keymap<C>,
        pending: &Pending,
        holding: &[&str],
        names: &[&str],
    ) -> Self {
        let following = keymap.continuations(pending, holding);
        let hints = if following.is_empty() {
            names
                .iter()
                .filter_map(|name| keymap.bindings().iter().find(|binding| binding.name == *name))
                .filter_map(|binding| Hint::of(binding, holding))
                .collect()
        } else {
            following.into_iter().map(Hint::from).collect()
        };

        Self { pending: pending.to_string(), hints, ..Self::default() }
    }

    fn left(&self) -> Line<'_> {
        if !self.pending.is_empty() {
            return Line::styled(self.pending.as_str(), self.styles.key);
        }
        if let Some(notice) = &self.notice {
            return Line::styled(notice.as_str(), self.styles.key);
        }
        let mut status = self.status.clone();
        status.style = self.styles.muted.patch(status.style);

        status
    }
}

impl Widget for &Footer {
    fn render(self, area: Rect, buffer: &mut Buffer) {
        if area.is_empty() {
            return;
        }
        buffer.set_style(area, self.styles.base);
        let room = usize::from(area.width.saturating_sub(2 * MARGIN));
        let left = self.left();
        let hints = fitted(&self.hints, left.width(), room);
        let width = spread(&hints);
        let x = area.x + MARGIN;

        let status_room = if hints.is_empty() { room } else { room.saturating_sub(width + GAP) };
        if status_room > 0 {
            buffer.set_line(x, area.y, &left, narrow(status_room));
        }

        let mut x = if left.width() == 0 { x } else { x + narrow(room - width) };
        for (index, hint) in hints.into_iter().enumerate() {
            if index > 0 {
                x += narrow(GAP);
            }
            let rest = usize::from(area.right().saturating_sub(x));
            if !hint.keys.is_empty() {
                (x, _) = buffer.set_stringn(x, area.y, &hint.keys, rest, self.styles.key);
                x += 1;
            }
            let rest = usize::from(area.right().saturating_sub(x));
            (x, _) = buffer.set_stringn(x, area.y, &hint.label, rest, self.styles.muted);
        }
    }
}

fn fitted(hints: &[Hint], status: usize, room: usize) -> Vec<&Hint> {
    let mut kept: Vec<&Hint> = hints.iter().collect();
    let needed =
        |kept: &[&Hint]| if status == 0 { spread(kept) } else { status + GAP + spread(kept) };
    while needed(&kept) > room {
        let Some(last) = kept.iter().rposition(|hint| !hint.pinned) else {
            break;
        };
        kept.remove(last);
    }
    while spread(&kept) > room {
        kept.pop();
    }

    kept
}

fn spread(hints: &[&Hint]) -> usize {
    let words: usize = hints.iter().map(|hint| wide(hint)).sum();

    words + GAP * hints.len().saturating_sub(1)
}

fn wide(hint: &Hint) -> usize {
    let keys = Span::raw(hint.keys.as_str()).width();
    let label = Span::raw(hint.label.as_str()).width();

    if keys == 0 { label } else { keys + 1 + label }
}

fn narrow(width: usize) -> u16 {
    u16::try_from(width).unwrap_or(u16::MAX)
}
