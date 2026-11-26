use std::path::{Path, PathBuf};

use config::Settings;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct Example {
    name: String,
    depth: usize,
    nested: Nested,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct Nested {
    colour: String,
    size: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct Host {
    title: String,
    guest: Example,
}

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name)
}

fn read<T: serde::de::DeserializeOwned>(name: &str) -> Settings {
    Settings::read::<T>(&fixture(name)).expect("the fixture is well formed")
}

fn refusal<T: serde::de::DeserializeOwned>(name: &str) -> String {
    let error = Settings::read::<T>(&fixture(name)).expect_err("the fixture is malformed");
    error.to_string().replace(env!("CARGO_MANIFEST_DIR"), "[crate]")
}

#[test]
fn a_missing_file_reads_as_every_default() {
    let settings = read::<Example>("absent.toml");

    assert_eq!(settings.parse::<Example>().expect("no keys parse"), Example::default());
}

#[test]
fn a_later_file_wins_key_by_key_and_keeps_the_keys_it_leaves_out() {
    let settings = read::<Example>("base.toml").over(read::<Example>("top.toml"));

    insta::assert_debug_snapshot!(settings.parse::<Example>().expect("both files are examples"));
}

#[test]
fn a_misspelt_key_is_refused_with_its_file_and_line() {
    insta::assert_snapshot!(refusal::<Example>("misspelt.toml"));
}

#[test]
fn a_misspelt_key_in_a_section_is_refused_by_the_file_that_holds_it() {
    insta::assert_snapshot!(refusal::<Host>("guest-misspelt.toml"));
}

#[test]
fn a_section_of_one_file_layers_over_a_file_of_its_own() {
    let host = read::<Host>("host.toml");
    let guest = read::<Example>("base.toml").over(host.section("guest"));

    insta::assert_debug_snapshot!(guest.parse::<Example>().expect("both are examples"));
}

#[test]
fn a_section_that_is_absent_reads_as_no_keys() {
    let section = read::<Example>("base.toml").section("guest");

    assert_eq!(section, Settings::default());
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize)]
struct Given {
    name: Option<String>,
    depth: Option<usize>,
}

#[test]
fn a_value_layers_over_a_file_with_only_the_fields_it_holds() {
    let given = Given { name: None, depth: Some(7) };
    let settings = read::<Example>("base.toml")
        .over(Settings::serialized(&given).expect("a struct is a table"));

    insta::assert_debug_snapshot!(
        settings.parse::<Example>().expect("given is part of an example")
    );
}
