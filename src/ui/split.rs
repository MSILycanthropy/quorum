//! Where the three columns divide, and dragging a divider to move one.

use ratatui::layout::{Position, Rect};

/// The narrowest each column is allowed to become, in cells.
const NARROWEST: [u16; 3] = [24, 40, 32];

/// The tree, the diff and the conversation, left to right.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Columns {
    pub tree: Rect,
    pub diff: Rect,
    pub conversation: Rect,
}

/// Which pair of columns a divider stands between.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
enum Divider {
    #[default]
    Listing,
    Conversation,
}

/// How the width is shared between the columns, and which divider is held.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Split {
    shares: [u32; 3],
    widths: [u16; 3],
    showing: [bool; 3],
    area: Rect,
    held: Option<Divider>,
}

impl Default for Split {
    fn default() -> Self {
        Self { shares: [1, 1, 1], widths: [0; 3], showing: [true; 3], area: Rect::ZERO, held: None }
    }
}

impl Split {
    pub fn columns(&mut self, area: Rect, showing: [bool; 3]) -> Columns {
        self.area = area;
        self.showing = showing;
        self.widths = self.measure(area.width, showing);

        let tree = Rect::new(area.x, area.y, self.widths[0], area.height);
        let diff = Rect::new(tree.right(), area.y, self.widths[1], area.height);
        let conversation = Rect::new(diff.right(), area.y, self.widths[2], area.height);

        Columns { tree, diff, conversation }
    }

    pub fn grab(&mut self, at: Position) -> bool {
        self.held = self.area.contains(at).then(|| self.divider(at.x)).flatten();

        self.held.is_some()
    }

    pub fn held(&self) -> bool {
        self.held.is_some()
    }

    pub fn release(&mut self) {
        self.held = None;
    }

    pub fn drag(&mut self, at: Position) {
        let Some(held) = self.held else {
            return;
        };
        let total = self.area.width;
        let mut widths = self.widths;

        match held {
            Divider::Listing => {
                let room = total.saturating_sub(NARROWEST[1] + widths[2]).max(NARROWEST[0]);
                let wanted = at.x.saturating_sub(self.area.x).saturating_add(1);
                widths[0] = wanted.clamp(NARROWEST[0], room);
            }
            Divider::Conversation => {
                let room = total.saturating_sub(NARROWEST[1] + widths[0]).max(NARROWEST[2]);
                let wanted = self.area.right().saturating_sub(at.x);
                widths[2] = wanted.clamp(NARROWEST[2], room);
            }
        }
        widths[1] = total.saturating_sub(widths[0] + widths[2]);

        self.widths = widths;
        self.reshare();
    }

    pub fn even(&mut self) {
        self.shares = [1, 1, 1];
    }

    fn divider(&self, x: u16) -> Option<Divider> {
        if self.widths[1] == 0 {
            return None;
        }

        let listing = self.area.x + self.widths[0];
        if self.widths[0] > 0 && x.saturating_add(1) == listing {
            return Some(Divider::Listing);
        }
        if self.widths[2] > 0 && x == listing + self.widths[1] {
            return Some(Divider::Conversation);
        }

        None
    }

    fn measure(&self, total: u16, showing: [bool; 3]) -> [u16; 3] {
        let weight: u32 =
            (0..3).filter(|&column| showing[column]).map(|column| self.shares[column]).sum();
        if weight == 0 {
            return [0; 3];
        }

        let mut widths = [0; 3];
        for column in 0..3 {
            if !showing[column] {
                continue;
            }
            let share = u32::from(total) * self.shares[column] / weight;
            widths[column] = u16::try_from(share).unwrap_or(total).max(NARROWEST[column]);
        }

        settle(widths, showing, total)
    }

    fn reshare(&mut self) {
        let weight: u32 =
            (0..3).filter(|&column| self.showing[column]).map(|column| self.shares[column]).sum();
        let filled: u32 = (0..3)
            .filter(|&column| self.showing[column])
            .map(|column| u32::from(self.widths[column]))
            .sum();
        if weight == 0 || filled == 0 {
            return;
        }

        for column in 0..3 {
            self.shares[column] = if self.showing[column] {
                u32::from(self.widths[column])
            } else {
                self.shares[column] * filled / weight
            };
        }
    }
}

fn settle(mut widths: [u16; 3], showing: [bool; 3], total: u16) -> [u16; 3] {
    while taken(widths) > u32::from(total) {
        let Some(column) = (0..3)
            .filter(|&column| showing[column] && widths[column] > 1)
            .max_by_key(|&column| widths[column].saturating_sub(NARROWEST[column]))
        else {
            break;
        };
        widths[column] -= 1;
    }

    let spare = u16::try_from(u32::from(total).saturating_sub(taken(widths))).unwrap_or(0);
    if let Some(column) = [1, 0, 2].into_iter().find(|&column| showing[column]) {
        widths[column] = widths[column].saturating_add(spare);
    }

    widths
}

fn taken(widths: [u16; 3]) -> u32 {
    widths.iter().map(|width| u32::from(*width)).sum()
}
