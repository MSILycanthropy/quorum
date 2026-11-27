//! What a setting's value is, how it reads on the page, and what stepping or typing makes of it.

use config::Field;
use toml::{Table, Value};

/// How a setting is changed: turned on and off, stepped through a fixed set,
/// stepped or typed as a number, or typed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    Switch,
    Choice,
    Number,
    Text,
}

impl Kind {
    pub fn of(field: &Field, value: Option<&Value>) -> Self {
        let switch = field.choices == ["true", "false"] || matches!(value, Some(Value::Boolean(_)));
        if switch {
            Self::Switch
        } else if !field.choices.is_empty() {
            Self::Choice
        } else if matches!(value, Some(Value::Integer(_))) {
            Self::Number
        } else {
            Self::Text
        }
    }
}

pub fn at<'table>(table: &'table Table, path: &[&str], name: &str) -> Option<&'table Value> {
    let mut table = table;
    for key in path {
        table = table.get(*key)?.as_table()?;
    }

    table.get(name)
}

pub fn shown(value: Option<&Value>) -> String {
    match value {
        None => String::new(),
        Some(Value::Boolean(true)) => "on".to_owned(),
        Some(Value::Boolean(false)) => "off".to_owned(),
        Some(Value::String(text)) => text.clone(),
        Some(value) => value.to_string(),
    }
}

pub fn stepped(field: &Field, value: Option<&Value>, forward: bool) -> Option<Value> {
    match (Kind::of(field, value), value) {
        (Kind::Switch, value) => Some(Value::Boolean(!matches!(value, Some(Value::Boolean(true))))),
        (Kind::Choice, value) => {
            let count = field.choices.len();
            let now = value
                .and_then(Value::as_str)
                .and_then(|current| field.choices.iter().position(|choice| choice == current));
            let next = match (now, forward) {
                (None, _) => 0,
                (Some(index), true) => (index + 1) % count,
                (Some(index), false) => (index + count - 1) % count,
            };
            Some(Value::String(field.choices[next].clone()))
        }
        (Kind::Number, Some(Value::Integer(number))) => {
            let next = if forward { number.saturating_add(1) } else { number.saturating_sub(1) };
            Some(Value::Integer(next.max(0)))
        }
        (Kind::Number | Kind::Text, _) => None,
    }
}

pub fn typed(kind: Kind, text: &str) -> Result<Value, String> {
    match kind {
        Kind::Number => match text.trim().parse::<i64>() {
            Ok(number) if number >= 0 => Ok(Value::Integer(number)),
            _ => Err(format!("{} is not a number of zero or more", text.trim())),
        },
        Kind::Switch | Kind::Choice | Kind::Text => Ok(Value::String(text.to_owned())),
    }
}
