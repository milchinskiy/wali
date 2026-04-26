use std::sync::mpsc;
use std::thread;

pub mod apply;
pub mod plan;

pub const BRAILLE: &str = "⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏ ";
pub const BRAILLE_SUCCESS: &str = "⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏✓";
pub const BRAILLE_FAIL: &str = "⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏✗";

pub enum RenderKind {
    Human,
    Json { pretty: bool },
    Text,
}

pub trait Layout: Send + 'static {
    type Event: Send + 'static;

    fn begin(&mut self) -> crate::Result {
        Ok(())
    }

    fn handle(&mut self, event: Self::Event) -> crate::Result;

    fn end(&mut self) -> crate::Result {
        Ok(())
    }
}

pub struct Reporter<L: Layout> {
    tx: mpsc::Sender<L::Event>,
    join: Option<thread::JoinHandle<crate::Result>>,
}

#[derive(Clone)]
pub struct ReportSender<E> {
    tx: mpsc::Sender<E>,
}

impl<E> ReportSender<E> {
    pub fn send(&self, event: E) -> crate::Result {
        self.tx
            .send(event)
            .map_err(|_| crate::Error::Reporter("failed to send event, channel closed".into()))
    }
}

impl<L: Layout> Reporter<L> {
    pub fn new(mut layout: L) -> Self {
        let (tx, rx) = mpsc::channel::<L::Event>();

        let join = thread::spawn(move || -> crate::Result {
            layout.begin()?;

            while let Ok(event) = rx.recv() {
                layout.handle(event)?;
            }

            layout.end()
        });

        Self { tx, join: Some(join) }
    }

    pub fn sender(&self) -> ReportSender<L::Event> {
        ReportSender { tx: self.tx.clone() }
    }

    pub fn join(mut self) -> crate::Result {
        drop(self.tx);

        match self.join.take().unwrap().join() {
            Ok(result) => result,
            Err(_) => Err(crate::Error::Reporter("thread panicked".into())),
        }
    }
}

pub trait Renderer: Send {
    type State;
    type Event;

    fn begin(&mut self, _state: &Self::State) -> crate::Result {
        Ok(())
    }

    fn handle(&mut self, event: &Self::Event, state: &mut Self::State) -> crate::Result;

    fn end(&mut self, _state: &Self::State) -> crate::Result {
        Ok(())
    }
}
