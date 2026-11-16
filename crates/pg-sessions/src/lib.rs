use bookie_core::{Connection, DriverError, Value};

pub const SESSIONS_SQL: &str = "SELECT activity.pid::pg_catalog.int8, activity.usename::pg_catalog.text, activity.datname::pg_catalog.text, activity.state::pg_catalog.text, \
activity.wait_event_type::pg_catalog.text, activity.wait_event::pg_catalog.text, \
COALESCE(EXTRACT(EPOCH FROM (pg_catalog.now() - activity.query_start)), 0)::pg_catalog.int8, \
pg_catalog.array_to_string(pg_catalog.pg_blocking_pids(activity.pid), ','), pg_catalog.left(activity.query, 2000) \
FROM pg_catalog.pg_stat_activity AS activity \
WHERE activity.backend_type = 'client backend' AND activity.pid <> pg_catalog.pg_backend_pid() \
ORDER BY activity.pid";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Session {
    pub pid: i64,
    pub user: Option<String>,
    pub database: Option<String>,
    pub state: Option<String>,
    pub wait: Option<String>,
    pub running_secs: i64,
    pub blocked_by: Vec<i64>,
    pub query: String,
}

pub async fn list_sessions(connection: &dyn Connection) -> Result<Vec<Session>, DriverError> {
    let result = connection.query(SESSIONS_SQL).await?;
    Ok(result.rows.iter().filter_map(parse_row).collect())
}

fn text(value: &Value) -> Option<String> {
    match value {
        Value::Text(text) if !text.is_empty() => Some(text.clone()),
        _ => None,
    }
}

fn parse_row(row: &[Value]) -> Option<Session> {
    let [
        Value::Int(pid),
        user,
        database,
        state,
        wait_type,
        wait_event,
        Value::Int(running_secs),
        blockers,
        query,
    ] = row
    else {
        return None;
    };
    let wait = match (text(wait_type), text(wait_event)) {
        (Some(kind), Some(event)) => Some(format!("{kind}: {event}")),
        (kind, event) => kind.or(event),
    };
    let blocked_by = text(blockers)
        .map(|list| list.split(',').filter_map(|pid| pid.trim().parse().ok()).collect())
        .unwrap_or_default();
    Some(Session {
        pid: *pid,
        user: text(user),
        database: text(database),
        state: text(state),
        wait,
        running_secs: *running_secs,
        blocked_by,
        query: text(query).unwrap_or_default(),
    })
}

pub fn blockers(sessions: &[Session]) -> Vec<(i64, usize)> {
    let mut counts: Vec<(i64, usize)> = Vec::new();
    for blocker in sessions.iter().flat_map(|session| &session.blocked_by) {
        match counts.iter_mut().find(|(pid, _)| pid == blocker) {
            Some((_, waiting)) => *waiting += 1,
            None => counts.push((*blocker, 1)),
        }
    }
    counts.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    counts
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(pid: i64, blockers: &str, wait: (&str, &str)) -> Vec<Value> {
        let text = |s: &str| {
            if s.is_empty() {
                Value::Null
            } else {
                Value::Text(s.into())
            }
        };
        vec![
            Value::Int(pid),
            text("app"),
            text("shop"),
            text("active"),
            text(wait.0),
            text(wait.1),
            Value::Int(7),
            text(blockers),
            text("SELECT 1"),
        ]
    }

    #[test]
    fn a_row_becomes_a_session_with_its_blockers_and_wait() {
        let session = parse_row(&row(42, "10,11", ("Lock", "transactionid"))).expect("parses");
        assert_eq!(session.pid, 42);
        assert_eq!(session.blocked_by, [10, 11]);
        assert_eq!(session.wait.as_deref(), Some("Lock: transactionid"));
        assert_eq!(session.running_secs, 7);
        assert_eq!(parse_row(&row(1, "", ("", ""))).unwrap().blocked_by, Vec::<i64>::new());
        assert_eq!(parse_row(&row(1, "", ("", ""))).unwrap().wait, None);
    }

    #[test]
    fn a_row_of_the_wrong_shape_is_skipped_not_guessed() {
        assert!(parse_row(&[Value::Int(1)]).is_none());
    }

    #[test]
    fn blockers_are_ranked_by_how_many_sessions_wait_on_them() {
        let sessions: Vec<Session> = [(1, "9"), (2, "9,8"), (3, "8"), (4, "9"), (5, "")]
            .iter()
            .map(|(pid, list)| parse_row(&row(*pid, list, ("", ""))).unwrap())
            .collect();
        assert_eq!(blockers(&sessions), [(9, 3), (8, 2)]);
    }
}
