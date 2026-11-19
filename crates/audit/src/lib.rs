use std::io::Write;
use std::path::PathBuf;

use bookie_app_services::unknown_outcomes::{
    OutcomeDecision, UnknownOutcome, list_unknown_outcomes, list_unknown_outcomes_at, resolve_unknown_outcome_as,
    resolve_unknown_outcome_at,
};
use uuid::Uuid;

const SESSION: &str = "cli";
const USAGE: &str = "usage:\n  bookie-audit list\n  bookie-audit resolve <operation-id> <committed|rolled-back|still-unknown> --reason <text> [--checked-the-database]\n(the reason is recorded in the audit journal and is visible in the process list while the command runs; do not put secrets in it)\n";

pub trait Terminal {
    fn ask(&mut self, prompt: &str) -> Option<String>;
}

pub fn is_root(proc_status: &str) -> bool {
    proc_status
        .lines()
        .find_map(|line| line.strip_prefix("Uid:"))
        .and_then(|rest| rest.split_whitespace().nth(1))
        .is_some_and(|effective| effective == "0")
}

pub async fn run(
    arguments: &[String],
    journal: Option<PathBuf>,
    terminal: Option<&mut dyn Terminal>,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> i32 {
    if std::fs::read_to_string("/proc/self/status").is_ok_and(|status| is_root(&status)) {
        return fail(
            err,
            "bookie-audit refuses to run as root; run it as the user who owns the BookiE data",
            1,
        );
    }
    match arguments.first().map(String::as_str) {
        Some("list") if arguments.len() == 1 => list(journal, out, err).await,
        Some("resolve") => resolve(&arguments[1..], journal, terminal, out, err).await,
        _ => fail(err, USAGE, 2),
    }
}

fn fail(err: &mut dyn Write, message: &str, code: i32) -> i32 {
    let _ = err.write_all(message.as_bytes());
    if !message.ends_with('\n') {
        let _ = err.write_all(b"\n");
    }
    code
}

async fn list(journal: Option<PathBuf>, out: &mut dyn Write, err: &mut dyn Write) -> i32 {
    let listed = match journal {
        Some(path) => list_unknown_outcomes_at(path, None).await,
        None => list_unknown_outcomes(None).await,
    };
    match listed {
        Ok(items) if items.is_empty() => {
            let _ = writeln!(out, "No write outcome is waiting to be resolved.");
            0
        }
        Ok(items) => {
            for item in &items {
                let _ = out.write_all(describe(item).as_bytes());
            }
            0
        }
        Err(message) => fail(err, &format!("cannot read the audit journal: {message}"), 1),
    }
}

fn describe(item: &UnknownOutcome) -> String {
    let scope = if item.connection_id.is_nil() {
        "ALL CONNECTIONS (a global block)".to_string()
    } else {
        format!("{} ({})", item.connection_name, item.environment)
    };
    let error = item
        .error
        .as_deref()
        .map(|text| format!("  error: {text}\n"))
        .unwrap_or_default();
    format!(
        "{}\n  connection: {scope}\n  when: {}\n  class: {}\n  sql: {}\n{error}",
        item.operation_id, item.when, item.class, item.sql
    )
}

struct Resolve {
    operation_id: Uuid,
    decision: OutcomeDecision,
    reason: String,
    checked: bool,
}

fn parse_resolve(arguments: &[String]) -> Result<Resolve, String> {
    let operation_id = arguments
        .first()
        .and_then(|text| text.parse::<Uuid>().ok())
        .ok_or("the first argument must be an operation id from `bookie-audit list`")?;
    let decision = match arguments.get(1).map(String::as_str) {
        Some("committed") => OutcomeDecision::Committed,
        Some("rolled-back") => OutcomeDecision::RolledBack,
        Some("still-unknown") => OutcomeDecision::StillUnknown,
        _ => return Err("the second argument must be committed, rolled-back or still-unknown".into()),
    };
    let mut reason = None;
    let mut checked = false;
    let mut rest = arguments[2..].iter();
    while let Some(flag) = rest.next() {
        match flag.as_str() {
            "--reason" => reason = Some(rest.next().ok_or("--reason needs a value")?.clone()),
            "--checked-the-database" => checked = true,
            other => return Err(format!("unknown option {other}")),
        }
    }
    let reason = reason.ok_or("--reason is required; it is recorded in the audit journal")?;
    Ok(Resolve {
        operation_id,
        decision,
        reason,
        checked,
    })
}

async fn find(journal: &Option<PathBuf>, operation_id: Uuid) -> Result<Option<UnknownOutcome>, String> {
    let listed = match journal {
        Some(path) => list_unknown_outcomes_at(path.clone(), None).await,
        None => list_unknown_outcomes(None).await,
    }?;
    Ok(listed.into_iter().find(|item| item.operation_id == operation_id))
}

fn confirmation_code(operation_id: Uuid) -> String {
    let text = operation_id.simple().to_string();
    text[text.len() - 6..].to_string()
}

async fn confirm_clearing(
    request: &Resolve,
    journal: &Option<PathBuf>,
    terminal: Option<&mut dyn Terminal>,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> Result<(), i32> {
    let Some(terminal) = terminal else {
        return Err(fail(
            err,
            "this decision lifts a write block, so it needs a person at an interactive terminal; run it from a terminal, not a script",
            3,
        ));
    };
    let item = match find(journal, request.operation_id).await {
        Ok(Some(item)) => item,
        Ok(None) => return Err(fail(err, "no unresolved unknown outcome has that operation id", 1)),
        Err(message) => return Err(fail(err, &format!("cannot read the audit journal: {message}"), 1)),
    };
    let _ = out.write_all(describe(&item).as_bytes());
    let claim = if request.decision == OutcomeDecision::Committed {
        "WAS applied"
    } else {
        "was NOT applied"
    };
    let _ = writeln!(
        out,
        "You are recording that the write {claim}. This lifts the write block and cannot be undone; BookiE will not replay or re-check the write."
    );
    let code = confirmation_code(request.operation_id);
    let prompt = format!("Type {code} to confirm, anything else to cancel: ");
    match terminal.ask(&prompt) {
        Some(answer) if answer.trim() == code => Ok(()),
        _ => Err(fail(err, "cancelled; nothing was recorded", 1)),
    }
}

async fn resolve(
    arguments: &[String],
    journal: Option<PathBuf>,
    terminal: Option<&mut dyn Terminal>,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> i32 {
    let request = match parse_resolve(arguments) {
        Ok(request) => request,
        Err(message) => return fail(err, &format!("{message}\n{USAGE}"), 2),
    };
    if request.decision != OutcomeDecision::StillUnknown
        && let Err(code) = confirm_clearing(&request, &journal, terminal, out, err).await
    {
        return code;
    }
    let decided = match journal {
        Some(path) => {
            resolve_unknown_outcome_at(
                path,
                SESSION,
                request.operation_id,
                request.decision,
                &request.reason,
                request.checked,
            )
            .await
        }
        None => {
            resolve_unknown_outcome_as(
                SESSION,
                request.operation_id,
                request.decision,
                &request.reason,
                request.checked,
            )
            .await
        }
    };
    match decided {
        Ok(()) => {
            let _ = writeln!(out, "Decision recorded for {}.", request.operation_id);
            0
        }
        Err(message) => fail(err, &format!("cannot record the decision: {message}"), 1),
    }
}

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
