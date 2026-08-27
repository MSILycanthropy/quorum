//! The changed files as a directory tree, flattened into the rows that draw it.

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};

use crate::git::FileChange;

/// One drawn line: a directory, or a file and the change it stands for.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Row {
    pub depth: usize,
    pub label: String,
    pub path: PathBuf,
    pub change: Option<usize>,
    pub open: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Tree {
    root: Node,
    shut: HashSet<PathBuf>,
    rows: Vec<Row>,
}

impl Tree {
    pub fn of(changes: &[FileChange]) -> Self {
        let mut root = Node::default();
        for (index, change) in changes.iter().enumerate() {
            root.sow(&change.path, index);
        }

        let mut tree = Self { root, shut: HashSet::new(), rows: Vec::new() };
        tree.spread();

        tree
    }

    pub fn rows(&self) -> &[Row] {
        &self.rows
    }

    pub fn toggle(&mut self, index: usize) {
        let Some(row) = self.rows.get(index).filter(|row| row.change.is_none()) else {
            return;
        };

        if row.open {
            self.shut.insert(row.path.clone());
        } else {
            self.shut.remove(&row.path);
        }
        self.spread();
    }

    pub fn row_of(&self, path: &Path) -> Option<usize> {
        self.rows.iter().position(|row| row.path == path)
    }

    fn spread(&mut self) {
        let mut rows = Vec::new();
        spread(&self.root, Path::new(""), 0, &self.shut, &mut rows);
        self.rows = rows;
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Node {
    children: BTreeMap<String, Node>,
    change: Option<usize>,
}

impl Node {
    fn sow(&mut self, path: &Path, change: usize) {
        let mut node = self;
        let mut components = path.components().peekable();

        while let Some(component) = components.next() {
            let name = component.as_os_str().to_string_lossy().into_owned();
            node = node.children.entry(name).or_default();
            if components.peek().is_none() {
                node.change = Some(change);
            }
        }
    }

    fn directory(&self) -> bool {
        self.change.is_none()
    }
}

fn spread(node: &Node, at: &Path, depth: usize, shut: &HashSet<PathBuf>, rows: &mut Vec<Row>) {
    let (folders, files): (Vec<_>, Vec<_>) =
        node.children.iter().partition(|(_, child)| child.directory());

    for (name, child) in folders {
        let (label, path, deepest) = compact(name.clone(), at.join(name), child);
        let open = !shut.contains(&path);
        rows.push(Row { depth, label, path: path.clone(), change: None, open });

        if open {
            spread(deepest, &path, depth + 1, shut, rows);
        }
    }

    for (name, child) in files {
        rows.push(Row {
            depth,
            label: name.clone(),
            path: at.join(name),
            change: child.change,
            open: false,
        });
    }
}

fn compact(mut label: String, mut path: PathBuf, mut node: &Node) -> (String, PathBuf, &Node) {
    while node.children.len() == 1 {
        let Some((only, inner)) = node.children.iter().next() else {
            break;
        };
        if !inner.directory() {
            break;
        }

        label = format!("{label}/{only}");
        path = path.join(only);
        node = inner;
    }

    (label, path, node)
}
