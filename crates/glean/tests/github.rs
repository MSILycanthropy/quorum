//! That Glean asks GitHub for the right pull requests, and reads its answers into the model.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::thread::{self, JoinHandle};
use std::time::{Duration, SystemTime};

use glean::github::{Client, Error};
use glean::model::{Filter, PullRequest, Repository};

const LISTED: &str = include_str!("fixtures/pull_requests.json");

/// What a fake GitHub saw of the one request it answered.
struct Asked {
    headers: Vec<String>,
    body: serde_json::Value,
}

fn answering(status: u16, answer: impl Into<String>) -> (Client, JoinHandle<Asked>) {
    let answer = answer.into();
    let listener = TcpListener::bind("127.0.0.1:0").expect("a local port is free");
    let address = listener.local_addr().expect("a bound listener has an address");
    let client = Client::at(format!("http://{address}/graphql"), "secret".to_owned());

    let server = thread::spawn(move || {
        let (stream, _) = listener.accept().expect("the client connects");
        let mut reader = BufReader::new(stream);
        let mut headers = Vec::new();
        loop {
            let mut line = String::new();
            reader.read_line(&mut line).expect("the request reads");
            let line = line.trim_end().to_owned();
            if line.is_empty() {
                break;
            }
            headers.push(line);
        }
        let length = headers
            .iter()
            .find_map(|header| {
                let (name, value) = header.split_once(':')?;
                name.eq_ignore_ascii_case("content-length").then(|| value.trim().parse().ok())?
            })
            .expect("the request says how long its body is");
        let mut received = vec![0; length];
        reader.read_exact(&mut received).expect("the body reads");

        let response = format!(
            "HTTP/1.1 {status} Answer\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{answer}",
            answer.len()
        );
        reader.get_mut().write_all(response.as_bytes()).expect("the answer writes");

        Asked { headers, body: serde_json::from_slice(&received).expect("the request is JSON") }
    });

    (client, server)
}

fn sift() -> Repository {
    Repository { owner: "MSILycanthropy".to_owned(), name: "sift".to_owned() }
}

fn asked(filter: Filter) -> Asked {
    let (client, server) = answering(200, LISTED);
    client.pull_requests(&sift(), filter).expect("the listing reads");
    server.join().expect("the server answers")
}

fn listed(filter: Filter) -> Vec<PullRequest> {
    let (client, _server) = answering(200, LISTED);
    client.pull_requests(&sift(), filter).expect("the listing reads")
}

fn failure(status: u16, answer: impl Into<String>) -> Error {
    let (client, _server) = answering(status, answer);
    client.pull_requests(&sift(), Filter::Open).expect_err("the listing fails")
}

#[test]
fn a_listing_asks_for_the_repository_with_the_token() {
    let asked = asked(Filter::Open);
    let query = asked.body["query"].as_str().unwrap_or_default();

    assert!(
        asked
            .headers
            .iter()
            .any(|header| header.eq_ignore_ascii_case("authorization: bearer secret"))
    );
    assert_eq!(asked.body["variables"]["owner"], "MSILycanthropy");
    assert_eq!(asked.body["variables"]["name"], "sift");
    assert_eq!(asked.body["variables"]["first"], 50);
    assert!(query.contains("repository(") && query.contains("search("));
}

#[test]
fn each_filter_searches_for_its_own_pull_requests() {
    let cases = [
        (Filter::Open, "is:open"),
        (Filter::Mine, "is:open author:@me"),
        (Filter::ReviewRequested, "is:open review-requested:@me"),
        (Filter::Drafts, "is:open draft:true"),
        (Filter::Closed, "is:closed"),
    ];
    for (filter, qualifiers) in cases {
        let wanted = format!("repo:MSILycanthropy/sift is:pr {qualifiers} sort:updated-desc");
        assert_eq!(asked(filter).body["variables"]["search"], wanted.as_str(), "{filter:?}");
    }
}

#[test]
fn a_listing_reads_open_draft_and_merged_pull_requests_into_the_model_in_order() {
    insta::assert_debug_snapshot!(listed(Filter::Open));
}

#[test]
fn a_refused_token_asks_to_sign_in_again() {
    let refused = failure(401, r#"{"message":"Bad credentials"}"#);
    assert!(matches!(refused, Error::Unauthorized));
}

#[test]
fn any_other_status_carries_githubs_message() {
    let failed = failure(502, r#"{"message":"Server Error"}"#);
    assert!(matches!(failed, Error::Status { status: 502, message } if message == "Server Error"));
}

#[test]
fn a_repository_github_cannot_find_is_an_error_rather_than_an_empty_list() {
    let failed = failure(
        200,
        r#"{"data":{"repository":null,"search":{"nodes":[]}},"errors":[{"type":"NOT_FOUND","message":"Could not resolve to a Repository"}]}"#,
    );
    assert!(
        matches!(failed, Error::NotFound(message) if message == "Could not resolve to a Repository")
    );
}

#[test]
fn a_graphql_error_without_data_carries_its_message() {
    let failed = failure(200, r#"{"data":null,"errors":[{"message":"Something went wrong"}]}"#);
    assert!(matches!(failed, Error::Query(message) if message == "Something went wrong"));
}

#[test]
fn a_timestamp_that_names_no_real_moment_is_refused() {
    for broken in [
        "2026-13-01T00:00:00Z",
        "2026-02-29T00:00:00Z",
        "2026-04-31T00:00:00Z",
        "1900-02-29T00:00:00Z",
        "2026-11-25T24:00:00Z",
        "2026-11-25 10:30:00Z",
    ] {
        let failed = failure(200, LISTED.replacen("2024-02-29T23:59:59Z", broken, 1));
        assert!(matches!(failed, Error::Timestamp(ref text) if text == broken), "{broken}");
    }
}

#[test]
fn a_leap_day_in_a_leap_year_reads() {
    let leap = LISTED.replacen("2024-02-29T23:59:59Z", "2000-02-29T12:34:56Z", 1);
    let (client, _server) = answering(200, leap);
    let listed = client.pull_requests(&sift(), Filter::Open).expect("2000 is a leap year");
    assert_eq!(listed[1].created, SystemTime::UNIX_EPOCH + Duration::from_secs(951_827_696));
}

#[test]
fn control_characters_in_githubs_text_never_reach_the_terminal() {
    let hostile = LISTED
        .replacen(
            "feat(ui): annotations under a line of the diff",
            r"feat\u001b]0;pwned\u0007(ui)\u001b[2J",
            1,
        )
        .replacen(r#""name": "ui""#, r#""name": "u\u009bi""#, 1)
        .replacen(r#""login": "reviewers""#, r#""login": "review\u0000ers""#, 1);
    let (client, _server) = answering(200, hostile);
    let listed = client.pull_requests(&sift(), Filter::Open).expect("the listing reads");

    assert_eq!(listed[0].title, "feat]0;pwned(ui)[2J");
    assert_eq!(listed[0].labels, ["ui", "feature"]);
    assert_eq!(listed[0].requested_reviewers, ["hubot", "reviewers"]);
}

#[test]
fn a_result_github_could_not_resolve_fails_with_githubs_reason_not_a_shape_error() {
    let unresolved = LISTED.replacen(
        r#""data": {"#,
        r#""errors": [{ "type": "FORBIDDEN", "message": "Resource not accessible by integration" }],
  "data": {"#,
        1,
    );
    let unresolved = unresolved.replacen(
        r#"{
          "__typename": "Issue"
        }"#,
        "null",
        1,
    );
    let failed = failure(200, unresolved);
    assert!(
        matches!(failed, Error::Query(message) if message == "Resource not accessible by integration")
    );
}

#[test]
fn a_null_result_with_no_reason_fails_on_its_shape() {
    let unexplained = LISTED.replacen(
        r#"{
          "__typename": "Issue"
        }"#,
        "null",
        1,
    );
    assert!(matches!(failure(200, unexplained), Error::Decode(_)));
}
