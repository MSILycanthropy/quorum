//! The keys a settings file gives commands in place of the ones they came with.

use std::collections::BTreeMap;

use crate::{Error, Result, Sequence};

/// The `[keys]` table of a settings file: a command's name, and the keys that
/// replace the ones it came with. An empty list leaves it with none.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, serde::Deserialize, serde::Serialize)]
#[serde(transparent)]
pub struct Rebindings {
    pub commands: BTreeMap<String, Vec<Sequence>>,
}

impl Rebindings {
    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }

    pub fn unknown<'name>(&self, known: impl IntoIterator<Item = &'name str>) -> Result<()> {
        let known: Vec<&str> = known.into_iter().collect();
        match self.commands.keys().find(|name| !known.contains(&name.as_str())) {
            Some(name) => Err(Error::Unknown { name: name.clone() }),
            None => Ok(()),
        }
    }
}
