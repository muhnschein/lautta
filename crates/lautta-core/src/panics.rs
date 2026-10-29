// SPDX-License-Identifier: LGPL-2.1-or-later
//! Panics of PDF drawing as errors (SPEC RS-4, PRV-12, PRV-14).
//!
//! Release builds unwind, but only inside [`catch`]: the hook that
//! [`abort_outside_catch`] installs ends the process on any other panic
//! before it unwinds, as `panic = "abort"` would. So a PDF that trips hayro
//! or the code around it fails that one draw and the app goes on, while a
//! bug anywhere else still stops the app at once: no task dies alone, no
//! state is left half changed, nothing unwinds into C++.
//!
//! Not caught: stack overflow, running out of memory, an abort; they end
//! the process whatever the hook says.

use std::cell::Cell;
use std::panic::{catch_unwind, AssertUnwindSafe};

thread_local! {
    /// This thread runs inside [`catch`].
    static CATCHING: Cell<bool> = const { Cell::new(false) };
}

/// A panic that [`catch`] stopped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Panicked;

/// Runs `work`; a panic in it ends here as [`Panicked`]. Only for work that
/// owns what it changes, or whose caller drops what it shared with it after
/// a panic: values `work` was changing may be left half done.
pub fn catch<T>(work: impl FnOnce() -> T) -> Result<T, Panicked> {
    let outer = CATCHING.replace(true);
    let done = catch_unwind(AssertUnwindSafe(work));
    CATCHING.set(outer);
    done.map_err(|_| Panicked)
}

/// Installs the panic hook of the app: the panic is reported as before
/// (stderr), then the process aborts unless the panic is inside [`catch`].
/// Called once at start, by the binary only: tests keep the default hook.
pub fn abort_outside_catch() {
    let report = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        report(info);
        if !CATCHING.get() {
            std::process::abort();
        }
    }));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn work_that_returns_gives_its_value() {
        assert_eq!(catch(|| 42), Ok(42));
        assert!(!CATCHING.get(), "outside again");
    }

    #[test]
    fn a_panic_ends_in_the_catch() {
        let caught = catch(|| -> u8 { panic!("render") });
        assert_eq!(caught, Err(Panicked));
        assert!(!CATCHING.get(), "outside again after a panic too");
    }

    #[test]
    fn the_work_runs_marked_as_caught_and_nesting_keeps_the_mark() {
        let inside = catch(|| {
            let inner = catch(|| CATCHING.get());
            (inner, CATCHING.get())
        });
        assert_eq!(
            inside,
            Ok((Ok(true), true)),
            "an inner catch leaves the outer one marked"
        );
    }

    #[test]
    fn the_mark_is_per_thread() {
        let other = catch(|| std::thread::spawn(|| CATCHING.get()).join().unwrap());
        assert_eq!(other, Ok(false), "a thread started inside is not caught");
    }
}
