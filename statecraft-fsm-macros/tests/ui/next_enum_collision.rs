// Two distinct branching transitions must not generate colliding target-enum
// names. `(AB, C)` and `(A, BC)` both map to `ABCNext`, which is our diagnostic.
use statecraft_fsm::fsm;

#[fsm(initial = Start)]
impl Machine {
    type Context = ();

    #[on(state = AB, event = C, next = [X, Y])]
    async fn h1(&mut self) -> ABCNext {
        ABCNext::X
    }

    #[on(state = A, event = BC, next = [X, Y])]
    async fn h2(&mut self) -> ABCNext {
        ABCNext::X
    }
}

fn main() {}
