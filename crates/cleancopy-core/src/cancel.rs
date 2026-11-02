//! Cooperative cancellation for long-running renders.
//!
//! A worker installs a token with [`run`]; renderers call [`cancelled`] at
//! block boundaries and stop early. Partial output produced after cancellation
//! is never returned: [`run`] yields `None` instead.

use std::cell::RefCell;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

thread_local! {
    static TOKEN: RefCell<Option<Arc<AtomicBool>>> = const { RefCell::new(None) };
}

/// True when the current thread's render was cancelled.
pub fn cancelled() -> bool {
    TOKEN.with(|t| {
        t.borrow()
            .as_ref()
            .is_some_and(|flag| flag.load(Ordering::Relaxed))
    })
}

/// Run `f` with `token` installed. Returns `None` when cancellation was
/// observed before or after `f`, discarding any partial result.
pub fn run<T>(token: &Arc<AtomicBool>, f: impl FnOnce() -> T) -> Option<T> {
    if token.load(Ordering::Relaxed) {
        return None;
    }
    let previous = TOKEN.with(|t| t.replace(Some(token.clone())));
    let result = f();
    TOKEN.with(|t| *t.borrow_mut() = previous);
    (!token.load(Ordering::Relaxed)).then_some(result)
}
