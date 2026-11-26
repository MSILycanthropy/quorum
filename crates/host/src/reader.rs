//! The thread that reads the terminal's input and hands it to the host's channel.

use std::sync::mpsc::{Receiver, Sender, SyncSender, TryRecvError, channel, sync_channel};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use ratatui::crossterm::event;

use crate::event::Event;

const POLL: Duration = Duration::from_millis(20);
const ACKNOWLEDGE: Duration = Duration::from_millis(200);
const STOP: Duration = Duration::from_millis(100);

enum Command {
    Pause(SyncSender<()>),
    Resume,
    Stop,
}

#[derive(Debug)]
pub(crate) struct Reader {
    commands: Sender<Command>,
    thread: JoinHandle<()>,
}

impl Reader {
    pub(crate) fn start<E>(events: Sender<E>) -> Self
    where
        E: From<Event> + Send + 'static,
    {
        let (commands, received) = channel();
        let thread = thread::Builder::new()
            .name("terminal".to_owned())
            .spawn(move || read(&events, &received))
            .expect("the terminal reader thread cannot fail to start");

        Self { commands, thread }
    }

    pub(crate) fn pause(&self) {
        let (acknowledge, acknowledged) = sync_channel(1);
        if self.commands.send(Command::Pause(acknowledge)).is_ok() {
            let _ = acknowledged.recv_timeout(ACKNOWLEDGE);
        }
    }

    pub(crate) fn resume(&self) {
        let _ = self.commands.send(Command::Resume);
    }

    pub(crate) fn stop(&self) {
        let _ = self.commands.send(Command::Stop);
        let deadline = Instant::now() + STOP;
        while !self.thread.is_finished() && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(5));
        }
    }
}

fn read<E: From<Event>>(events: &Sender<E>, commands: &Receiver<Command>) {
    loop {
        match commands.try_recv() {
            Ok(Command::Pause(acknowledge)) => {
                if !paused(&acknowledge, commands) {
                    return;
                }
            }
            Ok(Command::Resume) | Err(TryRecvError::Empty) => {}
            Ok(Command::Stop) | Err(TryRecvError::Disconnected) => return,
        }
        match event::poll(POLL) {
            Ok(true) => {}
            Ok(false) => continue,
            Err(_) => return,
        }
        let Ok(event) = event::read() else {
            return;
        };
        let Some(event) = Event::from_crossterm(event) else {
            continue;
        };
        if events.send(E::from(event)).is_err() {
            return;
        }
    }
}

fn paused(acknowledge: &SyncSender<()>, commands: &Receiver<Command>) -> bool {
    let _ = acknowledge.send(());
    loop {
        match commands.recv() {
            Ok(Command::Resume) => return true,
            Ok(Command::Pause(again)) => {
                let _ = again.send(());
            }
            Ok(Command::Stop) | Err(_) => return false,
        }
    }
}
