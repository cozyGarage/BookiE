use std::any::Any;
use std::future::Future;
use std::panic::AssertUnwindSafe;

use futures::FutureExt;
use tablepro_core::DriverError;

pub(crate) async fn caught_read<T, F>(operation: &'static str, execute: F) -> Result<T, DriverError>
where
    F: Future<Output = Result<T, DriverError>>,
{
    match AssertUnwindSafe(execute).catch_unwind().await {
        Ok(result) => result,
        Err(payload) => Err(DriverError::Internal(report(operation, &payload))),
    }
}

pub(crate) async fn caught_write<T, F>(operation: &'static str, execute: F) -> Result<T, DriverError>
where
    F: Future<Output = Result<T, DriverError>>,
{
    match AssertUnwindSafe(execute).catch_unwind().await {
        Ok(result) => result,
        Err(payload) => Err(DriverError::OperationOutcomeUnknown {
            source: Box::new(DriverError::Internal(report(operation, &payload))),
        }),
    }
}

fn report(operation: &'static str, payload: &Box<dyn Any + Send>) -> String {
    let detail = payload
        .downcast_ref::<&str>()
        .map(|text| (*text).to_owned())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "no panic message".to_owned());
    tracing::error!(operation, detail, "driver panicked; reporting it as a failed operation");
    format!("the driver stopped unexpectedly during {operation}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_panicking_read_becomes_an_internal_error() {
        let result: Result<(), DriverError> = caught_read("LIST TABLES", async { panic!("codec boom") }).await;
        let Err(DriverError::Internal(message)) = result else {
            panic!("expected an internal error");
        };
        assert!(message.contains("LIST TABLES"));
        assert!(!message.contains("codec boom"));
    }

    #[tokio::test]
    async fn a_panicking_write_reports_an_unknown_outcome() {
        let result: Result<(), DriverError> = caught_write("EXECUTE", async { panic!("codec boom") }).await;
        let Err(DriverError::OperationOutcomeUnknown { source }) = result else {
            panic!("expected an unknown outcome");
        };
        assert!(matches!(*source, DriverError::Internal(_)));
    }

    #[tokio::test]
    async fn a_successful_operation_passes_through_untouched() {
        let result = caught_read("LIST TABLES", async { Ok::<u8, DriverError>(7) }).await;
        assert_eq!(result.ok(), Some(7));
    }

    #[tokio::test]
    async fn an_ordinary_driver_error_is_not_rewritten() {
        let result: Result<(), DriverError> =
            caught_read("LIST TABLES", async { Err(DriverError::ConnectionRefused) }).await;
        assert!(matches!(result, Err(DriverError::ConnectionRefused)));
    }
}
