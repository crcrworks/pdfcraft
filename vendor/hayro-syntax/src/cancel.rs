//! PdfCraft patch: (hayro-syntax (46)) cancellation scoped to one PDF open, with no
//! thread-local state, as `craft-libs`' reader states it. Only the entry points of a load
//! carry it here: `craft-libs` also wires checkpoints through `Reader`, `ReaderContext` and
//! the filters, which this port does not take.

use crate::sync::{Arc, Mutex, MutexExt};
use core::fmt;

/// An owned cancellation callback, called synchronously on the opening thread.
///
/// Capture shared state, such as an `Arc<AtomicBool>`, to request cancellation from another
/// thread. A request is latched separately for each open. Parsed PDFs do not retain or call
/// the callback after that open returns.
#[derive(Clone)]
pub struct Cancellation(Arc<dyn Fn() -> bool + Send + Sync>);

impl Cancellation {
    /// Own a callback that returns true when the current operation should stop.
    pub fn new(check: impl Fn() -> bool + Send + Sync + 'static) -> Self {
        Self(Arc::new(check))
    }

    /// Poll the callback. An opening operation latches a true result in its own control.
    pub fn requested(&self) -> bool {
        (self.0)()
    }
}

impl fmt::Debug for Cancellation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Cancellation").finish_non_exhaustive()
    }
}

#[derive(Debug)]
struct State {
    callback: Option<Cancellation>,
    stopped: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct Control(Arc<Inner>);

#[derive(Debug)]
struct Inner {
    state: Mutex<State>,
    #[cfg(feature = "std")]
    active: core::sync::atomic::AtomicBool,
    #[cfg(not(feature = "std"))]
    active: core::cell::Cell<bool>,
}

impl Control {
    pub(crate) fn new(callback: &Cancellation) -> Self {
        Self(Arc::new(Inner {
            state: Mutex::new(State {
                callback: Some(callback.clone()),
                stopped: false,
            }),
            #[cfg(feature = "std")]
            active: core::sync::atomic::AtomicBool::new(true),
            #[cfg(not(feature = "std"))]
            active: core::cell::Cell::new(true),
        }))
    }

    pub(crate) fn check(&self) -> Option<()> {
        if !self.active() {
            return Some(());
        }
        // Never invoke caller code under the cancellation-state lock.
        let callback = {
            let state = self.0.state.get();
            if state.stopped {
                return None;
            }
            state.callback.clone()
        };
        if callback.is_some_and(|c| c.requested()) {
            self.0.state.get().stopped = true;
            None
        } else {
            Some(())
        }
    }

    fn active(&self) -> bool {
        #[cfg(feature = "std")]
        {
            self.0.active.load(core::sync::atomic::Ordering::Relaxed)
        }
        #[cfg(not(feature = "std"))]
        {
            self.0.active.get()
        }
    }

    fn finish(&self) {
        let callback = self.0.state.get().callback.take();
        #[cfg(feature = "std")]
        self.0
            .active
            .store(false, core::sync::atomic::Ordering::Relaxed);
        #[cfg(not(feature = "std"))]
        self.0.active.set(false);
        drop(callback);
    }
}

pub(crate) fn check(control: Option<&Control>) -> Option<()> {
    control.map_or(Some(()), Control::check)
}

/// Remove callback captures on success, failure and unwinding.
pub(crate) struct Scope(pub(crate) Control);

impl Drop for Scope {
    fn drop(&mut self) {
        self.0.finish();
    }
}
