use cleancopy_core::{cancel, render_numbered, render_plain, render_rtf, NumberingStyle};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

fn big() -> String {
    (0..40_000)
        .map(|i| format!("## H{i}\n\n- **item** {i}\n  - nested `code`\n\n"))
        .collect()
}

#[test]
fn uncancelled_run_returns_the_same_output() {
    let token = Arc::new(AtomicBool::new(false));
    let input = "# T\n\n- a\n- b\n";
    assert_eq!(
        cancel::run(&token, || render_plain(input)),
        Some(render_plain(input))
    );
}

#[test]
fn pre_cancelled_run_returns_none_without_rendering() {
    let token = Arc::new(AtomicBool::new(true));
    let mut ran = false;
    assert_eq!(cancel::run(&token, || ran = true), None);
    assert!(!ran);
}

#[test]
fn renderers_stop_at_block_boundaries_once_cancelled() {
    // Cancel from inside the closure (after parse would start) and check the
    // renderers produce truncated output instead of walking every block.
    let input = big();
    for render in [
        (|s: &str| render_plain(s)) as fn(&str) -> String,
        |s: &str| render_numbered(s, NumberingStyle::Chinese),
        |s: &str| render_rtf(s),
    ] {
        let full = render(&input);
        let token = Arc::new(AtomicBool::new(false));
        let inner = token.clone();
        let partial_len = std::cell::Cell::new(usize::MAX);
        let result = cancel::run(&token, || {
            inner.store(true, Ordering::Relaxed);
            partial_len.set(render(&input).len());
        });
        assert_eq!(result, None, "cancelled run must not publish output");
        assert!(
            partial_len.get() < full.len() / 10,
            "renderer kept working after cancellation: {} of {}",
            partial_len.get(),
            full.len()
        );
    }
}
