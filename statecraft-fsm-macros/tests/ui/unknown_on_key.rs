// A typo in an `#[on]` key must name the keys that do exist — `boxed` included.
use statecraft_fsm::fsm;

pub struct Ctx;

#[fsm(initial = Idle)]
impl M {
    type Context = Ctx;

    #[on(state = Idle, event = Go, next = Done, boxxed)]
    async fn on_go(&mut self) {}
}

fn main() {}
