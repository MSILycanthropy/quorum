//! An ACP agent in a subprocess, driven from a thread of its own.

use std::collections::HashMap;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};

use agent_client_protocol::schema::ProtocolVersion;
use agent_client_protocol::schema::v1::{
    CancelNotification, ContentBlock, InitializeRequest, NewSessionRequest, PlanEntryStatus,
    PromptRequest, RequestPermissionOutcome, RequestPermissionRequest, RequestPermissionResponse,
    SelectedPermissionOutcome, SessionConfigKind, SessionConfigOption, SessionConfigSelectOptions,
    SessionId, SessionNotification, SessionUpdate, StopReason, TextContent, ToolCallContent,
    ToolCallStatus,
};
use agent_client_protocol::{AcpAgent, Agent, Client, ConnectionTo, Responder};
use futures::StreamExt;
use futures::channel::mpsc::{UnboundedSender, unbounded};

use crate::agent::{
    Command, Cost, Ending, Error, Permission, Progress, Result, Setting, Task, Update, Usage,
};

const DEFAULT_COMMAND: &str = "npx -y @agentclientprotocol/claude-agent-acp";

type Waiting = Arc<Mutex<HashMap<String, Responder<RequestPermissionResponse>>>>;
type Modes = Arc<Mutex<HashMap<String, String>>>;

#[derive(Clone, Debug)]
enum Instruction {
    Prompt(String),
    Answer { permission: String, choice: String },
    Cancel,
}

/// Where an agent is launched from, and what it is launched against.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Spawn {
    pub command: String,
    pub directory: PathBuf,
}

impl Default for Spawn {
    fn default() -> Self {
        Self { command: DEFAULT_COMMAND.to_owned(), directory: PathBuf::from(".") }
    }
}

impl Spawn {
    #[expect(
        clippy::missing_errors_doc,
        reason = "the Error enum names the only failure, an unparseable agent command"
    )]
    pub fn session(self, updates: Sender<Update>) -> Result<Session> {
        let agent = AcpAgent::from_str(&self.command).map_err(|error| Error::Spawn {
            command: self.command.clone(),
            message: error.to_string(),
        })?;
        let (commands, incoming) = unbounded();

        std::thread::Builder::new().name("agent".to_owned()).spawn(move || {
            let outcome =
                futures::executor::block_on(converse(agent, self.directory, &updates, incoming));
            if let Err(error) = outcome {
                let _ = updates.send(Update::Failed(error.to_string()));
            }
        })?;

        Ok(Session { commands })
    }
}

/// The live agent, as the surface holds it.
#[derive(Clone, Debug)]
pub struct Session {
    commands: UnboundedSender<Instruction>,
}

impl Session {
    pub fn prompt(&self, text: String) {
        let _ = self.commands.unbounded_send(Instruction::Prompt(text));
    }

    pub fn answer(&self, permission: &str, choice: &str) {
        let answer =
            Instruction::Answer { permission: permission.to_owned(), choice: choice.to_owned() };
        let _ = self.commands.unbounded_send(answer);
    }

    pub fn cancel(&self) {
        let _ = self.commands.unbounded_send(Instruction::Cancel);
    }
}

async fn converse(
    agent: AcpAgent,
    directory: PathBuf,
    updates: &Sender<Update>,
    mut incoming: futures::channel::mpsc::UnboundedReceiver<Instruction>,
) -> Result<()> {
    let waiting: Waiting = Waiting::default();
    let modes: Modes = Modes::default();
    let noticed = updates.clone();
    let asked = updates.clone();
    let held = waiting.clone();
    let named = modes.clone();

    Client
        .builder()
        .on_receive_notification(
            async move |notification: SessionNotification, _connection| {
                for update in project(notification.update, &named) {
                    let _ = noticed.send(update);
                }

                Ok(())
            },
            agent_client_protocol::on_receive_notification!(),
        )
        .on_receive_request(
            async move |request: RequestPermissionRequest, responder, _connection| {
                let permission = ask(&request);
                held.lock()
                    .expect("the responder map is only locked to insert and remove")
                    .insert(permission.id.clone(), responder);
                let _ = asked.send(Update::Asked(permission));

                Ok(())
            },
            agent_client_protocol::on_receive_request!(),
        )
        .connect_with(agent, async move |connection: ConnectionTo<Agent>| {
            connection
                .send_request(InitializeRequest::new(ProtocolVersion::V1))
                .block_task()
                .await?;
            let opened =
                connection.send_request(NewSessionRequest::new(directory)).block_task().await?;
            let session = opened.session_id;

            while let Some(instruction) = incoming.next().await {
                match instruction {
                    Instruction::Prompt(text) => {
                        turn(&connection, &session, text, updates.clone())?;
                    }
                    Instruction::Answer { permission, choice } => {
                        respond(&waiting, &permission, &choice)?;
                    }
                    Instruction::Cancel => {
                        connection.send_notification(CancelNotification::new(session.clone()))?;
                    }
                }
            }

            Ok(())
        })
        .await?;

    Ok(())
}

fn turn(
    connection: &ConnectionTo<Agent>,
    session: &SessionId,
    text: String,
    updates: Sender<Update>,
) -> std::result::Result<(), agent_client_protocol::Error> {
    let prompt =
        PromptRequest::new(session.clone(), vec![ContentBlock::Text(TextContent::new(text))]);
    let spawned = connection.clone();

    connection.clone().spawn(async move {
        match spawned.send_request(prompt).block_task().await {
            Ok(response) => {
                let _ = updates.send(Update::Idle(ending(response.stop_reason)));
            }
            Err(error) => {
                let _ = updates.send(Update::Failed(error.to_string()));
            }
        }

        Ok(())
    })
}

fn respond(
    waiting: &Waiting,
    permission: &str,
    choice: &str,
) -> std::result::Result<(), agent_client_protocol::Error> {
    let responder = waiting
        .lock()
        .expect("the responder map is only locked to insert and remove")
        .remove(permission);
    let Some(responder) = responder else {
        return Ok(());
    };

    responder.respond(RequestPermissionResponse::new(RequestPermissionOutcome::Selected(
        SelectedPermissionOutcome::new(choice.to_owned()),
    )))
}

fn ask(request: &RequestPermissionRequest) -> Permission {
    let choices = request
        .options
        .iter()
        .map(|option| crate::agent::Choice {
            id: option.option_id.0.to_string(),
            name: option.name.clone(),
        })
        .collect();

    Permission {
        id: request.tool_call.tool_call_id.0.to_string(),
        title: request.tool_call.fields.title.clone().unwrap_or_else(|| "run this".to_owned()),
        choices,
    }
}

fn project(update: SessionUpdate, modes: &Modes) -> Vec<Update> {
    match update {
        SessionUpdate::AgentMessageChunk(chunk) => {
            text(&chunk.content).map(Update::Said).into_iter().collect()
        }
        SessionUpdate::AgentThoughtChunk(chunk) => {
            text(&chunk.content).map(Update::Thought).into_iter().collect()
        }
        SessionUpdate::ToolCall(call) => vec![Update::Ran {
            id: call.tool_call_id.0.to_string(),
            title: Some(call.title),
            progress: Some(progress(call.status)),
            output: Some(output(&call.content)),
        }],
        SessionUpdate::ToolCallUpdate(call) => vec![Update::Ran {
            id: call.tool_call_id.0.to_string(),
            title: call.fields.title,
            progress: call.fields.status.map(progress),
            output: call.fields.content.as_deref().map(output),
        }],
        SessionUpdate::Plan(plan) => vec![Update::Planned(
            plan.entries
                .into_iter()
                .map(|entry| Task { content: entry.content, progress: standing(&entry.status) })
                .collect(),
        )],
        SessionUpdate::CurrentModeUpdate(mode) => {
            let id = mode.current_mode_id.0.to_string();
            let named =
                modes.lock().expect("the mode table is only locked to read it").get(&id).cloned();

            vec![Update::Switched(named.unwrap_or(id))]
        }
        SessionUpdate::UsageUpdate(usage) => vec![Update::Used(Usage {
            used: usage.used,
            size: usage.size,
            cost: usage.cost.map(|cost| Cost { amount: cost.amount, currency: cost.currency }),
        })],
        SessionUpdate::AvailableCommandsUpdate(offered) => vec![Update::Offered(
            offered
                .available_commands
                .into_iter()
                .map(|command| Command { name: command.name, description: command.description })
                .collect(),
        )],
        SessionUpdate::ConfigOptionUpdate(configured) => {
            vec![Update::Configured(configured.config_options.iter().map(setting).collect())]
        }
        SessionUpdate::SessionInfoUpdate(info) => {
            info.title.take().map(Update::Titled).into_iter().collect()
        }
        _ => Vec::new(),
    }
}

fn text(content: &ContentBlock) -> Option<String> {
    match content {
        ContentBlock::Text(text) => Some(text.text.clone()),
        _ => None,
    }
}

fn output(content: &[ToolCallContent]) -> Vec<String> {
    content
        .iter()
        .filter_map(|piece| match piece {
            ToolCallContent::Content(spoken) => text(&spoken.content),
            _ => None,
        })
        .flat_map(|spoken| spoken.lines().map(str::to_owned).collect::<Vec<_>>())
        .collect()
}

fn setting(option: &SessionConfigOption) -> Setting {
    let value = match &option.kind {
        SessionConfigKind::Select(select) => {
            let options = match &select.options {
                SessionConfigSelectOptions::Ungrouped(options) => options.clone(),
                SessionConfigSelectOptions::Grouped(groups) => {
                    groups.iter().flat_map(|group| group.options.clone()).collect()
                }
                _ => Vec::new(),
            };

            options
                .iter()
                .find(|choice| choice.value == select.current_value)
                .map_or_else(|| select.current_value.0.to_string(), |choice| choice.name.clone())
        }
        SessionConfigKind::Boolean(boolean) => {
            if boolean.current_value { "on" } else { "off" }.to_owned()
        }
        _ => String::new(),
    };

    Setting { name: option.name.clone(), value }
}

fn standing(status: &PlanEntryStatus) -> Progress {
    match status {
        PlanEntryStatus::InProgress => Progress::Running,
        PlanEntryStatus::Completed => Progress::Done,
        _ => Progress::Pending,
    }
}

fn progress(status: ToolCallStatus) -> Progress {
    match status {
        ToolCallStatus::InProgress => Progress::Running,
        ToolCallStatus::Completed => Progress::Done,
        ToolCallStatus::Failed => Progress::Failed,
        _ => Progress::Pending,
    }
}

fn ending(reason: StopReason) -> Ending {
    match reason {
        StopReason::Cancelled => Ending::Cancelled,
        StopReason::Refusal => Ending::Refused,
        StopReason::MaxTokens => Ending::MaxTokens,
        StopReason::MaxTurnRequests => Ending::MaxRequests,
        _ => Ending::Finished,
    }
}
