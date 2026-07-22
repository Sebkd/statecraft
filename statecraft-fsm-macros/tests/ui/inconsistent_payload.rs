// The same event name must declare the same payload everywhere. Here `Go` is
// declared with two different payload types, which is our own diagnostic.
use statecraft_fsm::fsm;

#[derive(Debug)]
struct A;

#[derive(Debug)]
struct B;

#[fsm(initial = Idle)]
impl Machine {
    type Context = ();

    #[on(state = Idle, event = Go(A), next = Other)]
    async fn on_first(&mut self, _a: A) {}

    #[on(state = Other, event = Go(B), next = Idle)]
    async fn on_second(&mut self, _b: B) {}
}

fn main() {}
