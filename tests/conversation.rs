//! That the transcript column reads top to bottom, with the newest entry last.

use std::fmt::Write;

use quorum::agent::{Ending, Permission, Progress, Task, Transcript, Update};
use quorum::color::Rgb;
use quorum::theme::TokyoNight;
use quorum::ui::Conversation;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::style::Color;

fn choices() -> Vec<quorum::agent::Choice> {
    vec![
        quorum::agent::Choice { id: "reject".to_owned(), name: "Deny".to_owned() },
        quorum::agent::Choice { id: "allow".to_owned(), name: "Allow Once".to_owned() },
    ]
}

fn ran(id: &str, title: &str, progress: Progress) -> Update {
    Update::Ran {
        id: id.to_owned(),
        title: Some(title.to_owned()),
        progress: Some(progress),
        output: None,
    }
}

fn shown(transcript: &Transcript, width: u16, height: u16) -> String {
    let conversation = Conversation::default();
    let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("test backend starts");

    terminal
        .draw(|frame| {
            let area = frame.area();
            conversation.draw(frame.buffer_mut(), area, transcript);
        })
        .expect("draw succeeds");

    describe(terminal.backend().buffer())
}

fn collapsed(transcript: &Transcript, width: u16) -> String {
    let conversation = Conversation::default();
    let mut terminal = Terminal::new(TestBackend::new(width, 1)).expect("test backend starts");

    terminal
        .draw(|frame| {
            let area = frame.area();
            conversation.status(frame.buffer_mut(), area, transcript);
        })
        .expect("draw succeeds");

    describe(terminal.backend().buffer())
}

fn describe(buffer: &Buffer) -> String {
    let area = buffer.area();
    let mut out = String::new();

    for y in 0..area.height {
        let mut text = String::new();
        let mut tints = String::new();
        for x in 0..area.width {
            let cell = &buffer[(x, y)];
            text.push_str(cell.symbol());
            tints.push(tint(cell.fg));
        }
        writeln!(out, "{text}\n{tints}").expect("writing to a String cannot fail");
    }

    out
}

fn tint(color: Color) -> char {
    let tints = TokyoNight::TINTS;
    let Color::Rgb(red, green, blue) = color else {
        return '?';
    };
    match Rgb::new(red, green, blue) {
        found if found == tints.foreground => 'F',
        found if found == tints.muted => 'm',
        found if found == tints.attention => '!',
        found if found == tints.gutter => 'g',
        _ => '?',
    }
}

#[test]
fn a_turn_reads_as_prompt_then_thought_then_speech() {
    let mut transcript = Transcript::default();
    transcript.prompted("fix the flaky auth test".to_owned());
    transcript.apply(Update::Thought("the clock is the problem".to_owned()));
    transcript.apply(Update::Said("Rewrote the fixture to use a fixed clock.".to_owned()));
    transcript.apply(Update::Idle(Ending::Finished));

    insta::assert_snapshot!(shown(&transcript, 44, 14));
}

#[test]
fn a_tool_call_is_one_line_marked_by_how_far_it_got() {
    let mut transcript = Transcript::default();
    transcript.apply(ran("one", "Read src/git.rs", Progress::Done));
    transcript.apply(ran("two", "Write src/git.rs", Progress::Running));
    transcript.apply(ran("three", "Run cargo test", Progress::Failed));

    insta::assert_snapshot!(shown(&transcript, 44, 10));
}

#[test]
fn a_permission_request_numbers_its_options_for_the_keys_that_answer_them() {
    let mut transcript = Transcript::default();
    transcript.apply(Update::Asked(Permission {
        id: "one".to_owned(),
        title: "Write ACORN.txt".to_owned(),
        choices: choices(),
    }));

    insta::assert_snapshot!(shown(&transcript, 44, 6));
}

#[test]
fn speech_longer_than_the_column_wraps_on_words() {
    let mut transcript = Transcript::default();
    transcript.apply(Update::Said(
        "The write was denied, so ACORN.txt was not created at all.".to_owned(),
    ));

    insta::assert_snapshot!(shown(&transcript, 30, 6));
}

#[test]
fn the_column_shows_its_newest_entries_when_it_runs_out_of_room() {
    let mut transcript = Transcript::default();
    for line in 1..=9 {
        transcript.apply(ran(&line.to_string(), &format!("Read file {line}"), Progress::Done));
    }

    insta::assert_snapshot!(shown(&transcript, 30, 6));
}

#[test]
fn a_collapsed_column_keeps_the_state_glyph_and_the_title() {
    let mut transcript = Transcript::default();
    transcript.apply(Update::Titled("fix the flaky auth test".to_owned()));
    transcript.apply(Update::Asked(Permission {
        id: "one".to_owned(),
        title: "Write ACORN.txt".to_owned(),
        choices: choices(),
    }));

    insta::assert_snapshot!(collapsed(&transcript, 44));
}

#[test]
fn a_tool_call_shows_what_it_printed_beneath_it() {
    let mut transcript = Transcript::default();
    transcript.apply(Update::Ran {
        id: "one".to_owned(),
        title: Some("Run cargo test".to_owned()),
        progress: Some(Progress::Done),
        output: Some(vec!["running 2 tests".to_owned(), "test result: ok. 2 passed".to_owned()]),
    });

    insta::assert_snapshot!(shown(&transcript, 44, 6));
}

#[test]
fn output_longer_than_the_column_allows_says_how_much_it_kept_back() {
    let mut transcript = Transcript::default();
    transcript.apply(Update::Ran {
        id: "one".to_owned(),
        title: Some("Run cargo test".to_owned()),
        progress: Some(Progress::Done),
        output: Some((1..=9).map(|line| format!("line {line}")).collect()),
    });

    insta::assert_snapshot!(shown(&transcript, 30, 10));
}

#[test]
fn the_checklist_marks_each_task_by_how_far_it_got() {
    let mut transcript = Transcript::default();
    transcript.apply(Update::Planned(vec![
        Task { content: "read the fixture".to_owned(), progress: Progress::Done },
        Task { content: "rewrite the clock".to_owned(), progress: Progress::Running },
        Task { content: "run the suite".to_owned(), progress: Progress::Pending },
    ]));

    let conversation = Conversation::default();
    let mut terminal = Terminal::new(TestBackend::new(44, 3)).expect("test backend starts");
    terminal
        .draw(|frame| {
            let area = frame.area();
            conversation.plan(frame.buffer_mut(), area, &transcript);
        })
        .expect("draw succeeds");

    insta::assert_snapshot!(describe(terminal.backend().buffer()));
}
