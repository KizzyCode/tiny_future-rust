[![License](https://img.shields.io/badge/License-BSD--2--Clause-blue.svg)](https://opensource.org/licenses/BSD-2-Clause)
[![License](https://img.shields.io/badge/License-MIT-blue.svg)](https://opensource.org/licenses/MIT)
[![AppVeyor CI](https://ci.appveyor.com/api/projects/status/github/KizzyCode/tiny-future-rust?svg=true)](https://ci.appveyor.com/project/KizzyCode/tiny-future-rust)
[![docs.rs](https://docs.rs/tiny_future/badge.svg)](https://docs.rs/tiny_future)
[![crates.io](https://img.shields.io/crates/v/tiny_future.svg)](https://crates.io/crates/tiny_future)
[![Download numbers](https://img.shields.io/crates/d/tiny_future.svg)](https://crates.io/crates/tiny_future)
[![dependency status](https://deps.rs/crate/tiny_future/0.5.1/status.svg)](https://deps.rs/crate/tiny_future/0.5.1)

# `tiny_future`
Welcome to `tiny_future` 🎉

This library provides a simple, `Condvar` based future with automatic cancellation on drop.
 
## Example
```rust
use std::{thread, time::Duration};

// Create futures
let (setter, getter) = tiny_future::new::<u8>();

// Run something in a separate task
thread::spawn(move || {
    // Do some computations and set the result
    thread::sleep(Duration::from_secs(1));
    setter.set(7);
});

// Wait unil the result is available
let result = getter.wait().expect("Failed to compute result");
assert_eq!(result, 7);
```
