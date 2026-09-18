use std::any::Any;
use std::future::Future;
use std::panic::AssertUnwindSafe;

use futures::FutureExt;
use tablepro_core::DriverError;

use super::PolicyGuard;

impl PolicyGuard {
    pub(crate) async fn caught_read<T, F>(&self, operation: &'static str, execute: F) -> Result<T, DriverError>
    where
        F: Future<Output = Result<T, DriverError>>,
    {
        match AssertUnwindSafe(execute).catch_unwind().await {
            Ok(result) => result,
            Err(payload) => Err(DriverError::Internal(self.report(operation, &payload))),
        }
    }

    pub(crate) async fn caught_write<T, F>(&self, operation: &'static str, execute: F) -> Result<T, DriverError>
    where
        F: Future<Output = Result<T, DriverError>>,
    {
        match AssertUnwindSafe(execute).catch_unwind().await {
            Ok(result) => result,
            Err(payload) => Err(DriverError::OperationOutcomeUnknown {
                source: Box::new(DriverError::Internal(self.report(operation, &payload))),
            }),
        }
    }

    fn report(&self, operation: &'static str, payload: &Box<dyn Any + Send>) -> String {
        let detail = payload
            .downcast_ref::<&str>()
            .map(|text| (*text).to_owned())
            .or_else(|| payload.downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "no panic message".to_owned());
        tracing::error!(operation, detail, "driver panicked; reporting it as a failed operation");
        if let Some(fault) = &self.fault {
            fault.connection_became_unusable(operation);
        }
        format!("the driver stopped unexpectedly during {operation}")
    }
}
