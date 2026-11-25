//! The connection to GitHub's GraphQL endpoint, and the token it carries.

use std::fmt;
use std::process::Command;
use std::time::Duration;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::github::{Error, Result};

const ENDPOINT: &str = "https://api.github.com/graphql";
const TIMEOUT: Duration = Duration::from_secs(30);
const TOKEN_VARIABLES: [&str; 2] = ["GH_TOKEN", "GITHUB_TOKEN"];

/// A signed-in connection to one GitHub GraphQL endpoint. Every call blocks
/// until GitHub answers, so callers run it off the thread that draws.
#[derive(Clone)]
pub struct Client {
    agent: ureq::Agent,
    endpoint: String,
    token: String,
}

impl fmt::Debug for Client {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_struct("Client").field("endpoint", &self.endpoint).finish_non_exhaustive()
    }
}

#[derive(Serialize)]
struct Request<'query, Variables> {
    query: &'query str,
    variables: Variables,
}

#[derive(Deserialize)]
struct Answer {
    data: Option<serde_json::Value>,
    #[serde(default)]
    errors: Vec<Problem>,
}

#[derive(Deserialize)]
struct Problem {
    message: String,
    #[serde(rename = "type", default)]
    kind: Option<String>,
}

impl Client {
    pub fn at(endpoint: impl Into<String>, token: String) -> Self {
        let agent = ureq::Agent::config_builder()
            .timeout_global(Some(TIMEOUT))
            .http_status_as_error(false)
            .build()
            .into();

        Self { agent, endpoint: endpoint.into(), token }
    }

    #[expect(
        clippy::missing_errors_doc,
        reason = "the Error enum names the only failure, finding no token"
    )]
    pub fn signed_in() -> Result<Self> {
        Ok(Self::at(ENDPOINT, token()?))
    }

    pub(super) fn query<Variables: Serialize, Data: DeserializeOwned>(
        &self,
        query: &str,
        variables: Variables,
    ) -> Result<Data> {
        let mut response = self
            .agent
            .post(&self.endpoint)
            .header("Authorization", format!("bearer {}", self.token))
            .header("User-Agent", "glean")
            .send_json(Request { query, variables })?;
        let status = response.status().as_u16();
        let body = response.body_mut().read_to_string()?;

        match status {
            200 => {}
            401 => return Err(Error::Unauthorized),
            status => return Err(Error::Status { status, message: message(&body) }),
        }

        let answer: Answer = serde_json::from_str(&body)?;
        if !answer.errors.is_empty() {
            let missing =
                answer.errors.iter().any(|problem| problem.kind.as_deref() == Some("NOT_FOUND"));
            let messages: Vec<String> =
                answer.errors.into_iter().map(|problem| problem.message).collect();
            let messages = messages.join("; ");
            return Err(if missing { Error::NotFound(messages) } else { Error::Query(messages) });
        }

        let data =
            answer.data.ok_or_else(|| Error::Query("the answer carried no data".to_owned()))?;

        Ok(serde_json::from_value(data)?)
    }
}

fn message(body: &str) -> String {
    serde_json::from_str::<Problem>(body)
        .map_or_else(|_| body.trim().to_owned(), |problem| problem.message)
}

fn token() -> Result<String> {
    let set = TOKEN_VARIABLES
        .iter()
        .filter_map(|variable| std::env::var(variable).ok())
        .map(|token| token.trim().to_owned())
        .find(|token| !token.is_empty());
    if let Some(token) = set {
        return Ok(token);
    }

    let output = Command::new("gh").args(["auth", "token"]).output().map_err(|_| Error::NoToken)?;
    let token = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if !output.status.success() || token.is_empty() {
        return Err(Error::NoToken);
    }

    Ok(token)
}
