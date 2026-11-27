//! That a program's settings are listed from the clap definitions that are also its flags.

use config::{Field, fields};

#[derive(Clone, Copy, Debug, clap::ValueEnum)]
enum Layout {
    SideBySide,
    Unified,
}

#[derive(Debug, clap::Args)]
struct Example {
    #[arg(long, value_enum, help = "Side by side, or one column")]
    layout: Option<Layout>,
    #[arg(long, num_args = 0..=1, default_missing_value = "true", help = "Wrap long lines")]
    wrap: Option<bool>,
    #[arg(long, help = "Columns a tab expands to")]
    tab_width: Option<usize>,
    #[arg(long, hide = true)]
    secret: Option<String>,
}

#[test]
fn every_flag_is_a_field_with_its_help_and_its_choices() {
    assert_eq!(
        fields::<Example>(),
        vec![
            Field {
                name: "layout".to_owned(),
                help: "Side by side, or one column".to_owned(),
                choices: vec!["side-by-side".to_owned(), "unified".to_owned()],
            },
            Field {
                name: "wrap".to_owned(),
                help: "Wrap long lines".to_owned(),
                choices: vec!["true".to_owned(), "false".to_owned()],
            },
            Field {
                name: "tab-width".to_owned(),
                help: "Columns a tab expands to".to_owned(),
                choices: Vec::new(),
            },
        ]
    );
}
