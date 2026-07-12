// A handler's error type must match the FSM's `type Error`. Here the handler
// returns `Result<_, Other>` while `type Error = Boom`, so wrapping the error
// into `ApplyError::Handler` cannot type-check.
use statecraft::fsm;

#[derive(Debug)]
struct Boom;

#[derive(Debug)]
struct Other;

#[fsm(initial = Idle)]
impl Machine {
    type Context = ();
    type Error = Boom;

    #[on(state = Idle, event = Start, next = Running)]
    async fn on_start(&mut self) -> Result<(), Other> {
        Err(Other)
    }
}

fn main() {}
