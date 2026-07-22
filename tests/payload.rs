//! Event payloads: `#[on(event = Foo(Type), ...)]`. The payload is passed to the
//! handler by value; it works with branching and with `self.emit`.

use statecraft_fsm::fsm;

// --- payload reaches the handler ---

#[derive(Debug)]
pub struct Order {
    qty: u32,
}

#[derive(Debug, Default)]
struct Totals {
    total: u32,
}

#[fsm(initial = Idle)]
impl Shop {
    type Context = Totals;

    #[on(state = Idle, event = Add(Order), next = Idle)]
    async fn on_add(&mut self, order: Order) {
        self.context.total += order.qty;
    }
}

#[tokio::test]
async fn test_payload_reaches_handler() {
    let mut m = Shop::new(Totals::default());
    m.apply(ShopEvent::Add(Order { qty: 3 })).await.unwrap();
    m.apply(ShopEvent::Add(Order { qty: 4 })).await.unwrap();
    assert_eq!(m.context.total, 7);
}

// --- payload drives a compile-time-checked branch ---

#[derive(Debug)]
pub struct Amount(u32);

#[fsm(initial = Idle)]
impl Gate {
    type Context = ();

    #[on(state = Idle, event = Check(Amount), next = [Approved, Denied])]
    async fn on_check(&mut self, amount: Amount) -> IdleCheckNext {
        if amount.0 >= 100 {
            IdleCheckNext::Approved
        } else {
            IdleCheckNext::Denied
        }
    }
}

#[tokio::test]
async fn test_payload_drives_branch() {
    let mut approved = Gate::new(());
    approved.apply(GateEvent::Check(Amount(150))).await.unwrap();
    assert_eq!(approved.state(), GateState::Approved);

    let mut denied = Gate::new(());
    denied.apply(GateEvent::Check(Amount(50))).await.unwrap();
    assert_eq!(denied.state(), GateState::Denied);
}

// --- self.emit carries a (non-Copy) payload forward ---

#[derive(Debug, Default)]
struct Log {
    msgs: Vec<String>,
}

#[fsm(initial = Idle)]
impl Pipe {
    type Context = Log;

    #[on(state = Idle, event = Start(String), next = Working)]
    async fn on_start(&mut self, name: String) {
        self.context.msgs.push(format!("start:{name}"));
        self.emit(PipeEvent::Finish(name)); // emitted event carries a payload
    }

    #[on(state = Working, event = Finish(String), next = Done)]
    async fn on_finish(&mut self, name: String) {
        self.context.msgs.push(format!("finish:{name}"));
    }
}

#[tokio::test]
async fn test_emit_with_payload() {
    let mut m = Pipe::new(Log::default());
    m.apply(PipeEvent::Start("job".into())).await.unwrap();
    assert_eq!(m.state(), PipeState::Done);
    assert_eq!(m.context.msgs, vec!["start:job", "finish:job"]);
}

// --- unit and payload events coexist in one FSM ---

#[fsm(initial = Idle)]
impl Mixed {
    type Context = ();

    #[on(state = Idle, event = Tick, next = Idle)]
    async fn on_tick(&mut self) {}

    #[on(state = Idle, event = Set(u32), next = Idle)]
    async fn on_set(&mut self, _n: u32) {}
}

#[tokio::test]
async fn test_unit_and_payload_mixed() {
    let mut m = Mixed::new(());
    m.apply(MixedEvent::Tick).await.unwrap();
    m.apply(MixedEvent::Set(9)).await.unwrap();
    assert_eq!(m.state(), MixedState::Idle);
}
