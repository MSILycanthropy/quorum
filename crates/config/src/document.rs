//! A settings file edited in place, so the comments and order a person wrote survive.

use std::path::{Path, PathBuf};

use toml_edit::{Array, DocumentMut, InlineTable, Item, TableLike};

use crate::{Error, Result, directory};

/// One settings file, open for editing. A file that is not there yet is an
/// empty one, written on the first save.
#[derive(Clone, Debug)]
pub struct Document {
    path: PathBuf,
    document: DocumentMut,
}

impl Document {
    pub fn open(application: &str) -> Result<Self> {
        let directory =
            directory(application).ok_or_else(|| Error::Nowhere(application.to_owned()))?;

        Self::at(&directory.join("config.toml"))
    }

    pub fn at(path: &Path) -> Result<Self> {
        let text = match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
            Err(source) => return Err(Error::Read { path: path.to_path_buf(), source }),
        };
        let document =
            text.parse().map_err(|source| Error::Edit { path: path.to_path_buf(), source })?;

        Ok(Self { path: path.to_path_buf(), document })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn set(&mut self, path: &[&str], value: &toml::Value) {
        let Some((last, tables)) = path.split_last() else {
            return;
        };
        let mut table = self.document.as_table_mut() as &mut dyn TableLike;
        for key in tables {
            if !table.get(key).is_some_and(Item::is_table_like) {
                table.insert(key, toml_edit::table());
            }
            let Some(inner) = table.get_mut(key).and_then(Item::as_table_like_mut) else {
                return;
            };
            table = inner;
        }
        let mut value = edited(value);
        if let Some(old) = table.get(last).and_then(Item::as_value) {
            *value.decor_mut() = old.decor().clone();
        }
        table.insert(last, Item::Value(value));
    }

    pub fn remove(&mut self, path: &[&str]) {
        let Some((last, tables)) = path.split_last() else {
            return;
        };
        let mut table = self.document.as_table_mut() as &mut dyn TableLike;
        for key in tables {
            let Some(inner) = table.get_mut(key).and_then(Item::as_table_like_mut) else {
                return;
            };
            table = inner;
        }
        table.remove(last);
    }

    pub fn save(&self) -> Result<()> {
        let write = |source| Error::Write { path: self.path.clone(), source };
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent).map_err(write)?;
        }

        std::fs::write(&self.path, self.document.to_string()).map_err(write)
    }
}

impl std::fmt::Display for Document {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}", self.document)
    }
}

fn edited(value: &toml::Value) -> toml_edit::Value {
    match value {
        toml::Value::String(text) => text.as_str().into(),
        toml::Value::Integer(number) => (*number).into(),
        toml::Value::Float(number) => (*number).into(),
        toml::Value::Boolean(on) => (*on).into(),
        toml::Value::Datetime(when) => when.to_string().into(),
        toml::Value::Array(values) => {
            toml_edit::Value::Array(values.iter().map(edited).collect::<Array>())
        }
        toml::Value::Table(table) => toml_edit::Value::InlineTable(
            table.iter().map(|(key, value)| (key.as_str(), edited(value))).collect::<InlineTable>(),
        ),
    }
}
