use std::time::Duration;

use bookie_core::{DriverError, Interruption};
use tokio::time::Instant;

use crate::MssqlClient;

pub(crate) async fn wait_for_deadline(deadline: Option<Instant>) {
    match deadline {
        Some(deadline) => tokio::time::sleep_until(deadline).await,
        None => std::future::pending().await,
    }
}

pub(crate) async fn send_attention(
    client: &mut MssqlClient,
    interruption: Interruption,
    dispatch_timeout: Duration,
) -> DriverError {
    match tokio::time::timeout(dispatch_timeout, client.cancel_query()).await {
        Ok(Ok(())) => interruption.into_error(),
        _ => DriverError::OperationOutcomeUnknown {
            source: Box::new(interruption.into_error()),
        },
    }
}

macro_rules! with_attention {
    ($client:ident, $control:expr, $operation:expr) => {{
        let control: &bookie_core::OperationControl = $control;
        bookie_core::check_pre_dispatch(control)?;
        let interruption = {
            let mut running = std::pin::pin!($operation);
            tokio::select! {
                biased;
                result = &mut running => return result,
                () = control.cancellation_token().cancelled() => bookie_core::Interruption::Cancelled,
                () = $crate::attention::wait_for_deadline(control.deadline()) => bookie_core::Interruption::TimedOut,
            }
        };
        Err(
            $crate::attention::send_attention(&mut $client, interruption, bookie_core::CANCELLATION_DISPATCH_TIMEOUT)
                .await,
        )
    }};
}
pub(crate) use with_attention;
