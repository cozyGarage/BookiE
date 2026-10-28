use std::collections::HashMap;
use std::path::Path;

use serde::{Deserialize, Serialize};
use tablepro_core::Environment;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum WritePolicy {
    Allow,
    #[default]
    Approve,
    Deny,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvPolicy {
    pub agent_writes: WritePolicy,
    pub agent_allow_ddl: bool,
    pub agent_allow_multi_statement: bool,
    pub human_approve_writes: bool,
    pub human_approve_ddl: bool,
    pub human_approve_unparseable: bool,
    pub human_allow_unaudited_writes: bool,
    pub blast_radius_max_rows: Option<u64>,
    pub mask_agent_results: bool,
}

impl Default for EnvPolicy {
    fn default() -> Self {
        Self {
            agent_writes: WritePolicy::Approve,
            agent_allow_ddl: false,
            agent_allow_multi_statement: false,
            human_approve_writes: false,
            human_approve_ddl: false,
            human_approve_unparseable: true,
            human_allow_unaudited_writes: false,
            blast_radius_max_rows: Some(10_000),
            mask_agent_results: true,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnvPolicyOverride {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_writes: Option<WritePolicy>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_allow_ddl: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_allow_multi_statement: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub human_approve_writes: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub human_approve_ddl: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub human_approve_unparseable: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub human_allow_unaudited_writes: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blast_radius_max_rows: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mask_agent_results: Option<bool>,
}

impl EnvPolicyOverride {
    fn apply_to(&self, mut policy: EnvPolicy) -> EnvPolicy {
        if let Some(value) = self.agent_writes {
            policy.agent_writes = value;
        }
        if let Some(value) = self.agent_allow_ddl {
            policy.agent_allow_ddl = value;
        }
        if let Some(value) = self.agent_allow_multi_statement {
            policy.agent_allow_multi_statement = value;
        }
        if let Some(value) = self.human_approve_writes {
            policy.human_approve_writes = value;
        }
        if let Some(value) = self.human_approve_ddl {
            policy.human_approve_ddl = value;
        }
        if let Some(value) = self.human_approve_unparseable {
            policy.human_approve_unparseable = value;
        }
        if let Some(value) = self.human_allow_unaudited_writes {
            policy.human_allow_unaudited_writes = value;
        }
        if let Some(value) = self.blast_radius_max_rows {
            policy.blast_radius_max_rows = Some(value);
        }
        if let Some(value) = self.mask_agent_results {
            policy.mask_agent_results = value;
        }
        policy
    }
}

impl EnvPolicy {
    pub fn local_defaults() -> Self {
        Self {
            agent_writes: WritePolicy::Approve,
            human_approve_writes: false,
            human_approve_ddl: false,
            blast_radius_max_rows: Some(100_000),
            ..Self::default()
        }
    }

    pub fn prod_defaults() -> Self {
        Self {
            agent_writes: WritePolicy::Deny,
            agent_allow_ddl: false,
            human_approve_writes: true,
            human_approve_ddl: true,
            blast_radius_max_rows: Some(1_000),
            mask_agent_results: true,
            ..Self::default()
        }
    }

    pub fn staging_defaults() -> Self {
        Self {
            agent_writes: WritePolicy::Approve,
            human_approve_writes: false,
            human_approve_ddl: true,
            blast_radius_max_rows: Some(5_000),
            ..Self::default()
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MaskRule {
    /// Glob matched against column names (case-insensitive).
    pub pattern: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PolicyConfig {
    #[serde(default)]
    pub environments: HashMap<String, EnvPolicyOverride>,
    #[serde(default)]
    pub connection_overrides: HashMap<String, EnvPolicyOverride>,
    /// Patterns the operator adds on top of `mask::DEFAULT_SENSITIVE_PATTERNS`.
    /// The defaults are always active; there is no way to configure masking
    /// off for `password`, `ssn`, `cvv` and the rest of the built-in list.
    #[serde(default)]
    pub mask_patterns: Vec<MaskRule>,
}

impl PolicyConfig {
    /// The full pattern list masking actually uses: the built-in sensitive
    /// defaults plus whatever the operator configured, deduplicated. An
    /// operator adding one pattern can only make masking cover more columns,
    /// never fewer.
    pub fn effective_mask_patterns(&self) -> Vec<String> {
        let mut patterns: Vec<String> = crate::mask::DEFAULT_SENSITIVE_PATTERNS
            .iter()
            .map(|p| (*p).to_string())
            .collect();
        for rule in &self.mask_patterns {
            if !patterns.contains(&rule.pattern) {
                patterns.push(rule.pattern.clone());
            }
        }
        patterns
    }

    pub fn for_environment(&self, env: Environment) -> EnvPolicy {
        let defaults = match env {
            Environment::Local | Environment::Dev => EnvPolicy::local_defaults(),
            Environment::Staging => EnvPolicy::staging_defaults(),
            Environment::Prod => EnvPolicy::prod_defaults(),
        };
        self.environments
            .get(env.as_str())
            .map_or(defaults.clone(), |overrides| overrides.apply_to(defaults))
    }

    pub fn for_connection(&self, connection_id: &str, env: Environment) -> EnvPolicy {
        let environment_policy = self.for_environment(env);
        self.connection_overrides
            .get(connection_id)
            .map_or(environment_policy.clone(), |overrides| {
                overrides.apply_to(environment_policy)
            })
    }
}

const KNOWN_ENVIRONMENT_NAMES: &[&str] = &["local", "dev", "staging", "prod"];

pub fn load_from_path(path: &Path) -> Result<PolicyConfig, String> {
    let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let mut policy: PolicyConfig = toml::from_str(&text).map_err(|e| e.to_string())?;
    for rule in &policy.mask_patterns {
        glob::Pattern::new(&rule.pattern.to_lowercase())
            .map_err(|e| format!("invalid mask_patterns entry {:?}: {e}", rule.pattern))?;
    }
    for name in policy.environments.keys() {
        if !KNOWN_ENVIRONMENT_NAMES.contains(&name.as_str()) {
            return Err(format!(
                "[environments.{name}] does not match a known environment ({})",
                KNOWN_ENVIRONMENT_NAMES.join(", ")
            ));
        }
    }
    // A connection_overrides key that differs from the connection id's
    // canonical lowercase-hyphenated form (an uppercase UUID, say) would
    // otherwise parse without error and then never match at lookup time --
    // silently dropping the override, including one meant to tighten policy.
    // Re-key by the canonical form so casing in the file can't matter.
    let mut canonical_overrides = HashMap::with_capacity(policy.connection_overrides.len());
    for (key, value) in policy.connection_overrides {
        let id: uuid::Uuid = key
            .parse()
            .map_err(|e| format!("[connection_overrides.{key}] is not a connection id: {e}"))?;
        canonical_overrides.insert(id.to_string(), value);
    }
    policy.connection_overrides = canonical_overrides;
    Ok(policy)
}

#[cfg(test)]
#[path = "config_tests.rs"]
mod tests;
