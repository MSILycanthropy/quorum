//! The settings a program has, read from the clap definitions that are also their flags.

/// One setting as a settings page lists it: the key it is written under, what
/// it does, and the values it can take when they are a fixed set.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Field {
    pub name: String,
    pub help: String,
    pub choices: Vec<String>,
}

pub fn fields<T: clap::Args>() -> Vec<Field> {
    let command = T::augment_args(clap::Command::new("settings"));

    command
        .get_arguments()
        .filter(|argument| !argument.is_hide_set())
        .filter_map(|argument| {
            let name = argument.get_long()?.to_owned();
            let help = argument.get_help().map(ToString::to_string).unwrap_or_default();
            let choices = argument
                .get_possible_values()
                .iter()
                .filter(|choice| !choice.is_hide_set())
                .map(|choice| choice.get_name().to_owned())
                .collect();

            Some(Field { name, help, choices })
        })
        .collect()
}
