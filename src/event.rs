use std::io;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use crossterm::event::{self, Event as CEvent, KeyEvent};
use crossterm::event::KeyEventKind;

pub enum Event {
    Key(KeyEvent),
    Tick,
}

pub struct EventHandler {
    rx: mpsc::Receiver<Event>,
}

impl EventHandler {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel();
        let handler_tx = tx.clone();
        thread::spawn(move || loop {
            if event::poll(Duration::from_millis(200)).ok().unwrap_or(false) {
                if let Ok(CEvent::Key(key)) = event::read() {
                    if key.kind == KeyEventKind::Press {
                        handler_tx.send(Event::Key(key)).ok();
                    }
                }
            }
            if handler_tx.send(Event::Tick).is_err() {
                break;
            }
        });
        Self { rx }
    }

    pub fn next(&self) -> Result<Event, io::Error> {
        self.rx.recv().map_err(|_| io::Error::new(io::ErrorKind::Other, "channel closed"))
    }
}
