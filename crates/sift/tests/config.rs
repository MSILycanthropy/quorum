use std::path::{Path, PathBuf};

use config::Settings;
use sift::config::Config;

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name)
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
