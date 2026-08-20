//! The dispatch frame, measured the way it actually fails: a thread with a
//! bounded stack.
//!
//! `boxed_dispatch.rs` asserts byte counts of the `apply` future. That is the
//! contract, and it holds identically in every profile because the size of a
//! coroutine is fixed before optimisation. This file is the confirmation that
//! those byte counts describe something real — and, just as importantly, it
//! records where the correspondence between "future size" and "stack needed"
//! breaks down.
//!
//! Both tests are `#[ignore]`d, for two separate reasons:
//!
//! * They deliberately abort a child process (a stack overflow kills the
//!   process, not the test, so each case re-executes this binary and inspects
//!   the exit status). That is noisy locally and writes crash reports on some
//!   platforms.
//! * **The stack cost of boxing is an optimiser decision, not a guarantee.**
//!   `Box::pin(fut)` constructs `fut` and then moves it into the allocation;
//!   whether that construction ever touches the stack is up to codegen. In a
//!   debug build it always does, so a boxed heavy handler still needs a stack
//!   roughly its own future's size. In a release build the construction is
//!   commonly built in place, and — for this shape — so is the *unboxed*
//!   future, which is why release does not separate the two cases at all.
//!
//! Measured on the path these tests drive — owned mode, the `apply` future
//! handed straight to `block_on` — with a handler holding two 512 KiB locals
//! across suspends:
//!
//! | profile | boxed | inlined |
//! |---------|-------|---------|
//! | debug   | 2 MiB | 16 MiB  |
//! | release | 1 MiB | 1 MiB   |
//!
//! A spawned FSM sits one future deeper and costs more: 4 MiB boxed against
//! 16 MiB inlined in debug, 1 MiB against 4 MiB in release. Release separates
//! the two cases there but not here, which is the same point from the other
//! side — what boxing buys on the stack depends on codegen.
//!
//! So: run these with `cargo test --test dispatch_stack -- --ignored` in a
//! **debug** build, where the separation is stark. What boxing guarantees
//! everywhere is the bounded future size; what it buys on the stack in a given
//! profile is what these tests show.

use statecraft_fsm::fsm;
use std::process::Command;

/// Set in the child; its presence means "do the work" rather than "spawn a
/// child".
const CHILD: &str = "STATECRAFT_DISPATCH_STACK_CHILD";

/// Between the two debug figures above: the boxed machine clears it, the
/// inlined one does not.
const BUDGET: usize = 4 * 1024 * 1024;

const HEAVY: usize = 512 * 1024;

/// A real suspend point. `std::future::ready` would not do: it is always ready,
/// and the optimiser is free to collapse the whole coroutine around it, which
/// silently removes the very locals these tests are about.
struct YieldOnce(bool);

impl std::future::Future for YieldOnce {
    type Output = u8;

    fn poll(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<u8> {
        if self.0 {
            std::task::Poll::Ready(7)
        } else {
            self.0 = true;
            cx.waker().wake_by_ref();
            std::task::Poll::Pending
        }
    }
}

async fn tick() -> u8 {
    YieldOnce(false).await
}

#[fsm(initial = Idle)]
impl Boxed {
    type Context = usize;

    #[on(state = Idle, event = Go, next = Done, boxed)]
    async fn on_go(&mut self) {
        let mut a = [0u8; HEAVY];
        a[0] = tick().await;
        let mut b = [0u8; HEAVY];
        b[0] = tick().await;
        self.context += std::hint::black_box(&a)[0] as usize + std::hint::black_box(&b)[0] as usize;
    }
}

#[fsm(initial = Idle)]
impl Inlined {
    type Context = usize;

    #[on(state = Idle, event = Go, next = Done)]
    async fn on_go(&mut self) {
        let mut a = [0u8; HEAVY];
        a[0] = tick().await;
        let mut b = [0u8; HEAVY];
        b[0] = tick().await;
        self.context += std::hint::black_box(&a)[0] as usize + std::hint::black_box(&b)[0] as usize;
    }
}

fn block_on<F: std::future::Future>(f: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap()
        .block_on(f)
}

/// Run `body` on a thread with [`BUDGET`] of stack. Aborts the process if it
/// does not fit.
fn drive_within_budget<F: FnOnce() + Send + 'static>(body: F) {
    std::thread::Builder::new()
        .stack_size(BUDGET)
        .spawn(body)
        .unwrap()
        .join()
        .unwrap();
}

/// Re-run this test binary with the marker set and capture what happened.
fn run_child(test_name: &str) -> std::process::Output {
    Command::new(std::env::current_exe().unwrap())
        .args([test_name, "--exact", "--ignored", "--test-threads=1"])
        .env(CHILD, "1")
        .output()
        .unwrap()
}

/// Did the child die *of a stack overflow*, as opposed to any other failure?
///
/// Only the negative test needs this, and that test is compiled out under
/// `boxed-all` (no unboxed transition to compare against), so gate it the same
/// way rather than leave dead code behind.
///
/// Exit status alone would not tell us: a panic — `apply` regressing to `Err`,
/// say — also exits non-zero, and would let the negative test below pass while
/// proving nothing. The runtime announces this particular death, so match on
/// that. (Observed: the message on stderr, then `abort`, so status 134.)
#[cfg(not(feature = "boxed-all"))]
fn overflowed_the_stack(out: &std::process::Output) -> bool {
    !out.status.success()
        && String::from_utf8_lossy(&out.stderr).contains("has overflowed its stack")
}

#[test]
#[ignore = "spawns a child process; see the module docs"]
fn boxed_handler_fits_the_budget() {
    if std::env::var(CHILD).is_ok() {
        drive_within_budget(|| {
            let mut m = Boxed::new(0);
            block_on(m.apply(BoxedEvent::Go)).unwrap();
            assert_eq!(m.state(), BoxedState::Done);
        });
        return;
    }
    let out = run_child("boxed_handler_fits_the_budget");
    assert!(
        out.status.success(),
        "a boxed handler failed inside a {BUDGET} byte stack: {}",
        String::from_utf8_lossy(&out.stderr),
    );
}

/// The counterpart, and the reason the test above means anything: the same
/// machine without the mark does not fit the same budget. Only meaningful
/// where the mark is what makes the difference — under `boxed-all` there is no
/// unboxed transition to compare against.
#[cfg(not(feature = "boxed-all"))]
#[test]
#[ignore = "deliberately overflows a child process's stack; debug builds only"]
fn inlined_handler_overflows_the_budget() {
    if std::env::var(CHILD).is_ok() {
        drive_within_budget(|| {
            let mut m = Inlined::new(0);
            block_on(m.apply(InlinedEvent::Go)).unwrap();
        });
        return;
    }
    let out = run_child("inlined_handler_overflows_the_budget");
    assert!(
        overflowed_the_stack(&out),
        "expected an inlined handler to overflow a {BUDGET} byte stack, but the \
         child ended as {} — in a release build fitting is expected, see the \
         module docs; stderr: {}",
        out.status,
        String::from_utf8_lossy(&out.stderr),
    );
}
