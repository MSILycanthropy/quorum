//! A repository's pull requests that match a filter, as a list shows them.

use serde::{Deserialize, Serialize};

use crate::github::{Client, Result, timestamp};
use crate::model::{
    Branch, CheckState, Filter, Mergeability, PullRequest, Repository, ReviewDecision, State,
};

const QUERY: &str = include_str!("pull_requests.graphql");

const PAGE: usize = 50;
const GHOST: &str = "ghost";

#[derive(Serialize)]
struct Variables {
    owner: String,
    name: String,
    search: String,
    first: usize,
}

#[derive(Deserialize)]
struct Data {
    search: Nodes<Node>,
}

#[derive(Deserialize)]
struct Nodes<Item> {
    nodes: Vec<Item>,
}

#[derive(Deserialize)]
#[serde(tag = "__typename")]
enum Node {
    PullRequest(Box<Found>),
    #[serde(other)]
    Other,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Found {
    number: u64,
    title: String,
    author: Option<Login>,
    state: String,
    is_draft: bool,
    base_ref_name: String,
    base_ref_oid: String,
    head_ref_name: String,
    head_ref_oid: String,
    created_at: String,
    updated_at: String,
    review_decision: Option<String>,
    mergeable: String,
    additions: usize,
    deletions: usize,
    changed_files: usize,
    comments: Count,
    labels: Nodes<Label>,
    review_requests: Nodes<ReviewRequest>,
    commits: Nodes<Pushed>,
}

#[derive(Deserialize)]
struct Login {
    login: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Count {
    total_count: usize,
}

#[derive(Deserialize)]
struct Label {
    name: String,
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
    #[expect(
        clippy::missing_errors_doc,
        reason = "the Error enum names every failure, from the connection to the answer's shape"
    )]
    pub fn pull_requests(
        &self,
        repository: &Repository,
        filter: Filter,
    ) -> Result<Vec<PullRequest>> {
        read(self.query(QUERY, variables(repository, filter))?)
    }
}

fn variables(repository: &Repository, filter: Filter) -> Variables {
    let qualifiers = match filter {
        Filter::Open => "is:open",
        Filter::Mine => "is:open author:@me",
        Filter::ReviewRequested => "is:open review-requested:@me",
        Filter::Drafts => "is:open draft:true",
        Filter::Closed => "is:closed",
    };

    Variables {
        owner: repository.owner.clone(),
        name: repository.name.clone(),
        search: format!("repo:{repository} is:pr {qualifiers} sort:updated-desc"),
        first: PAGE,
    }
}

fn read(data: Data) -> Result<Vec<PullRequest>> {
    data.search
        .nodes
        .into_iter()
        .filter_map(|node| match node {
            Node::PullRequest(found) => Some(found.into_model()),
            Node::Other => None,
        })
        .collect()
}

impl Found {
    fn into_model(self) -> Result<PullRequest> {
        let state = match (self.state.as_str(), self.is_draft) {
            ("MERGED", _) => State::Merged,
            ("CLOSED", _) => State::Closed,
            (_, true) => State::Draft,
            _ => State::Open,
        };
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

        Ok(PullRequest {
            number: self.number,
            title: printable(self.title),
            author: self.author.map_or_else(|| GHOST.to_owned(), |author| author.login),
            state,
            base: Branch { name: self.base_ref_name, commit: self.base_ref_oid },
            head: Branch { name: self.head_ref_name, commit: self.head_ref_oid },
            created: timestamp::parse(&self.created_at)?,
            updated: timestamp::parse(&self.updated_at)?,
            review_decision,
            checks,
            mergeability,
            labels: self.labels.nodes.into_iter().map(|label| printable(label.name)).collect(),
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
            comments: self.comments.total_count,
        })
    }
}

fn printable(text: String) -> String {
    if text.chars().any(char::is_control) {
        return text.chars().filter(|character| !character.is_control()).collect();
    }

    text
}
