// An event with a payload, passed to the handler by value, compiles.
use statecraft_fsm::fsm;

#[derive(Debug)]
pub struct Order {
    qty: u32,
}

#[fsm(initial = Idle)]
impl Shop {
    type Context = ();

    #[on(state = Idle, event = Add(Order), next = Idle)]
    async fn on_add(&mut self, order: Order) {
        let _ = order.qty;
    }
}

fn main() {}
