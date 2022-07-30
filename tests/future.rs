use std::{thread, time::Duration};

#[test]
fn success() {
    let (setter, getter) = tiny_future::new::<u8>();

    // Set the result after one second
    thread::spawn(move || {
        thread::sleep(Duration::from_secs(1));
        setter.set(7);
    });

    // Wait unil the future is set
    assert_eq!(getter.wait(), Some(7), "Future has invalid result");
}

#[test]
fn success_timeout() {
    let (setter, getter) = tiny_future::new::<u8>();

    // Set the result after one second
    thread::spawn(move || {
        thread::sleep(Duration::from_secs(1));
        setter.set(7);
    });

    // Await the result
    let result = getter.wait_timeout(Duration::from_secs(2)).expect("Future has not been set in time");
    assert_eq!(result, Some(7), "Future has invalid result");
}

#[test]
fn timeout() {
    let (setter, getter) = tiny_future::new::<u8>();

    // Set the result after _two_ seconds
    thread::spawn(move || {
        thread::sleep(Duration::from_secs(2));
        setter.set(7);
    });

    // Await the result for _one_ second
    let result = getter.wait_timeout(Duration::from_secs(1));
    assert!(result.is_err(), "Future has been set too early");
}

#[test]
fn cancellation_setter() {
    let (setter, getter) = tiny_future::new::<u8>();

    // Drop the setter after one second
    thread::spawn(move || {
        thread::sleep(Duration::from_secs(1));
        drop(setter);
    });

    // Wait unil the future is cancelled
    assert_eq!(getter.wait(), None, "Future has not been marked as cancelled on drop");
}

#[test]
fn cancellation_setter_timeout() {
    let (setter, getter) = tiny_future::new::<u8>();

    // Drop the setter after one second
    thread::spawn(move || {
        thread::sleep(Duration::from_secs(1));
        drop(setter);
    });

    // Wait unil the future is cancelled
    let result = getter.wait_timeout(Duration::from_secs(2)).expect("Future has not been cancelled in time");
    assert_eq!(result, None, "Future has not been marked as cancelled on drop");
}

#[test]
fn cancellation_getter() {
    // Create futures
    let (setter, getter) = tiny_future::new::<u8>();

    // Drop the setter after one second
    thread::spawn(move || {
        thread::sleep(Duration::from_secs(1));
        drop(getter);
    });

    // Wait unil the future is cancelled
    thread::sleep(Duration::from_secs(2));
    assert!(setter.is_cancelled(), "Future has not been cancelled on drop");
}
