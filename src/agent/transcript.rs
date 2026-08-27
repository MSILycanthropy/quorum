//! What the agent has said and done, accumulated from its updates.

/// How far a tool call got.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Progress {
    #[default]
    Pending,
    Running,
    Done,
    Failed,
}

/// Why the agent stopped talking.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Ending {
    #[default]
    Finished,
    Cancelled,
    Refused,
    MaxTokens,
    MaxRequests,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Call {
    pub id: String,
    pub title: String,
    pub progress: Progress,
    pub output: Vec<String>,
}

/// One item on the checklist the agent maintains for itself.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Task {
    pub content: String,
    pub progress: Progress,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Cost {
    pub amount: f64,
    pub currency: String,
}

/// How much of the agent's context window the session has spent.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Usage {
    pub used: u64,
    pub size: u64,
    pub cost: Option<Cost>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Command {
    pub name: String,
    pub description: String,
}

/// One of the agent's settings, flattened to what it currently reads as.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Setting {
    pub name: String,
    pub value: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Choice {
    pub id: String,
    pub name: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Permission {
    pub id: String,
    pub title: String,
    pub choices: Vec<Choice>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Update {
    Titled(String),
    Said(String),
    Thought(String),
    Ran {
        id: String,
        title: Option<String>,
        progress: Option<Progress>,
        output: Option<Vec<String>>,
    },
    Asked(Permission),
    Planned(Vec<Task>),
    Switched(String),
    Used(Usage),
    Offered(Vec<Command>),
    Configured(Vec<Setting>),
    Idle(Ending),
    Failed(String),
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Entry {
    Prompt(String),
    Said(String),
    Thought(String),
    Ran(Call),
    Asked(Permission),
    Answered(String),
    Failed(String),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum State {
    #[default]
    Idle,
    Working,
    Blocked,
}

/// What the surface owes an update beyond redrawing.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Consequence {
    #[default]
    Nothing,
    Rescan,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Transcript {
    entries: Vec<Entry>,
    title: Option<String>,
    state: State,
    plan: Vec<Task>,
    mode: Option<String>,
    usage: Option<Usage>,
    commands: Vec<Command>,
    settings: Vec<Setting>,
}

impl Transcript {
    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    pub fn title(&self) -> Option<&str> {
        self.title.as_deref()
    }

    pub fn state(&self) -> State {
        self.state
    }

    pub fn plan(&self) -> &[Task] {
        &self.plan
    }

    pub fn mode(&self) -> Option<&str> {
        self.mode.as_deref()
    }

    pub fn usage(&self) -> Option<&Usage> {
        self.usage.as_ref()
    }

    pub fn commands(&self) -> &[Command] {
        &self.commands
    }

    pub fn settings(&self) -> &[Setting] {
        &self.settings
    }

    pub fn pending(&self) -> Option<&Permission> {
        self.entries.iter().rev().find_map(|entry| match entry {
            Entry::Asked(permission) => Some(permission),
            _ => None,
        })
    }

    pub fn prompted(&mut self, text: String) {
        self.entries.push(Entry::Prompt(text));
        self.state = State::Working;
    }

    pub fn answered(&mut self, permission: &str, choice: &str) {
        let found = self.entries.iter().enumerate().find_map(|(index, entry)| match entry {
            Entry::Asked(asked) if asked.id == permission => Some((index, asked)),
            _ => None,
        });
        let Some((index, asked)) = found else {
            return;
        };

        let name = asked
            .choices
            .iter()
            .find(|option| option.id == choice)
            .map_or_else(|| choice.to_owned(), |option| option.name.clone());
        let answered = Entry::Answered(name);

        self.entries[index] = answered;
        self.state = State::Working;
    }

    pub fn apply(&mut self, update: Update) -> Consequence {
        match update {
            Update::Titled(title) => {
                self.title = Some(title);
                Consequence::Nothing
            }
            Update::Said(text) => {
                self.state = State::Working;
                self.said(&text);
                Consequence::Nothing
            }
            Update::Thought(text) => {
                self.state = State::Working;
                self.thought(&text);
                Consequence::Nothing
            }
            Update::Ran { id, title, progress, output } => {
                self.state = State::Working;

                match self.record(id, title, progress, output) {
                    Progress::Done => Consequence::Rescan,
                    _ => Consequence::Nothing,
                }
            }
            Update::Asked(permission) => {
                self.state = State::Blocked;
                self.entries.push(Entry::Asked(permission));
                Consequence::Nothing
            }
            Update::Planned(plan) => {
                self.plan = plan;
                Consequence::Nothing
            }
            Update::Switched(mode) => {
                self.mode = Some(mode);
                Consequence::Nothing
            }
            Update::Used(usage) => {
                self.usage = Some(usage);
                Consequence::Nothing
            }
            Update::Offered(commands) => {
                self.commands = commands;
                Consequence::Nothing
            }
            Update::Configured(settings) => {
                self.settings = settings;
                Consequence::Nothing
            }
            Update::Idle(_) => {
                self.state = State::Idle;
                Consequence::Rescan
            }
            Update::Failed(message) => {
                self.state = State::Idle;
                self.entries.push(Entry::Failed(message));
                Consequence::Rescan
            }
        }
    }

    fn said(&mut self, text: &str) {
        match self.entries.last_mut() {
            Some(Entry::Said(said)) => said.push_str(text),
            _ => self.entries.push(Entry::Said(text.to_owned())),
        }
    }

    fn thought(&mut self, text: &str) {
        match self.entries.last_mut() {
            Some(Entry::Thought(thought)) => thought.push_str(text),
            _ => self.entries.push(Entry::Thought(text.to_owned())),
        }
    }

    fn record(
        &mut self,
        id: String,
        title: Option<String>,
        progress: Option<Progress>,
        output: Option<Vec<String>>,
    ) -> Progress {
        let existing = self.entries.iter_mut().find_map(|entry| match entry {
            Entry::Ran(ran) if ran.id == id => Some(ran),
            _ => None,
        });

        let Some(ran) = existing else {
            let call = Call {
                id,
                title: title.unwrap_or_default(),
                progress: progress.unwrap_or_default(),
                output: output.unwrap_or_default(),
            };
            let started = call.progress;
            self.entries.push(Entry::Ran(call));

            return started;
        };

        if let Some(title) = title {
            ran.title = title;
        }
        if let Some(progress) = progress {
            ran.progress = progress;
        }
        if let Some(output) = output {
            ran.output = output;
        }

        ran.progress
    }
}
