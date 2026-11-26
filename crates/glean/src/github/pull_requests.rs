//! A page of a repository's pull requests that match a filter, as a list shows them.

use serde::{Deserialize, Serialize};

use crate::github::{Client, Result, printable, timestamp};
use crate::model::{Branch, Filter, PullRequest, Repository, State};

const QUERY: &str = include_str!("pull_requests.graphql");

const PAGE: usize = 20;
const GHOST: &str = "ghost";

/// Where the next page of a listing starts. Only GitHub can make one.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Cursor(String);

/// One page of a listing, and where the next one starts if there is more.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Page {
    pub pull_requests: Vec<PullRequest>,
    pub next: Option<Cursor>,
}

#[derive(Serialize)]
struct Variables<'cursor> {
    owner: String,
    name: String,
    search: String,
    first: usize,
    after: Option<&'cursor str>,
}

#[derive(Deserialize)]
struct Data {
    search: Search,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Search {
    page_info: PageInfo,
    nodes: Vec<Node>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PageInfo {
    has_next_page: bool,
    end_cursor: Option<String>,
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
    id: String,
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
    comments: Count,
    labels: Nodes<Label>,
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

impl Client {
    #[expect(
        clippy::missing_errors_doc,
        reason = "the Error enum names every failure, from the connection to the answer's shape"
    )]
    pub fn pull_requests(
        &self,
        repository: &Repository,
        filter: Filter,
        after: Option<&Cursor>,
    ) -> Result<Page> {
        read(self.query(QUERY, variables(repository, filter, after))?)
    }
}

fn variables<'cursor>(
    repository: &Repository,
    filter: Filter,
    after: Option<&'cursor Cursor>,
) -> Variables<'cursor> {
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
        after: after.map(|cursor| cursor.0.as_str()),
    }
}

fn read(data: Data) -> Result<Page> {
    let pull_requests = data
        .search
        .nodes
        .into_iter()
        .filter_map(|node| match node {
            Node::PullRequest(found) => Some(found.into_model()),
            Node::Other => None,
        })
        .collect::<Result<_>>()?;
    let page_info = data.search.page_info;
    let next = page_info.end_cursor.filter(|_| page_info.has_next_page).map(Cursor);

    Ok(Page { pull_requests, next })
}

impl Found {
    fn into_model(self) -> Result<PullRequest> {
        let state = match (self.state.as_str(), self.is_draft) {
            ("MERGED", _) => State::Merged,
            ("CLOSED", _) => State::Closed,
            (_, true) => State::Draft,
            _ => State::Open,
        };

        Ok(PullRequest {
            id: self.id,
            number: self.number,
            title: printable(self.title),
            author: self.author.map_or_else(|| GHOST.to_owned(), |author| author.login),
            state,
            base: Branch { name: self.base_ref_name, commit: self.base_ref_oid },
            head: Branch { name: self.head_ref_name, commit: self.head_ref_oid },
            created: timestamp::parse(&self.created_at)?,
            updated: timestamp::parse(&self.updated_at)?,
            labels: self.labels.nodes.into_iter().map(|label| printable(label.name)).collect(),
            comments: self.comments.total_count,
            standing: None,
        })
    }
}
