//use std;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum State {
	Waiting, Ready, Canceled
}

#[derive(Clone)]
pub struct Void;

/// A future
pub struct Future<T: 'static, U: 'static + Clone> {
	payload: std::sync::Mutex<(State, Option<T>)>,
	cond_var: std::sync::Condvar,
	shared_state: std::sync::Mutex<U>
}
impl<T: 'static, U: 'static + Clone> Future<T, U> {
	/// Creates an empty `Future<T>`
	pub fn new(initial_shared_state: U) -> Future<T, U> {
		Future {
			payload: std::sync::Mutex::new((State::Waiting, None)),
			cond_var: std::sync::Condvar::new(),
			shared_state: std::sync::Mutex::new(initial_shared_state)
		}
	}
	
	/// Puts a result into the future
	pub fn set(&self, result: T) -> Result<(), State> {
		// Check if the future can be set (is `State::Waiting`)
		let mut payload = self.payload.lock().expect("Failed to lock mutex");
		if payload.0 != State::Waiting { return Err(payload.0) }
		
		// Set result
		*payload = (State::Ready, Some(result));
		self.cond_var.notify_all();
		Ok(())
	}
	
	/// Cancels (poisons) the future
	///
	/// This is useful to indicate that the future is obsolete and should not be `set` anymore
	pub fn cancel(&self) {
		self.payload.lock().expect("Failed to lock mutex").0 = State::Canceled;
		self.cond_var.notify_all();
	}
	
	/// Returns the state of the future
	pub fn get_state(&self) -> State {
		self.payload.lock().expect("Failed to lock mutex").0
	}
	
	/// Checks if the future is still waiting or has been set/canceled
	pub fn is_waiting(&self) -> bool {
		self.get_state() == State::Waiting
	}
	
	/// Tries to get the future's result
	///
	/// If the future is ready, it is consumed and `T` is returned;
	/// if the future is not ready, `Error::InvalidState(State)` is returned
	pub fn try_get(&self) -> Result<T, State> {
		// Lock this future and check if it has a result (is `State::Ready`)
		let mut payload = self.payload.lock().expect("Failed to lock mutex");
		if payload.0 != State::Ready { Err(payload.0) }
			else { Ok(payload.1.take().expect("Failed to extract payload")) }
	}
	
	/// Tries to get the future's result
	///
	/// If the future is ready or or becomes ready before the timeout occurres, it is consumed
	/// and `T` is returned; if the future is not ready, `Error::InvalidState(State)` is returned
	pub fn try_get_timeout(&self, timeout: std::time::Duration) -> Result<T, State> {
		let start = std::time::Instant::now();
		
		// Wait for condvar until the state is not `State::Waiting` anymore or the timeout has occurred
		let mut payload = self.payload.lock().expect("Failed to lock mutex");
		while payload.0 == State::Waiting && start.elapsed() < timeout {
			payload = self.cond_var.wait_timeout(payload, time_remaining(start, timeout)).expect("Failed to lock mutex").0;
		}
		
		// Check if the future has a result (is `State::Ready`)
		if payload.0 != State::Ready { Err(payload.0) }
			else { Ok(payload.1.take().expect("Failed to extract result")) }
	}
	
	/// Gets the future's result
	///
	/// __Warning: this function will block until a result becomes available__
	pub fn get(&self) -> Result<T, State> {
		// Wait for condvar until the state is not `State::Waiting` anymore
		let mut payload = self.payload.lock().expect("Failed to lock mutex");
		while payload.0 == State::Waiting { payload = self.cond_var.wait(payload).expect("Failed to lock mutex") }
		
		// Check if the future has a result (is `State::Ready`)
		if payload.0 != State::Ready { Err(payload.0) }
			else { Ok(payload.1.take().expect("Failed to extract result")) }
	}
	
	/// Get a clone of the current shared state
	pub fn get_shared_state(&self) -> U {
		self.shared_state.lock().expect("Failed to lock mutex").clone()
	}
	
	/// Replace the current shared state
	pub fn set_shared_state(&self, shared_state: U) {
		*self.shared_state.lock().expect("Failed to lock mutex") = shared_state
	}
	
	/// Provides exclusive access to the shared state for `modifier` until this function returns
	///
	/// The return value of `modifier` will become the new shared state
	pub fn access_shared_state<V>(&self, modifier: &Fn(U, V) -> U, parameter: V) {
		let mut shared_state_lock = self.shared_state.lock().expect("Failed to lock mutex");
		*shared_state_lock = modifier(*shared_state_lock, parameter);
	}
}
unsafe impl<T, U: Clone> Send for Future<T, U> {}
unsafe impl<T, U: Clone> Sync for Future<T, U> {}



/// Computes the remaining time underflow-safe
fn time_remaining(start: std::time::Instant, timeout: std::time::Duration) -> std::time::Duration {
	let elapsed = start.elapsed();
	if elapsed < timeout { timeout - elapsed } else { std::time::Duration::default() }
}



/// Creates a future for `job` and runs `job`. The result of `job` will be set as result into the future.
/// The parameter passed to `job` is a function that returns if the future is still waiting
/// so that `job` can check for cancellation.
pub fn run_job<T: 'static, U: 'static + Clone, F: FnOnce(&Future<T, U>) + Send + 'static>(job: F, initial_shared_state: U) -> std::sync::Arc<Future<T, U>> {
	// Create future
	let future = std::sync::Arc::new(Future::new(initial_shared_state));
	
	// Spawn job
	let _future = future.clone();
	std::thread::spawn(move || job(&_future));
	
	future
}

#[macro_export]
macro_rules! job_return {
    ($future:expr, $result:expr) => ({
    	let _ = $future.set($result);
		return
	})
}

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
	
	#[test] #[should_panic]
	fn double_set_panic() {
		let future = std::sync::Arc::new(super::Future::<u8, super::Void>::new(super::Void));
		let _future = future.clone();
		
		std::thread::spawn(move || {
			future.set(7).expect("Failed to set future");
			future.set(77).expect("Failed to set future");
		}).join().expect("Failed to join thread");
	}
	
	#[test] #[should_panic]
	fn cancel_set_panic() {
		let future = std::sync::Arc::new(super::Future::<u8, super::Void>::new(super::Void));
		let _future = future.clone();
		
		future.cancel();
		std::thread::spawn(move || _future.set(7).expect("Failed to set future")).join().expect("Failed to join thread");
	}
	
	#[test] #[should_panic(expected = "Failed to get future: Canceled")]
	fn cancel_get_panic() {
		let future = std::sync::Arc::new(super::Future::<u8, super::Void>::new(super::Void));
		let _future = future.clone();
		
		std::thread::spawn(move || {
			std::thread::sleep(std::time::Duration::from_secs(3));
			_future.cancel()
		});
		future.get().expect("Failed to get future");
	}
	
	#[test]
	fn is_ready() {
		let future = std::sync::Arc::new(super::Future::<u8, super::Void>::new(super::Void));
		let _future = future.clone();
		
		assert_eq!(future.get_state(), super::State::Waiting);
		
		std::thread::spawn(move || _future.set(7).expect("Failed to set future")).join().expect("Failed to join thread");
		assert_eq!(future.get_state(), super::State::Ready);
	}
	
	#[test]
	fn get() {
		let future = std::sync::Arc::new(super::Future::<u8, super::Void>::new(super::Void));
		let _future = future.clone();
		
		std::thread::spawn(move || {
			std::thread::sleep(std::time::Duration::from_secs(3));
			_future.set(7).expect("Failed to set future");
		});
		assert_eq!(future.get().expect("Failed to get future"), 7)
	}
	
	#[test]
	fn try_get() {
		let future = std::sync::Arc::new(super::Future::<u8, super::Void>::new(super::Void));
		let _future = future.clone();
		
		assert_eq!(future.try_get().expect_err("Failed to get error"), super::State::Waiting);
		
		std::thread::spawn(move || _future.set(7).expect("Failed to set future")).join().expect("Failed to join thread");
		assert_eq!(future.try_get().expect("Failed to get future"), 7);
	}
	
	#[test]
	fn try_get_timeout() {
		let future = std::sync::Arc::new(super::Future::<u8, super::Void>::new(super::Void));
		let _future = future.clone();
		
		assert_eq!(future.try_get_timeout(std::time::Duration::from_secs(3)).expect_err("Failed to get error"), super::State::Waiting);
		
		std::thread::spawn(move || {
			std::thread::sleep(std::time::Duration::from_secs(2));
			_future.set(7).expect("Failed to set future")
		});
		assert_eq!(future.try_get_timeout(std::time::Duration::from_secs(5)).expect("Failed to get future"), 7);
	}
	
	#[test]
	fn test_job() {
		let future = super::run_job(|future: &super::Future<u8, super::Void>| {
			std::thread::sleep(std::time::Duration::from_secs(4));
			job_return!(future, 7);
		}, super::Void);
		
		assert_eq!(future.try_get_timeout(std::time::Duration::from_secs(7)).expect("Failed to get future"), 7);
	}
	
	#[test]
	fn test_job_cancellation() {
		let control_flag = std::sync::Arc::new(std::sync::Mutex::new(false));
		
		let _control_flag = control_flag.clone();
		let future = super::run_job(move |future: &super::Future<(), super::Void>| {
			std::thread::sleep(std::time::Duration::from_secs(2));
			if !future.is_waiting() { job_die!(future) }
			
			std::thread::sleep(std::time::Duration::from_secs(7));
			if future.is_waiting() { job_die!(future) }
			
			*_control_flag.lock().expect("Failed to lock mutex") = true;
			job_die!(future)
		}, super::Void);
		
		std::thread::sleep(std::time::Duration::from_secs(4));
		future.cancel();
		
		std::thread::sleep(std::time::Duration::from_secs(7));
		assert_eq!(*control_flag.lock().expect("Failed to lock mutex"), true);
	}
}