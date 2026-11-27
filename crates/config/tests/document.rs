//! That a settings file is edited in place, keeping what a person wrote around the change.

use std::path::PathBuf;

use config::Document;

fn written(name: &str, text: &str) -> PathBuf {
    let directory = std::env::temp_dir().join(format!("config-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&directory).expect("the temporary directory is writable");
    let path = directory.join("config.toml");
    std::fs::write(&path, text).expect("the temporary file is writable");

    path
}

const WRITTEN: &str = "# my settings\n[diff]\nlayout = \"unified\" # I like one column\nwrap = true\n\n[keys]\nquit = [\"q\"]\n";

#[test]
fn setting_a_value_keeps_the_comments_and_the_order_around_it() {
    let mut document = Document::at(&written("set", WRITTEN)).expect("the file reads");

    document.set(&["diff", "wrap"], &toml::Value::Boolean(false));

    insta::assert_snapshot!(document.to_string());
}

#[test]
fn setting_a_value_under_a_table_that_is_not_there_adds_the_table() {
    let mut document = Document::at(&written("add", "# nothing yet\n")).expect("the file reads");

    document.set(&["diff", "context"], &toml::Value::Integer(5));
    document.set(
        &["keys", "next-hunk"],
        &toml::Value::Array(vec![
            toml::Value::String("]c".to_owned()),
            toml::Value::String("F7".to_owned()),
        ]),
    );

    insta::assert_snapshot!(document.to_string());
}

#[test]
fn removing_a_value_leaves_the_rest_of_its_table() {
    let mut document = Document::at(&written("remove", WRITTEN)).expect("the file reads");

    document.remove(&["diff", "layout"]);
    document.remove(&["diff", "absent"]);
    document.remove(&["absent", "layout"]);

    insta::assert_snapshot!(document.to_string());
}

#[test]
fn a_saved_file_reads_back_as_it_was_written() {
    let path = written("saved", WRITTEN);
    let mut document = Document::at(&path).expect("the file reads");
    document.set(&["diff", "tab-width"], &toml::Value::Integer(2));
    document.save().expect("the file saves");

    let reread = Document::at(&path).expect("the saved file reads");

    assert_eq!(reread.to_string(), document.to_string());
}

#[test]
fn a_file_that_is_not_there_is_an_empty_one_and_saving_makes_it() {
    let directory = std::env::temp_dir().join(format!("config-{}-missing", std::process::id()));
    let path = directory.join("nested").join("config.toml");
    let _ = std::fs::remove_dir_all(&directory);
    let mut document = Document::at(&path).expect("a missing file is an empty one");

    document.set(&["diff", "wrap"], &toml::Value::Boolean(true));
    document.save().expect("saving makes the file and its directory");

    assert_eq!(std::fs::read_to_string(&path).expect("the file was made"), "[diff]\nwrap = true\n");
}
