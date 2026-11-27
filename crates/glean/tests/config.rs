//! That Glean's settings file holds its own settings, Sift's under `[diff]`, and every key.

use std::fmt::Write;
use std::path::{Path, PathBuf};

use config::Settings;
use glean::config::{Config, Listing, sections};
use keys::Sequence;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use settings::{Page, Styles, Values};
use sift::command::Catalog;
use sift::ui::Mode;

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name)
}

#[test]
fn the_file_holds_gleans_settings_sifts_under_diff_and_every_key() {
    let config: Config = Settings::read::<Config>(&fixture("glean.toml"))
        .expect("the fixture reads")
        .parse()
        .expect("every key is known");

    assert_eq!((config.filter, config.refresh), (Some(Listing::Mine), Some(60)));
    assert_eq!(config.diff.presentation().mode, Mode::Unified);
    assert_eq!(
        config.keys.commands["next-hunk"],
        Sequence::all(&["]c", "F7"]).expect("the keys read")
    );
}

#[test]
fn a_misspelt_diff_setting_is_refused_with_its_line() {
    let error =
        Settings::read::<Config>(&fixture("misspelt-diff.toml")).expect_err("lyout is no setting");

    insta::assert_snapshot!(error.to_string().replace(env!("CARGO_MANIFEST_DIR"), "[crate]"));
}

#[test]
fn the_settings_page_lists_gleans_settings_then_sifts() {
    let effective = Config {
        filter: Some(Listing::Open),
        refresh: Some(60),
        diff: sift::config::Config::default().resolved(),
        ..Config::default()
    };
    let values = Values {
        effective: Settings::serialized(&effective).expect("a config is a table").table().clone(),
        ..Values::default()
    };
    let keymap = Catalog::default().keymap().clone();
    let mut page =
        Page::new(sections(), values, keymap, keys::Rebindings::default(), Styles::default());

    let mut terminal = Terminal::new(TestBackend::new(110, 24)).expect("test backend starts");
    terminal.draw(|frame| frame.render_widget(&mut page, frame.area())).expect("draw succeeds");
    let buffer = terminal.backend().buffer();
    let mut screen = String::new();
    for y in 0..buffer.area().height {
        let line: String =
            (0..buffer.area().width).map(|x| buffer[(x, y)].symbol().to_owned()).collect();
        writeln!(screen, "{}", line.trim_end()).expect("writing to a String cannot fail");
    }

    insta::assert_snapshot!(screen);
}
