// A payload event's handler must accept the payload argument. Forgetting it is
// our own diagnostic, pointing at the handler.
use statecraft_fsm::fsm;

#[derive(Debug)]
pub struct Order;

#[fsm(initial = Idle)]
impl Machine {
    type Context = ();

    #[on(state = Idle, event = Add(Order), next = Idle)]
    async fn on_add(&mut self) {}
}

fn main() {}
