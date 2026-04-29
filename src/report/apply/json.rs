use super::Event;
use super::state::State;

pub(super) struct JsonRender {
    pretty: bool,
}
impl JsonRender {
    pub(super) fn new(pretty: bool) -> Self {
        Self { pretty }
    }
}

impl crate::report::Renderer for JsonRender {
    type State = State;
    type Event = Event;

    fn handle(&mut self, _event: &Self::Event, _state: &mut Self::State) -> crate::Result {
        Ok(())
    }

    fn end(&mut self, state: &Self::State) -> crate::Result {
        println!(
            "{}",
            match self.pretty {
                true => serde_json::to_string_pretty(state)?,
                false => serde_json::to_string(state)?,
            }
        );
        Ok(())
    }
}
