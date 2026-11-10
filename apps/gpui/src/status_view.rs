use bookie_app_services::database_service::{ConnectionHealth, ConnectionOverview, TunnelState};

use crate::messages::MessageKind;

pub fn describe(overview: &[ConnectionOverview]) -> Vec<(MessageKind, String)> {
    if overview.is_empty() {
        return vec![(MessageKind::Info, "no open connection".into())];
    }
    overview
        .iter()
        .map(|connection| {
            let (kind, health) = match &connection.health {
                ConnectionHealth::Healthy => (MessageKind::Success, "healthy".to_string()),
                ConnectionHealth::Reconnecting { attempt } => {
                    (MessageKind::Info, format!("reconnecting, attempt {attempt}"))
                }
                ConnectionHealth::Failed { reason } => (MessageKind::Error, format!("failed · {reason:?}")),
            };
            let tunnel = match connection.tunnel {
                TunnelState::Direct => "direct",
                TunnelState::Open => "through an SSH tunnel (open)",
                TunnelState::Closed => "through an SSH tunnel (closed)",
            };
            let kind = if connection.tunnel == TunnelState::Closed {
                MessageKind::Error
            } else {
                kind
            };
            (
                kind,
                format!("{} · {} · {health} · {tunnel}", connection.name, connection.driver_id),
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use bookie_core::Environment;

    fn connection(health: ConnectionHealth, tunnel: TunnelState) -> ConnectionOverview {
        ConnectionOverview {
            name: "prod db".into(),
            driver_id: "postgres".into(),
            environment: Environment::Prod,
            health,
            tunnel,
        }
    }

    #[test]
    fn a_healthy_direct_connection_reads_as_success() {
        let lines = describe(&[connection(ConnectionHealth::Healthy, TunnelState::Direct)]);
        assert_eq!(
            lines,
            [(MessageKind::Success, "prod db · postgres · healthy · direct".into())]
        );
    }

    #[test]
    fn a_closed_tunnel_and_a_reconnect_are_called_out() {
        let closed = describe(&[connection(ConnectionHealth::Healthy, TunnelState::Closed)]);
        assert_eq!(closed[0].0, MessageKind::Error);
        assert!(closed[0].1.ends_with("through an SSH tunnel (closed)"));
        let retrying = describe(&[connection(
            ConnectionHealth::Reconnecting { attempt: 2 },
            TunnelState::Open,
        )]);
        assert_eq!(retrying[0].0, MessageKind::Info);
        assert!(retrying[0].1.contains("reconnecting, attempt 2"));
    }

    #[test]
    fn no_open_connection_says_so() {
        assert_eq!(describe(&[]), [(MessageKind::Info, "no open connection".into())]);
    }
}
