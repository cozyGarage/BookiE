use tablepro_core::Environment;

use crate::classify::{StatementClass, StatementFacts};
use crate::config::EnvPolicy;
use crate::effects::Effects;
use crate::principal::Principal;
use crate::transaction_control::{
    TransactionControl, has_implicit_transaction_starter, leaves_tsql_transaction_unclosed,
    mysql_statement_is_transaction_safe, transaction_control,
};
pub use crate::verdict::Decision;

pub fn evaluate(
    principal: &Principal,
    environment: Environment,
    facts: &StatementFacts,
    connection_read_only: bool,
    env_policy: &EnvPolicy,
    estimated_rows: Option<u64>,
) -> Decision {
    evaluate_with_effects(
        principal,
        environment,
        facts,
        connection_read_only,
        env_policy,
        estimated_rows,
    )
}

pub(crate) fn evaluate_with_effects(
    principal: &Principal,
    environment: Environment,
    facts: &StatementFacts,
    connection_read_only: bool,
    env_policy: &EnvPolicy,
    estimated_rows: Option<u64>,
) -> Decision {
    let effects = facts.effects;
    let mut decisions = evaluate_categorical_decisions(principal, environment, facts, connection_read_only, env_policy);
    if effects.writes()
        || facts.class == StatementClass::Unparseable
        || effects.contains(Effects::ADMIN)
        || effects.contains(Effects::HOST_OR_FILE_ACCESS)
        || effects.contains(Effects::UNKNOWN)
    {
        decisions.extend(evaluate_eligible_write_decisions(
            principal,
            environment,
            facts,
            env_policy,
            estimated_rows,
        ));
    }

    Decision::join_all(decisions).unwrap_or_else(|| Decision::Deny {
        rule: "no_policy_rules_applied".into(),
        message: "policy evaluation produced no verdict".into(),
    })
}

pub(crate) fn shared_connection_decision(sql: &str, driver_id: &str, facts: &StatementFacts) -> Option<Decision> {
    if (facts.class == StatementClass::Transaction && !facts.is_multi_statement)
        || has_implicit_transaction_starter(sql, driver_id)
        || has_unclosed_shared_transaction(sql, driver_id)
    {
        return Some(Decision::Deny {
            rule: "transaction_control_needs_session".into(),
            message: "BEGIN, COMMIT and ROLLBACK cannot run on a shared connection, because the transaction would not \
                      cover the statements after it. Turn on Session for this editor tab to keep a transaction \
                      open between statements, or send the whole transaction as one batch"
                .into(),
        });
    }
    None
}

pub(crate) fn interactive_transaction_decision(driver_id: &str, sql: &str, facts: &StatementFacts) -> Option<Decision> {
    let plain_dml = matches!(
        facts.class,
        StatementClass::Select | StatementClass::Insert | StatementClass::Update | StatementClass::Delete
    );
    if driver_id != "mysql"
        || (plain_dml && !facts.contains_ddl() && !facts.contains_unknown_write())
        || mysql_statement_is_transaction_safe(sql)
    {
        return None;
    }
    Some(Decision::Deny {
        rule: "mysql_statement_not_rollback_safe".into(),
        message: "This MySQL statement may have effects that the interactive transaction cannot roll back. Run it \
                  on its own, outside the transaction"
            .into(),
    })
}

fn has_unclosed_shared_transaction(sql: &str, driver_id: &str) -> bool {
    if leaves_tsql_transaction_unclosed(sql, driver_id) {
        return true;
    }
    let statements = tablepro_core::sql_lex::split_statements(sql, driver_id);
    if statements.len() < 2 {
        return false;
    }

    let mut open = false;
    for statement in statements {
        match transaction_control(&statement, driver_id) {
            Some(
                TransactionControl::Begin
                | TransactionControl::BeginReadWrite
                | TransactionControl::BeginReadOnly
                | TransactionControl::BeginReadOnlySnapshot,
            ) if open => return true,
            Some(
                TransactionControl::Begin
                | TransactionControl::BeginReadWrite
                | TransactionControl::BeginReadOnly
                | TransactionControl::BeginReadOnlySnapshot,
            ) => open = true,
            Some(TransactionControl::Commit { chain } | TransactionControl::Rollback { chain }) => {
                if !open || chain {
                    return true;
                }
                open = false;
            }
            None if open => {
                let facts = crate::classify::classify(&statement, driver_id);
                if facts.class == StatementClass::Unparseable || (driver_id == "mysql" && facts.contains_ddl()) {
                    return true;
                }
            }
            None => {}
        }
    }
    open
}

pub(crate) fn evaluate_categorical(
    principal: &Principal,
    environment: Environment,
    facts: &StatementFacts,
    connection_read_only: bool,
    env_policy: &EnvPolicy,
) -> Option<Decision> {
    Decision::join_all(evaluate_categorical_decisions(
        principal,
        environment,
        facts,
        connection_read_only,
        env_policy,
    ))
}

fn evaluate_categorical_decisions(
    principal: &Principal,
    environment: Environment,
    facts: &StatementFacts,
    connection_read_only: bool,
    env_policy: &EnvPolicy,
) -> Vec<Decision> {
    let effects = facts.effects;
    let mut decisions = Vec::new();
    if connection_read_only && effects.writes() {
        decisions.push(Decision::Deny {
            rule: "connection_read_only".into(),
            message: "connection is marked read-only; statements that may write are not permitted".into(),
        });
    }

    if facts.class == StatementClass::Unparseable {
        return evaluate_unparseable_decisions(decisions, principal, environment, facts, env_policy);
    }

    if facts.is_multi_statement && principal.is_agent() && !env_policy.agent_allow_multi_statement {
        decisions.push(Decision::Deny {
            rule: "agent_no_multi_statement".into(),
            message: "agents may not run multi-statement scripts".into(),
        });
    }

    if principal.is_agent() && (facts.class == StatementClass::Administrative || effects.contains(Effects::ADMIN)) {
        decisions.push(Decision::Deny {
            rule: "agent_admin_denied".into(),
            message: "agents may not run administrative database operations".into(),
        });
    }

    if !effects.writes() && !effects.contains(Effects::HOST_OR_FILE_ACCESS) {
        decisions.push(Decision::Allow {
            rule: "read_allow".into(),
        });
        return decisions;
    }

    if principal.is_agent() {
        decisions.extend(evaluate_agent_write_categorical(environment, facts, env_policy));
    } else {
        decisions.extend(evaluate_human_dangerous_effects(environment, facts, env_policy));
    }
    decisions
}

fn evaluate_unparseable_decisions(
    mut decisions: Vec<Decision>,
    principal: &Principal,
    environment: Environment,
    facts: &StatementFacts,
    env_policy: &EnvPolicy,
) -> Vec<Decision> {
    let unparseable = decide_unparseable(principal, env_policy);
    let explicitly_allowed = unparseable.is_allow();
    decisions.push(unparseable);
    if principal.is_agent() {
        return decisions;
    }
    if explicitly_allowed {
        decisions.extend(evaluate_human_dangerous_effects(environment, facts, env_policy));
    } else if env_policy.human_approve_writes {
        decisions.push(Decision::RequireApproval {
            rule: "human_write_approve".into(),
            reason: format!("writes require approval in {}", environment.as_str()),
            preview: None,
        });
    }
    decisions
}

fn evaluate_human_dangerous_effects(
    environment: Environment,
    facts: &StatementFacts,
    env_policy: &EnvPolicy,
) -> Vec<Decision> {
    let effects = facts.effects;
    let dangerous_effect = effects.contains(Effects::ADMIN)
        || effects.contains(Effects::HOST_OR_FILE_ACCESS)
        || effects.contains(Effects::UNKNOWN);
    if !dangerous_effect {
        return Vec::new();
    }

    let mut decisions = vec![Decision::RequireApproval {
        rule: "human_dangerous_effect_approval".into(),
        reason: "administrative, host-access or unknown write effects require confirmation".into(),
        preview: Some(table_preview(facts)),
    }];
    if env_policy.human_approve_writes {
        decisions.push(Decision::RequireApproval {
            rule: "human_write_approve".into(),
            reason: format!("writes require approval in {}", environment.as_str()),
            preview: Some(table_preview(facts)),
        });
    }
    decisions
}

pub(crate) fn evaluate_eligible_write(
    principal: &Principal,
    environment: Environment,
    facts: &StatementFacts,
    env_policy: &EnvPolicy,
    estimated_rows: Option<u64>,
) -> Decision {
    Decision::join_all(evaluate_eligible_write_decisions(
        principal,
        environment,
        facts,
        env_policy,
        estimated_rows,
    ))
    .unwrap_or_else(|| Decision::Deny {
        rule: "no_policy_rules_applied".into(),
        message: "policy evaluation produced no verdict".into(),
    })
}

fn evaluate_eligible_write_decisions(
    principal: &Principal,
    environment: Environment,
    facts: &StatementFacts,
    env_policy: &EnvPolicy,
    estimated_rows: Option<u64>,
) -> Vec<Decision> {
    if facts.class == StatementClass::Unparseable {
        return vec![decide_unparseable(principal, env_policy)];
    }

    if principal.is_agent() {
        return evaluate_agent_write_decisions(environment, facts, env_policy, estimated_rows);
    }

    evaluate_human_write_decisions(environment, facts, env_policy, estimated_rows)
}

fn decide_unparseable(principal: &Principal, env_policy: &EnvPolicy) -> Decision {
    if principal.is_agent() {
        Decision::Deny {
            rule: "fail_closed_unparseable".into(),
            message: "SQL could not be parsed; agents are denied".into(),
        }
    } else if env_policy.human_approve_unparseable {
        Decision::RequireApproval {
            rule: "fail_closed_unparseable".into(),
            reason: "SQL could not be parsed; confirm before running".into(),
            preview: None,
        }
    } else {
        Decision::Allow {
            rule: "unparseable_human_allow".into(),
        }
    }
}

fn table_preview(facts: &StatementFacts) -> String {
    let mut tables = facts.tables.clone();
    tables.sort_unstable();
    tables.dedup();
    format!("tables: {}", tables.join(", "))
}

fn evaluate_agent_write_categorical(
    environment: Environment,
    facts: &StatementFacts,
    env_policy: &EnvPolicy,
) -> Vec<Decision> {
    let effects = facts.effects;
    let mut decisions = Vec::new();
    if effects.contains(Effects::HOST_OR_FILE_ACCESS) {
        decisions.push(Decision::Deny {
            rule: "agent_host_access_denied".into(),
            message: "agents may not access host files or external programs".into(),
        });
    }

    if env_policy.agent_writes == crate::config::WritePolicy::Deny {
        decisions.push(Decision::Deny {
            rule: "agent_writes_denied".into(),
            message: format!("agent writes are denied in {}", environment.as_str()),
        });
    }

    if facts.contains_ddl() && !env_policy.agent_allow_ddl {
        decisions.push(Decision::Deny {
            rule: "agent_ddl_denied".into(),
            message: "agents may not run DDL".into(),
        });
    }

    if facts.contains_unknown_write() {
        decisions.push(Decision::Deny {
            rule: "agent_unknown_write_denied".into(),
            message: "agents may not run writes whose safety category cannot be determined".into(),
        });
    }

    if facts.contains_unscoped_dml {
        decisions.push(Decision::Deny {
            rule: "agent_no_unscoped_dml".into(),
            message: "agents may not run unscoped UPDATE/DELETE or TRUNCATE".into(),
        });
    }

    decisions
}

fn evaluate_agent_write_decisions(
    environment: Environment,
    facts: &StatementFacts,
    env_policy: &EnvPolicy,
    estimated_rows: Option<u64>,
) -> Vec<Decision> {
    let mut decisions = evaluate_agent_write_categorical(environment, facts, env_policy);

    if let Some(limit) = env_policy.blast_radius_max_rows
        && facts.contains_mutating_dml()
    {
        match estimated_rows {
            Some(rows) if rows > limit => {
                decisions.push(Decision::Deny {
                    rule: "blast_radius_exceeded".into(),
                    message: format!("would affect {rows} rows; limit is {limit}"),
                });
            }
            Some(_) => {}
            None => {
                decisions.push(Decision::Deny {
                    rule: "blast_radius_unknown".into(),
                    message: "could not estimate affected rows; agents are denied when a blast-radius limit is set"
                        .into(),
                });
            }
        }
    }

    decisions.push(match env_policy.agent_writes {
        crate::config::WritePolicy::Allow => Decision::Allow {
            rule: "agent_write_allow".into(),
        },
        crate::config::WritePolicy::Approve => Decision::RequireApproval {
            rule: "agent_write_approve".into(),
            reason: format!("{:?} requires approval for agent writes", facts.class),
            preview: Some(table_preview(facts)),
        },
        crate::config::WritePolicy::Deny => Decision::Deny {
            rule: "agent_writes_denied".into(),
            message: format!("agent writes are denied in {}", environment.as_str()),
        },
    });
    decisions
}

fn evaluate_human_write_categorical(
    environment: Environment,
    facts: &StatementFacts,
    env_policy: &EnvPolicy,
) -> Vec<Decision> {
    let mut decisions = Vec::new();
    if facts.contains_unscoped_dml {
        decisions.push(Decision::RequireApproval {
            rule: "human_unscoped_dml".into(),
            reason: "unscoped UPDATE/DELETE or TRUNCATE".into(),
            preview: Some(table_preview(facts)),
        });
    }

    if facts.contains_ddl() && env_policy.human_approve_ddl {
        decisions.push(Decision::RequireApproval {
            rule: "human_ddl_approve".into(),
            reason: format!("DDL on {}", environment.as_str()),
            preview: Some(table_preview(facts)),
        });
    }

    decisions
}

fn evaluate_human_write_decisions(
    environment: Environment,
    facts: &StatementFacts,
    env_policy: &EnvPolicy,
    estimated_rows: Option<u64>,
) -> Vec<Decision> {
    let effects = facts.effects;
    let mut decisions = evaluate_human_write_categorical(environment, facts, env_policy);

    if let Some(limit) = env_policy.blast_radius_max_rows
        && facts.contains_mutating_dml()
    {
        match estimated_rows {
            Some(rows) if rows > limit => {
                decisions.push(Decision::RequireApproval {
                    rule: "blast_radius_approve".into(),
                    reason: format!("would affect {rows} rows (limit {limit})"),
                    preview: None,
                });
            }
            Some(_) => {}
            None => {
                decisions.push(Decision::RequireApproval {
                    rule: "blast_radius_unknown".into(),
                    reason: "could not estimate affected rows; confirm before running".into(),
                    preview: None,
                });
            }
        }
    }

    if env_policy.human_approve_writes {
        decisions.push(Decision::RequireApproval {
            rule: "human_write_approve".into(),
            reason: format!("writes require approval in {}", environment.as_str()),
            preview: Some(table_preview(facts)),
        });
    } else if effects.contains(Effects::ADMIN)
        || effects.contains(Effects::HOST_OR_FILE_ACCESS)
        || effects.contains(Effects::UNKNOWN)
    {
        decisions.push(Decision::RequireApproval {
            rule: "human_dangerous_effect_approval".into(),
            reason: "administrative, host-access or unknown write effects require confirmation".into(),
            preview: Some(table_preview(facts)),
        });
    }

    decisions.push(Decision::Allow {
        rule: "human_write_allow".into(),
    });
    decisions
}

#[cfg(test)]
#[path = "rules_tests.rs"]
mod tests;
