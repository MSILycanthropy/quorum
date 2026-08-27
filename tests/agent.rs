//! That a stream of agent updates accumulates into a readable transcript.

use quorum::agent::{
    Call, Choice, Command, Consequence, Ending, Entry, Permission, Progress, Setting, State, Task,
    Transcript, Update, Usage,
};

fn call(id: &str, title: &str, progress: Progress) -> Update {
    Update::Ran {
        id: id.to_owned(),
        title: Some(title.to_owned()),
        progress: Some(progress),
        output: None,
    }
}

fn moved(id: &str, progress: Progress) -> Update {
    Update::Ran { id: id.to_owned(), title: None, progress: Some(progress), output: None }
}

fn permission(id: &str) -> Update {
    Update::Asked(Permission {
        id: id.to_owned(),
        title: "write src/main.rs".to_owned(),
        choices: vec![
            Choice { id: "once".to_owned(), name: "Allow once".to_owned() },
            Choice { id: "never".to_owned(), name: "Reject".to_owned() },
        ],
    })
}

fn applied(updates: Vec<Update>) -> Transcript {
    let mut transcript = Transcript::default();
    for update in updates {
        transcript.apply(update);
    }

    transcript
}

#[test]
fn consecutive_chunks_of_speech_accumulate_into_one_entry() {
    let transcript = applied(vec![
        Update::Said("the flake ".to_owned()),
        Update::Said("is the clock".to_owned()),
    ]);

    assert_eq!(transcript.entries(), [Entry::Said("the flake is the clock".to_owned())]);
}

#[test]
fn a_thought_between_two_chunks_of_speech_keeps_them_apart() {
    let transcript = applied(vec![
        Update::Said("reading".to_owned()),
        Update::Thought("this is a clock problem".to_owned()),
        Update::Said("rewriting".to_owned()),
    ]);

    assert_eq!(
        transcript.entries(),
        [
            Entry::Said("reading".to_owned()),
            Entry::Thought("this is a clock problem".to_owned()),
            Entry::Said("rewriting".to_owned()),
        ]
    );
}

#[test]
fn a_tool_call_is_updated_in_place_rather_than_repeated() {
    let transcript = applied(vec![
        call("one", "read src/git.rs", Progress::Pending),
        Update::Said("looking".to_owned()),
        moved("one", Progress::Done),
    ]);

    assert_eq!(
        transcript.entries(),
        [
            Entry::Ran(Call {
                id: "one".to_owned(),
                title: "read src/git.rs".to_owned(),
                progress: Progress::Done,
                output: Vec::new(),
            }),
            Entry::Said("looking".to_owned()),
        ]
    );
}

#[test]
fn a_finished_tool_call_asks_for_a_rescan_and_a_pending_one_does_not() {
    let mut transcript = Transcript::default();

    assert_eq!(transcript.apply(call("one", "edit", Progress::Pending)), Consequence::Nothing);
    assert_eq!(transcript.apply(moved("one", Progress::Done)), Consequence::Rescan);
}

#[test]
fn an_update_that_carries_only_a_status_keeps_the_title_it_was_given() {
    let transcript = applied(vec![
        call("one", "write ACORN.txt", Progress::Pending),
        moved("one", Progress::Failed),
    ]);

    assert_eq!(
        transcript.entries(),
        [Entry::Ran(Call {
            id: "one".to_owned(),
            title: "write ACORN.txt".to_owned(),
            progress: Progress::Failed,
            output: Vec::new(),
        })]
    );
}

#[test]
fn an_update_that_carries_only_a_title_keeps_the_progress_it_had() {
    let transcript = applied(vec![
        call("one", "preparing", Progress::Running),
        Update::Ran {
            id: "one".to_owned(),
            title: Some("write ACORN.txt".to_owned()),
            progress: None,
            output: None,
        },
    ]);

    assert_eq!(
        transcript.entries(),
        [Entry::Ran(Call {
            id: "one".to_owned(),
            title: "write ACORN.txt".to_owned(),
            progress: Progress::Running,
            output: Vec::new(),
        })]
    );
}

#[test]
fn the_end_of_a_turn_asks_for_a_rescan() {
    let mut transcript = Transcript::default();

    assert_eq!(transcript.apply(Update::Idle(Ending::Finished)), Consequence::Rescan);
    assert_eq!(transcript.state(), State::Idle);
}

#[test]
fn a_permission_request_blocks_until_it_is_answered() {
    let mut transcript = Transcript::default();
    transcript.prompted("fix the flake".to_owned());
    transcript.apply(permission("one"));

    assert_eq!(transcript.state(), State::Blocked);
    assert_eq!(transcript.pending().map(|pending| pending.id.as_str()), Some("one"));

    transcript.answered("one", "once");

    assert_eq!(transcript.state(), State::Working);
    assert_eq!(transcript.pending(), None);
    assert_eq!(transcript.entries().last(), Some(&Entry::Answered("Allow once".to_owned())));
}

#[test]
fn answering_a_permission_that_is_not_pending_changes_nothing() {
    let mut transcript = Transcript::default();
    transcript.apply(permission("one"));
    transcript.answered("two", "once");

    assert_eq!(transcript.state(), State::Blocked);
    assert_eq!(transcript.pending().map(|pending| pending.id.as_str()), Some("one"));
}

#[test]
fn the_agent_names_the_session_without_leaving_an_entry_behind() {
    let transcript = applied(vec![Update::Titled("fix the flaky auth test".to_owned())]);

    assert_eq!(transcript.title(), Some("fix the flaky auth test"));
    assert_eq!(transcript.entries(), []);
}

#[test]
fn output_arrives_whole_and_replaces_what_the_call_showed_before() {
    let mut transcript = Transcript::default();
    transcript.apply(Update::Ran {
        id: "one".to_owned(),
        title: Some("Run cargo test".to_owned()),
        progress: Some(Progress::Running),
        output: Some(vec!["running 3 tests".to_owned()]),
    });
    transcript.apply(Update::Ran {
        id: "one".to_owned(),
        title: None,
        progress: Some(Progress::Done),
        output: Some(vec!["running 3 tests".to_owned(), "test result: ok".to_owned()]),
    });

    assert_eq!(
        transcript.entries(),
        [Entry::Ran(Call {
            id: "one".to_owned(),
            title: "Run cargo test".to_owned(),
            progress: Progress::Done,
            output: vec!["running 3 tests".to_owned(), "test result: ok".to_owned()],
        })]
    );
}

#[test]
fn an_update_with_no_output_leaves_the_output_the_call_already_had() {
    let mut transcript = Transcript::default();
    transcript.apply(Update::Ran {
        id: "one".to_owned(),
        title: Some("Run cargo test".to_owned()),
        progress: Some(Progress::Running),
        output: Some(vec!["running 3 tests".to_owned()]),
    });
    transcript.apply(moved("one", Progress::Done));

    assert_eq!(
        transcript.entries(),
        [Entry::Ran(Call {
            id: "one".to_owned(),
            title: "Run cargo test".to_owned(),
            progress: Progress::Done,
            output: vec!["running 3 tests".to_owned()],
        })]
    );
}

#[test]
fn the_plan_is_a_snapshot_that_replaces_itself_rather_than_an_entry() {
    let mut transcript = Transcript::default();
    transcript.apply(Update::Planned(vec![
        Task { content: "read the fixture".to_owned(), progress: Progress::Done },
        Task { content: "rewrite the clock".to_owned(), progress: Progress::Running },
    ]));
    transcript.apply(Update::Planned(vec![Task {
        content: "rewrite the clock".to_owned(),
        progress: Progress::Done,
    }]));

    assert_eq!(transcript.entries(), []);
    assert_eq!(
        transcript.plan(),
        [Task { content: "rewrite the clock".to_owned(), progress: Progress::Done }]
    );
}

#[test]
fn mode_usage_commands_and_settings_are_the_latest_the_agent_reported() {
    let mut transcript = Transcript::default();
    transcript.apply(Update::Switched("Accept Edits".to_owned()));
    transcript.apply(Update::Used(Usage { used: 12_000, size: 200_000, cost: None }));
    transcript.apply(Update::Offered(vec![Command {
        name: "compact".to_owned(),
        description: "Compact the conversation".to_owned(),
    }]));
    transcript.apply(Update::Configured(vec![Setting {
        name: "Model".to_owned(),
        value: "Sonnet".to_owned(),
    }]));

    assert_eq!(transcript.mode(), Some("Accept Edits"));
    assert_eq!(transcript.usage().map(|usage| (usage.used, usage.size)), Some((12_000, 200_000)));
    assert_eq!(transcript.commands().len(), 1);
    assert_eq!(transcript.settings()[0].value, "Sonnet");
    assert_eq!(transcript.entries(), []);
}
