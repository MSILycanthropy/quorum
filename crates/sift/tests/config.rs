use std::path::{Path, PathBuf};

use clap::Parser;
use config::Settings;
use keys::Sequence;
use sift::config::Config;
use sift::ui::Presentation;

#[derive(Debug, Parser)]
struct Flags {
    #[command(flatten)]
    config: Config,
}

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name)
}

fn flagged(flags: &[&str]) -> Config {
    let arguments = std::iter::once("sift").chain(flags.iter().copied());

    Flags::try_parse_from(arguments).expect("the flags are sift's").config
}

#[test]
fn every_key_is_spelt_the_way_sift_names_it() {
    let settings =
        Settings::read::<Config>(&fixture("every-key.toml")).expect("every key is known");

    insta::assert_debug_snapshot!(settings.parse::<Config>().expect("every value is known"));
}

#[test]
fn no_keys_at_all_is_the_default() {
    assert_eq!(Settings::default().parse::<Config>().expect("no keys parse"), Config::default());
}

#[test]
fn a_layout_sift_does_not_have_is_refused_with_the_ones_it_does() {
    let error =
        Settings::read::<Config>(&fixture("split.toml")).expect_err("split is not a layout");

    insta::assert_snapshot!(error.to_string().replace(env!("CARGO_MANIFEST_DIR"), "[crate]"));
}

#[test]
fn a_flag_wins_over_its_key_and_leaves_the_other_keys_be() {
    let file = Settings::read::<Config>(&fixture("every-key.toml")).expect("every key is known");
    let flags = flagged(&["--layout", "side-by-side", "--wrap=false"]);
    let flags = Settings::serialized(&flags).expect("a config is a table");

    insta::assert_debug_snapshot!(file.over(flags).parse::<Config>().expect("both are configs"));
}

#[test]
fn a_bare_wrap_flag_turns_wrapping_on() {
    assert_eq!(flagged(&["--wrap"]).wrap, Some(true));
}

#[test]
fn a_wrap_flag_takes_its_value_only_after_an_equals_sign() {
    let config = flagged(&["--wrap", "--tab-width", "2"]);

    assert_eq!((config.wrap, config.tab_width), (Some(true), Some(2)));
}

#[test]
fn nothing_set_presents_the_defaults() {
    assert_eq!(Config::default().presentation(), Presentation::default());
}

#[test]
fn a_command_in_a_later_files_keys_replaces_its_keys_and_leaves_the_rest() {
    let under = Settings::read::<Config>(&fixture("keys-under.toml")).expect("the keys are known");
    let over = Settings::read::<Config>(&fixture("keys-over.toml")).expect("the keys are known");

    let keys = under.over(over).parse::<Config>().expect("both are configs").keys;

    assert_eq!(keys.commands["quit"], Sequence::all(&["ZZ"]).expect("ZZ is spelt right"));
    assert_eq!(keys.commands["reload"], Sequence::all(&["R"]).expect("R is spelt right"));
}
