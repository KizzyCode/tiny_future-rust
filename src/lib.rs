#[macro_use] extern crate etrace;
use etrace::Error;


#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum State{ Waiting, Ready, Consumed, Canceled }


/// The futures inner state
struct Inner<T, U> {
	payload: std::sync::Mutex<(State, Option<T>)>,
	cond_var: std::sync::Condvar,
	shared_state: std::sync::Mutex<U>,
	cancel_on_drop: std::sync::atomic::AtomicBool
}
unsafe impl<T, U> Sync for Inner<T, U> {}



pub struct Future<T, U = ()>(std::sync::Arc<Inner<T, U>>);
impl<T, U> Future<T, U> {
	/// Creates a new `Future<T, U>` with `shared_state` as shared-state
	pub fn with_state(shared_state: U) -> Self {
		Future(std::sync::Arc::new(Inner {
			payload: std::sync::Mutex::new((State::Waiting, None)),
			cond_var: std::sync::Condvar::new(),
			shared_state: std::sync::Mutex::new(shared_state),
			cancel_on_drop: std::sync::atomic::AtomicBool::new(true)
		}))
	}
	
	/// Sets the future
	pub fn set(&self, result: T) -> Result<(), Error<State>> {
		// Check if the future can be set (is `State::Waiting`)
		let mut payload = self.0.payload.lock().unwrap();
		if payload.0 != State::Waiting { throw_err!(payload.0) }
		
		// Set result
		*payload = (State::Ready, Some(result));
		self.0.cond_var.notify_all();
		Ok(())
	}
	
	/// Cancels (poisons) the future
	///
	/// This is useful to indicate that the future is obsolete and should not be `set` anymore
	pub fn cancel(&self) {
		let mut payload = self.0.payload.lock().unwrap();
		// Check if the payload is still cancelable
		if payload.0 == State::Waiting {
			payload.0 = State::Canceled;
			self.0.cond_var.notify_all();
		}
	}
	
	/// Returns the future's state
	pub fn get_state(&self) -> State {
		self.0.payload.lock().unwrap().0
	}
	
	/// Checks if the future is still waiting or has been set/canceled
	pub fn is_waiting(&self) -> bool {
		self.get_state() == State::Waiting
	}
	
	/// Tries to get the future's result
	///
	/// If the future is ready, it is consumed and `T` is returned;
	/// if the future is not ready, `Error::InvalidState(State)` is returned
	pub fn try_get(&self) -> Result<T, Error<State>> {
		// Lock this future and check if it has a result (is `State::Ready`)
		let payload = self.0.payload.lock().unwrap();
		Ok(try_err!(Future::<T, U>::extract_payload(payload)))
	}
	
	/// Tries to get the future's result
	///
	/// If the future is ready or or becomes ready before the timeout occurres, it is consumed
	/// and `T` is returned; if the future is not ready, `Error::InvalidState(State)` is returned
	pub fn try_get_timeout(&self, timeout: std::time::Duration) -> Result<T, Error<State>> {
		let timeout_point = std::time::Instant::now() + timeout;
		
		// Wait for condvar until the state is not `State::Waiting` anymore or the timeout has occurred
		let mut payload = self.0.payload.lock().unwrap();
		while payload.0 == State::Waiting && std::time::Instant::now() < timeout_point {
			payload = self.0.cond_var.wait_timeout(payload, time_remaining(timeout_point)).unwrap().0;
		}
		Ok(try_err!(Future::<T, U>::extract_payload(payload)))
	}
	
	/// Gets the future's result
	///
	/// __Warning: this function will block until a result becomes available__
	pub fn get(&self) -> Result<T, Error<State>> {
		// Wait for condvar until the state is not `State::Waiting` anymore
		let mut payload = self.0.payload.lock().unwrap();
		while payload.0 == State::Waiting { payload = self.0.cond_var.wait(payload).unwrap() }
		
		Ok(try_err!(Future::<T, U>::extract_payload(payload)))
	}
	
	/// Get a clone of the current shared state
	pub fn get_shared_state(&self) -> U where U: Clone {
		self.0.shared_state.lock().unwrap().clone()
	}
	
	/// Replace the current shared state
	pub fn set_shared_state(&self, shared_state: U) {
		*self.0.shared_state.lock().unwrap() = shared_state
	}
	
	/// Provides exclusive access to the shared state within `modifier` until `modifier` returns
	pub fn access_shared_state<F: FnOnce(&mut U)>(&self, modifier: F) {
		let mut shared_state_lock = self.0.shared_state.lock().unwrap();
		modifier(&mut *shared_state_lock);
	}
	
	/// Provides exclusive access to the shared state within `modifier` until `modifier` returns
	pub fn access_shared_state_param<V, F: FnOnce(&mut U, V)>(&self, modifier: F, parameter: V) {
		let mut shared_state_lock = self.0.shared_state.lock().unwrap();
		modifier(&mut *shared_state_lock, parameter);
	}
	
	/// Detaches the future so it won't be canceled if there is only one instance left
	///
	/// Useful if you either don't want that your future is ever canceled or if there's always only
	/// one instance (e.g. if you wrap it into a reference-counting container)
	pub fn detach(&self) {
		self.0.cancel_on_drop.store(false, std::sync::atomic::Ordering::Relaxed)
	}
	
	
	/// Internal helper to validate/update the future's state and get the payload
	fn extract_payload(mut payload: std::sync::MutexGuard<(State, Option<T>)>) -> Result<T, Error<State>> {
		// Validate state
		if payload.0 == State::Ready {
			// Update state and return the payload
			payload.0 = State::Consumed;
			// If the payload cannot be taken, we'll fall to `throw_err!(payload.0)` where `payload.0 == State::Consumed`
			if let Some(payload) = payload.1.take() { return Ok(payload) }
		}
		throw_err!(payload.0)
	}
}
impl<T> Future<T, ()> {
	pub fn new() -> Self {
		Future::with_state(())
	}
}
impl<T, U> Default for Future<T, U> where U: Default {
	fn default() -> Self {
		Future::with_state(U::default())
	}
}
impl<T, U> Drop for Future<T, U> {
	fn drop(&mut self) {
		if std::sync::Arc::strong_count(&self.0) <= 2 && self.0.cancel_on_drop.load(std::sync::atomic::Ordering::Relaxed) { self.cancel() }
	}
}
impl<T, U> Clone for Future<T, U> {
	fn clone(&self) -> Self {
		Future(self.0.clone())
	}
}
unsafe impl<T, U> Send for Future<T, U> {}
unsafe impl<T, U> Sync for Future<T, U> {}



/// Computes the remaining time underflow-safe
pub fn time_remaining(timeout_point: std::time::Instant) -> std::time::Duration {
	let now = std::time::Instant::now();
	if now > timeout_point { std::time::Duration::default() } else { timeout_point - now }
}



/// Creates a future for `job` and runs `job`. The result of `job` will be set as result into the
/// future. The parameter passed to `job` is a function that returns if the future is still waiting
/// so that `job` can check for cancellation.
pub fn async_with_state<T: 'static, U: 'static, F: FnOnce(Future<T, U>) + Send + 'static>(job: F, shared_state: U) -> Future<T, U> {
	use std::clone::Clone;
	
	// Create future and spawn job
	let future = Future::with_state(shared_state);
	let _future = future.clone();
	std::thread::spawn(move || job(_future));
	
	future
}

/// Creates a future for `job` and runs `job`. The result of `job` will be set as result into the
/// future. The parameter passed to `job` is a function that returns if the future is still waiting
/// so that `job` can check for cancellation.
pub fn async<T: 'static, F: FnOnce(Future<T, ()>) + Send + 'static>(job: F) -> Future<T, ()> {
	async_with_state(job, ())
}



/// Sets `$result` as the `$future`'s result and returns
#[macro_export]
macro_rules! job_return {
    ($future:expr, $result:expr) => ({
    	let _ = $future.set($result);
		return
	})
}

/// Cancels `$future` and returns
#[macro_export]
macro_rules! job_die {
    ($future:expr) => ({
    	$future.cancel();
    	return
    })
}



#[cfg(test)]
mod test {
	use std;
	use super::{ Future, State, async };
	
	#[test]
	fn double_set_err() {
		let fut = Future::<u8>::new();
		fut.set(7).unwrap();
		assert_eq!(fut.set(77).unwrap_err().kind, State::Ready)
	}
	
	#[test]
	fn cancel_set_err() {
		let fut = Future::<u8>::new();
		fut.cancel();
		assert_eq!(fut.set(7).unwrap_err().kind, State::Canceled)
	}
	
	#[test]
	fn drop_is_canceled() {
		let fut = Future::<u8>::new();
		assert_eq!(fut.get_state(), State::Waiting);
		{
			let _fut = fut.clone();
			std::thread::sleep(std::time::Duration::from_secs(2));
		}
		assert_eq!(fut.get_state(), State::Canceled)
	}
	
	#[test]
	fn cancel_get_err() {
		let fut = async(|fut: Future<u8>| {
			std::thread::sleep(std::time::Duration::from_secs(4));
			job_die!(fut)
		});
		assert_eq!(fut.get().unwrap_err().kind, State::Canceled)
	}
	
	#[test]
	fn is_ready_and_get() {
		let fut = async(|fut: Future<u8>| {
			std::thread::sleep(std::time::Duration::from_secs(4));
			fut.set(7).unwrap();
		});
		assert_eq!(fut.get_state(), State::Waiting);
		
		// Create and drop future
		{
			let _fut = fut.clone();
			std::thread::sleep(std::time::Duration::from_secs(7));
			assert_eq!(_fut.get_state(), State::Ready);
		}
		
		assert_eq!(fut.get().unwrap(), 7);
	}
}