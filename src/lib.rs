#![doc = include_str!("../README.md")]

mod future;

use crate::future::Future;
pub use crate::future::{Getter, Setter};
use std::sync::{atomic::AtomicBool, Arc};

/// Creates a new future
pub fn new<T>() -> (Setter<T>, Getter<T>) {
    // Create the inner cell
    let future = Arc::new(Future::new());
    let cancelled = Arc::new(AtomicBool::default());

    // Create the setter/getter pair
    let setter = Setter::new(future.clone(), cancelled.clone());
    let getter = Getter::new(future, cancelled);
    (setter, getter)
}
