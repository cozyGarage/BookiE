use std::future::Future;
use std::sync::OnceLock;

use tokio::runtime::{Builder, Handle, Runtime};

static RUNTIME: OnceLock<Result<Runtime, std::io::Error>> = OnceLock::new();

pub fn block_on<T>(work: impl Future<Output = Result<T, String>>) -> Result<T, String> {
    if Handle::try_current().is_ok() {
        return Err("Application services must be awaited from a Tokio runtime".to_owned());
    }
    RUNTIME
        .get_or_init(|| Builder::new_multi_thread().worker_threads(2).enable_all().build())
        .as_ref()
        .map_err(|error| error.to_string())?
        .block_on(work)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_nested_blocking_call_returns_an_error_without_running_the_work() {
        let called = std::cell::Cell::new(false);
        let result = block_on(async {
            called.set(true);
            Ok(42)
        });
        assert!(result.is_err());
        assert!(!called.get());
    }

    #[test]
    fn background_callers_can_run_timed_async_work() {
        let result = block_on(async {
            tokio::time::sleep(std::time::Duration::ZERO).await;
            Ok(42)
        });
        assert_eq!(result, Ok(42));
    }
}
