//! Request cancellation reaches only the short-lived kernel handle registered by that request.

use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};

use crate::QueryError;

/// One caller controls both bounded source reads and the kernel computation for its request.
#[derive(Clone, Debug, Default)]
pub struct RequestCancellation {
    pub(crate) input: eska::analysis_core::Cancellation,
    pub(crate) query: QueryCancellation,
}

impl RequestCancellation {
    /// Stop source reading and the active kernel handle without cancelling other requests.
    pub fn cancel(&self) {
        self.input.cancel();
        self.query.cancel();
    }

    /// A transport may discard a result completed concurrently with cancellation.
    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.input.is_cancelled()
    }
}

/// One token controls one active query; cancelling it never cancels the session's other handles.
#[derive(Clone, Debug, Default)]
pub struct QueryCancellation(Arc<RequestState>);

#[derive(Debug, Default)]
struct RequestState {
    cancelled: AtomicBool,
    active: Mutex<Option<salsa::CancellationToken>>,
}

impl QueryCancellation {
    /// Cooperatively stop the registered computation at its next Salsa cancellation checkpoint.
    ///
    /// Cancellation is permanent for this token and all clones; there is no reset operation.
    pub fn cancel(&self) {
        self.0.cancelled.store(true, Ordering::Release);
        if let Some(active) = self
            .0
            .active
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .as_ref()
        {
            active.cancel();
        }
    }

    /// A transport can discard a late result after cancelling its request without changing inputs.
    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.0.cancelled.load(Ordering::Acquire)
    }

    /// Register inside the outer attach scope so Salsa retains cancellation until a checkpoint.
    pub(super) fn register(
        &self,
        token: salsa::CancellationToken,
    ) -> Result<QueryRegistration<'_>, QueryError> {
        let mut active = self
            .0
            .active
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        // The flag is checked under the lock: cancellation before/during registration cannot vanish.
        if self.is_cancelled() {
            return Err(QueryError::Cancelled);
        }
        if active.is_some() {
            return Err(QueryError::CancellationTokenInUse);
        }
        *active = Some(token);
        drop(active);
        Ok(QueryRegistration(self))
    }
}

/// Drop on success, cancellation or an unrelated panic; no database clone escapes the query.
pub struct QueryRegistration<'a>(&'a QueryCancellation);

impl Drop for QueryRegistration<'_> {
    /// Removing the handle avoids leaking cancellation into a later completed request.
    fn drop(&mut self) {
        self.0
            .0
            .active
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take();
    }
}
