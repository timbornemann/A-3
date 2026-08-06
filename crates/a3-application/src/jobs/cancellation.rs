use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::task::Poll;
use std::time::Duration;

/// Read-only cooperative cancellation signal passed to one job execution.
#[derive(Clone, Debug)]
pub struct CancellationToken {
    inner: Arc<CancellationState>,
}

#[derive(Debug)]
struct CancellationState {
    requested: AtomicBool,
    gate: Mutex<()>,
    changed: Condvar,
    async_waiter: futures::task::AtomicWaker,
}

impl CancellationToken {
    pub(super) fn new() -> Self {
        Self {
            inner: Arc::new(CancellationState {
                requested: AtomicBool::new(false),
                gate: Mutex::new(()),
                changed: Condvar::new(),
                async_waiter: futures::task::AtomicWaker::new(),
            }),
        }
    }

    /// Returns whether the owner requested cancellation.
    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.inner.requested.load(Ordering::Acquire)
    }

    /// Blocks the current job until cancellation is requested.
    pub fn wait_cancelled(&self) {
        let mut guard = lock_recovering_poison(&self.inner.gate);
        while !self.is_cancelled() {
            guard = match self.inner.changed.wait(guard) {
                Ok(next_guard) => next_guard,
                Err(poisoned) => poisoned.into_inner(),
            };
        }
    }

    /// Waits up to the supplied duration and returns whether cancellation arrived.
    #[must_use]
    pub fn wait_cancelled_timeout(&self, timeout: Duration) -> bool {
        let guard = lock_recovering_poison(&self.inner.gate);
        if self.is_cancelled() {
            return true;
        }

        let wait_result = self
            .inner
            .changed
            .wait_timeout_while(guard, timeout, |_| !self.is_cancelled());
        match wait_result {
            Ok((_, _)) => self.is_cancelled(),
            Err(poisoned) => {
                let _recovered = poisoned.into_inner();
                self.is_cancelled()
            }
        }
    }

    /// Asynchronously waits without occupying a worker thread until cancellation is requested.
    pub async fn cancelled(&self) {
        futures::future::poll_fn(|context| {
            if self.is_cancelled() {
                return Poll::Ready(());
            }
            self.inner.async_waiter.register(context.waker());
            if self.is_cancelled() {
                Poll::Ready(())
            } else {
                Poll::Pending
            }
        })
        .await;
    }

    pub(super) fn request(&self) -> bool {
        let _guard = lock_recovering_poison(&self.inner.gate);
        let already_requested = self.inner.requested.swap(true, Ordering::AcqRel);
        if !already_requested {
            self.inner.changed.notify_all();
            self.inner.async_waiter.wake();
        }
        !already_requested
    }
}

fn lock_recovering_poison<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    match mutex.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

#[cfg(test)]
mod tests {
    use super::CancellationToken;
    use std::sync::mpsc;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn cancellation_wakes_all_waiters() -> Result<(), Box<dyn std::error::Error>> {
        let token = CancellationToken::new();
        let waiter_token = token.clone();
        let async_waiter_token = token.clone();
        let (ready_sender, ready_receiver) = mpsc::sync_channel(0);
        let (done_sender, done_receiver) = mpsc::sync_channel(0);
        let (async_ready_sender, async_ready_receiver) = mpsc::sync_channel(0);
        let (async_done_sender, async_done_receiver) = mpsc::sync_channel(0);
        let waiter = thread::Builder::new()
            .name("a3-cancellation-test".to_owned())
            .spawn(move || {
                let _ = ready_sender.send(());
                waiter_token.wait_cancelled();
                let _ = done_sender.send(());
            })?;
        let async_waiter = thread::Builder::new()
            .name("a3-async-cancellation-test".to_owned())
            .spawn(move || {
                let _ = async_ready_sender.send(());
                futures::executor::block_on(async_waiter_token.cancelled());
                let _ = async_done_sender.send(());
            })?;

        ready_receiver.recv_timeout(Duration::from_secs(1))?;
        async_ready_receiver.recv_timeout(Duration::from_secs(1))?;
        assert!(token.request());
        assert!(!token.request());
        done_receiver.recv_timeout(Duration::from_secs(1))?;
        async_done_receiver.recv_timeout(Duration::from_secs(1))?;
        assert!(waiter.join().is_ok());
        assert!(async_waiter.join().is_ok());
        Ok(())
    }
}
