// D2 guarantee: a branching handler cannot return a target outside its declared
// `next` list. `Idle` is not among `[Running, Done]`, so the generated
// `RunningCheckNext` enum has no such variant and this must fail to compile.
use statecraft_fsm::fsm;

#[fsm(initial = Idle)]
impl Machine {
    type Context = ();

    #[on(state = Running, event = Check, next = [Running, Done])]
    async fn on_check(&mut self) -> RunningCheckNext {
        RunningCheckNext::Idle
    }
}

fn main() {}
