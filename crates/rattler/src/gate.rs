use std::sync::Mutex;
use tokio::sync::watch;

/// A `Gate` is a synchronization primitive where multiple tasks wait for a gate which can either be
/// open or closed. If the gate is opened tasks continue. Otherwise, they wait until the gate is
/// opened. Any number of tasks can enter the gate if it's open.
#[derive(Debug)]
pub struct Gate {
    state: Mutex<GateState>,
    wait: watch::Receiver<bool>,
}

#[derive(Debug)]
struct GateState {
    waker: watch::Sender<bool>,
}

impl Gate {
    /// Creates a new [`Gate`]. Initially the gate starts closed.
    pub fn new() -> Self {
        let (waker, wait) = watch::channel(false);
        Self {
            state: Mutex::new(GateState { waker }),
            wait,
        }
    }

    /// Opens the gate, so all pending tasks can continue. Returns true if the gate was closed,
    /// false otherwise.
    pub fn open(&self) -> bool {
        let mut state = self.state.lock().expect("gate lock was poisoned");

        if *state.waker.borrow() == true {
            return false;
        }

        // Notify all the waiting tasks. We dont care about the error because its only returned if
        // there are no receivers.
        let _ = state.waker.send(true);

        return true;
    }

    /// Asynchronously waits for the gate to be opened.
    pub async fn wait(&self) {
        let mut waiter = self.wait.clone();
        loop {
            let _ = waiter.changed().await;
            if *waiter.borrow() == true {
                break;
            }
        }
    }
}
