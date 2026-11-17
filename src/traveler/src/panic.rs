//! The one panic boundary: a dependency's future awaited with each poll inside `catch_unwind`, so a panic the
//! driver or an HTTP client raises comes back as an error the caller reports (Rust.md "Panics"). Copied from
//! agent-memory's `core::panic`, without the hook's thread-local hand-off: this service installs no hook, so a
//! panic that reaches no boundary goes to the default hook's stderr line and ends that connection's task.

#![cfg_attr(
    not(test),
    deny(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::unreachable,
        clippy::todo,
        clippy::unimplemented,
        clippy::indexing_slicing,
        clippy::string_slice,
        clippy::arithmetic_side_effects,
        clippy::exit
    )
)]

use std::future::poll_fn;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::pin::pin;
use std::task::Poll;

/// Awaits `fut` with every poll inside a panic boundary. A caught panic is its payload's message.
///
/// `AssertUnwindSafe` is a claim: a panicked driver call's future is dropped and the shared `Client` behind it
/// may be in any state the driver allows. Our code reaches it only through calls that return a result, so a
/// damaged client makes later calls fail, which are reported like any other failure.
pub async fn caught<T>(fut: impl Future<Output = T>) -> Result<T, String> {
    let mut fut = pin!(fut);
    poll_fn(move |cx| match catch_unwind(AssertUnwindSafe(|| fut.as_mut().poll(cx))) {
        Ok(Poll::Pending) => Poll::Pending,
        Ok(Poll::Ready(value)) => Poll::Ready(Ok(value)),
        Err(payload) => Poll::Ready(Err(panic_detail(&payload))),
    })
    .await
}

/// A panic payload is `Box<dyn Any>`; `panic!` produces a `&str` or a `String`.
pub fn panic_detail(payload: &Box<dyn std::any::Any + Send>) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        (*s).to_owned()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "a panic payload of an unknown type".to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn caught_passes_a_completed_futures_output_through_unchanged() {
        assert_eq!(caught(async { Ok::<u32, &str>(7) }).await, Ok(Ok(7)));
        assert_eq!(caught(async { Err::<u32, &str>("database") }).await, Ok(Err("database")));
    }

    #[tokio::test]
    async fn caught_returns_the_message_of_a_panic_raised_after_an_await() {
        let outcome: Result<u32, String> = caught(async {
            tokio::task::yield_now().await;
            panic!("driver broke")
        })
        .await;
        assert_eq!(outcome, Err("driver broke".to_owned()));
    }
}
