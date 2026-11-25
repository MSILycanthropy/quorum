//! That a run of CI jobs adds up to the state of its most urgent job.

use glean::model::{Check, CheckState};

fn run(states: &[CheckState]) -> Option<CheckState> {
    let checks: Vec<Check> =
        states.iter().map(|state| Check { state: *state, ..Check::default() }).collect();
    Check::rollup(&checks)
}

#[test]
fn no_jobs_add_up_to_no_state() {
    assert_eq!(run(&[]), None);
}

#[test]
fn one_failure_fails_the_whole_run() {
    use CheckState::{Failed, Passed, Running};
    assert_eq!(run(&[Passed, Running, Failed, Passed]), Some(Failed));
}

#[test]
fn a_run_still_going_is_running_until_something_fails() {
    use CheckState::{Passed, Pending, Running, Skipped};
    assert_eq!(run(&[Passed, Pending, Skipped]), Some(Pending));
    assert_eq!(run(&[Passed, Pending, Running]), Some(Running));
}

#[test]
fn a_cancelled_job_outranks_one_still_running_but_not_a_failure() {
    use CheckState::{Cancelled, Failed, Running};
    assert_eq!(run(&[Running, Cancelled]), Some(Cancelled));
    assert_eq!(run(&[Cancelled, Failed]), Some(Failed));
}

#[test]
fn skipped_jobs_do_not_hold_back_a_passing_run() {
    use CheckState::{Passed, Skipped};
    assert_eq!(run(&[Skipped, Passed, Skipped]), Some(Passed));
}

#[test]
fn a_run_of_only_skipped_jobs_is_skipped() {
    use CheckState::Skipped;
    assert_eq!(run(&[Skipped, Skipped]), Some(Skipped));
}
