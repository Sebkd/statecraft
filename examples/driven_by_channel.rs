//! Owned FSM driven by an external event source: a separate task produces
//! events on a timer and feeds them into `apply`. No Tokio adapter needed.
//!
//! Run: `cargo run --example driven_by_channel`

use std::time::Duration;

use statecraft_fsm::fsm;
use tokio::sync::mpsc;

#[fsm(initial = Red)]
impl Light {
    type Context = ();

    #[on(state = Red, event = Tick, next = Green)]
    async fn on_red(&mut self) {}

    #[on(state = Green, event = Tick, next = Yellow)]
    async fn on_green(&mut self) {}

    #[on(state = Yellow, event = Tick, next = Red)]
    async fn on_yellow(&mut self) {}
}

#[tokio::main]
async fn main() {
    let (tx, mut rx) = mpsc::channel(8);

    // External source: emit a Tick every 50ms, a handful of times.
    tokio::spawn(async move {
        for _ in 0..6 {
            tokio::time::sleep(Duration::from_millis(50)).await;
            if tx.send(LightEvent::Tick).await.is_err() {
                break;
            }
        }
    });

    // Owned FSM: pull events off the channel and apply them.
    let mut light = Light::new(());
    while let Some(event) = rx.recv().await {
        light.apply(event).await.unwrap();
        println!("light -> {:?}", light.state());
    }
}
