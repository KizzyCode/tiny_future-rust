//! Implements the future

use std::{
    fmt::{self, Debug, Formatter},
    sync::{
        atomic::{AtomicBool, Ordering::SeqCst},
        Arc, Condvar, Mutex,
    },
    time::Duration,
};

/// The inner state of the future
pub struct Future<T> {
    /// The signal variable
    signal: Condvar,
    /// The result
    result: Mutex<Option<T>>,
}
impl<T> Future<T> {
    /// Creates a new inner state of the future
    pub fn new() -> Self {
        Self { signal: Condvar::new(), result: Mutex::default() }
    }
}
impl<T> Debug for Future<T> {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        // Get a debug representation for the result
        let result: &dyn Debug = match self.result.lock() {
            Ok(result) if result.is_some() => &Some("<opaque>"),
            Ok(_) => &Option::<&str>::None,
            Err(_) => &"<poisoned>",
        };

        // Debug-format the struct
        f.debug_struct("Future").field("signal", &"<opaque>").field("result", &result).finish()
    }
}

/// A setter for a future
pub struct Setter<T> {
    /// The underlying future
    future: Arc<Future<T>>,
    /// Whether the future has been cancelled or not
    cancelled: Arc<AtomicBool>,
}
impl<T> Setter<T> {
    /// Creates a new setter
    pub(in crate) const fn new(future: Arc<Future<T>>, cancelled: Arc<AtomicBool>) -> Self {
        Self { future, cancelled }
    }

    /// Whether the future has been cancelled or not
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(SeqCst)
    }
    /// Cancels the future
    pub fn cancel(&self) {
        // Cancel the future and wake waiting threads
        self.cancelled.store(true, SeqCst);
        self.future.signal.notify_all();
    }

    /// Sets the result
    pub fn set(self, value: T) {
        // Only do something if the future has not been cancelled
        if !self.is_cancelled() {
            // Set result and wake waiting threads
            let mut result = self.future.result.lock().expect("The future is poisoned?!");
            *result = Some(value);
            self.future.signal.notify_all();
        }
    }
}
impl<T> Debug for Setter<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("Setter").field("future", &self.future).field("cancelled", &self.cancelled.load(SeqCst)).finish()
    }
}
impl<T> Drop for Setter<T> {
    fn drop(&mut self) {
        self.cancel();
    }
}

/// A getter for a future
pub struct Getter<T> {
    /// The underlying future
    future: Arc<Future<T>>,
    /// Whether the future has been cancelled or not
    cancelled: Arc<AtomicBool>,
}
impl<T> Getter<T> {
    /// Creates a new getter
    pub(in crate) const fn new(future: Arc<Future<T>>, cancelled: Arc<AtomicBool>) -> Self {
        Self { future, cancelled }
    }

    /// Whether the future has been cancelled or not
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(SeqCst)
    }
    /// Cancels the future
    pub fn cancel(&self) {
        self.cancelled.store(true, SeqCst);
    }

    /// Waits until the result is ready, returns either `Some(result)` if the future has completed successfully or `None`
    /// if the future has been cancelled
    pub fn wait(self) -> Option<T> {
        // Wait for the future if necessary
        let cond = |result: &mut Option<T>| result.is_none() && !self.is_cancelled();
        let result = self.future.result.lock().expect("The future is poisoned?!");
        let mut result = self.future.signal.wait_while(result, cond).expect("The future is poisoned?!");

        // Claim the result
        result.take()
    }
    /// Waits until a result is available or the timeout is reached
    pub fn wait_timeout(self, timeout: Duration) -> Result<Option<T>, Self> {
        // Wait while the queue is empty and not cancelled and the timeout is not reached
        let cond = |queue: &mut Option<T>| queue.is_none() && !self.is_cancelled();
        let result = self.future.result.lock().expect("The future is poisoned?!");
        let (mut result, timeout_result) =
            self.future.signal.wait_timeout_while(result, timeout, cond).expect("The future is poisoned?!");

        // Claim the result
        if timeout_result.timed_out() {
            drop(result);
            return Err(self);
        }
        Ok(result.take())
    }
}
impl<T> Debug for Getter<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("Getter").field("future", &self.future).field("cancelled", &self.cancelled.load(SeqCst)).finish()
    }
}
impl<T> Drop for Getter<T> {
    fn drop(&mut self) {
        self.cancel();
    }
}
