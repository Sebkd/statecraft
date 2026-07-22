// A fallible handler requires the FSM to declare `type Error`. Omitting it is
// our own diagnostic, pointing at the handler.
use statecraft_fsm::fsm;

#[derive(Debug)]
struct Boom;

#[fsm(initial = Idle)]
impl Machine {
    type Context = ();

    #[on(state = Idle, event = Go, next = Done)]
    async fn on_go(&mut self) -> Result<(), Boom> {
        Err(Boom)
    }
}

fn main() {}
