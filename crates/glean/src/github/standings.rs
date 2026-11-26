//! Where pull requests stand, asked of GitHub a few at a time, all at once.

use std::sync::mpsc::{Receiver, channel};
use std::thread;

use serde::{Deserialize, Serialize};

use crate::github::{Client, Result, printable};
use crate::model::{CheckState, Mergeability, ReviewDecision, Standing};

const QUERY: &str = include_str!("standings.graphql");

const GROUP: usize = 5;

#[derive(Serialize)]
struct Variables<'ids> {
    ids: &'ids [String],
}

#[derive(Deserialize)]
struct Data {
    nodes: Vec<Option<Found>>,
}

#[derive(Deserialize)]
struct Nodes<Item> {
    nodes: Vec<Item>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Found {
    id: String,
    review_decision: Option<String>,
    mergeable: String,
    additions: usize,
    deletions: usize,
    changed_files: usize,
    review_requests: Nodes<ReviewRequest>,
    commits: Nodes<Pushed>,
}

#[derive(Deserialize)]
struct Login {
    login: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReviewRequest {
    requested_reviewer: Option<Login>,
}

#[derive(Deserialize)]
struct Pushed {
    commit: Rollups,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Rollups {
    status_check_rollup: Option<Rollup>,
}

#[derive(Deserialize)]
struct Rollup {
    state: String,
}

impl Client {
    pub fn standings(&self, ids: &[String]) -> Receiver<Result<Vec<(String, Standing)>>> {
        let (sender, answers) = channel();
        for group in ids.chunks(GROUP) {
            let client = self.clone();
            let group = group.to_vec();
            let sender = sender.clone();
            thread::spawn(move || {
                let _ = sender.send(client.standing_of(&group));
            });
        }

        answers
    }

    fn standing_of(&self, ids: &[String]) -> Result<Vec<(String, Standing)>> {
        let data: Data = self.query(QUERY, Variables { ids })?;

        Ok(data
            .nodes
            .into_iter()
            .flatten()
            .map(|found| (found.id.clone(), found.into_model()))
            .collect())
    }
}

impl Found {
    fn into_model(self) -> Standing {
        let review_decision = match self.review_decision.as_deref() {
            Some("APPROVED") => Some(ReviewDecision::Approved),
            Some("CHANGES_REQUESTED") => Some(ReviewDecision::ChangesRequested),
            Some("REVIEW_REQUIRED") => Some(ReviewDecision::ReviewRequired),
            _ => None,
        };
        let mergeability = match self.mergeable.as_str() {
            "MERGEABLE" => Mergeability::Clean,
            "CONFLICTING" => Mergeability::Conflicting,
            _ => Mergeability::Unknown,
        };
        let checks = self
            .commits
            .nodes
            .into_iter()
            .next_back()
            .and_then(|pushed| pushed.commit.status_check_rollup)
            .map(|rollup| match rollup.state.as_str() {
                "SUCCESS" => CheckState::Passed,
                "FAILURE" | "ERROR" => CheckState::Failed,
                _ => CheckState::Pending,
            });

        Standing {
            review_decision,
            checks,
            mergeability,
            requested_reviewers: self
                .review_requests
                .nodes
                .into_iter()
                .filter_map(|request| request.requested_reviewer)
                .map(|reviewer| printable(reviewer.login))
                .collect(),
            added: self.additions,
            removed: self.deletions,
            changed_files: self.changed_files,
        }
    }
}
