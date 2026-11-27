//! Which entries match what was typed, best first.

use crate::Entry;

pub fn ranked<T>(needle: &str, entries: &[Entry<T>]) -> Vec<usize> {
    let needle = lower(needle.trim());
    if needle.is_empty() {
        return (0..entries.len()).collect();
    }

    let mut ranked = entries
        .iter()
        .enumerate()
        .filter_map(|(order, entry)| Some((score(&needle, &lower(&entry.label))?, order)))
        .collect::<Vec<_>>();
    ranked.sort_unstable();

    ranked.into_iter().map(|(_, order)| order).collect()
}

fn lower(text: &str) -> Vec<char> {
    text.chars().flat_map(char::to_lowercase).collect()
}

fn score(needle: &[char], label: &[char]) -> Option<u8> {
    if needle == label {
        return Some(0);
    }
    if label.starts_with(needle) {
        return Some(1);
    }
    let name = label.rsplit(|character| *character == '/').next().unwrap_or(label);
    if name.starts_with(needle) {
        return Some(1);
    }
    if label.windows(needle.len()).any(|window| window == needle) {
        return Some(2);
    }

    let mut label = label.iter();
    for character in needle {
        if !label.any(|candidate| candidate == character) {
            return None;
        }
    }

    Some(3)
}
