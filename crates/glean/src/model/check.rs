//! What CI says about a pull request, job by job and as a whole.

/// Where a CI job stands, ordered from least to most urgent, so that a whole
/// run stands wherever its most urgent job does.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum CheckState {
    Skipped,
    Passed,
    #[default]
    Pending,
    Running,
    Cancelled,
    Failed,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Check {
    pub name: String,
    pub state: CheckState,
    pub url: Option<String>,
}

impl Check {
    pub fn rollup(checks: &[Self]) -> Option<CheckState> {
        checks.iter().map(|check| check.state).max()
    }
}
