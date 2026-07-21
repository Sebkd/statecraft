// Without the `public-emit` feature, the generated `emit` is module-private:
// calling it from outside the FSM's module must not compile.

mod inner {
    use statecraft::fsm;

    #[fsm(initial = Idle)]
    impl Machine {
        type Context = ();

        #[on(state = Idle, event = Go, next = Idle)]
        async fn on_go(&mut self) {}
    }
}

fn main() {
    let mut m = inner::Machine::new(());
    m.emit(inner::MachineEvent::Go);
}
