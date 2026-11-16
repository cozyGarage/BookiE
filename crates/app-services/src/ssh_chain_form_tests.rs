use bookie_storage::{SavedSshAuth, SavedSshConfig, SshClient};
use secrecy::{ExposeSecret, SecretString};
use uuid::Uuid;

use super::*;

fn head() -> SavedSshConfig {
    SavedSshConfig {
        hop_id: Uuid::new_v4(),
        credential_revision: 0,
        host: "edge".into(),
        port: 22,
        username: "me".into(),
        auth: SavedSshAuth::Password,
        jump: None,
        client: SshClient::Builtin,
        agent: false,
        agent_identity: None,
        local_port: Some(15432),
    }
}

fn hop(host: &str) -> JumpHopForm {
    JumpHopForm {
        hop_id: None,
        host: host.into(),
        port: 22,
        username: "ops".into(),
        auth: JumpAuth::PrivateKey,
        key_path: "/home/me/.ssh/id_ed25519".into(),
        agent_identity: String::new(),
        password: None,
        passphrase: None,
        stored_password: false,
        stored_passphrase: false,
    }
}

fn secret(value: &str) -> Option<SecretString> {
    Some(SecretString::new(value.into()))
}

fn three_hop_chain() -> ChainPlan {
    let mut inner = hop("inner");
    inner.auth = JumpAuth::Password;
    inner.password = secret("inner-pw");
    let mut middle = hop("middle");
    middle.passphrase = secret("key-pass");
    plan_chain(head(), &[middle, inner], None).unwrap()
}

#[test]
fn a_three_hop_chain_nests_in_order_and_keeps_the_local_port_on_the_first_hop() {
    let plan = three_hop_chain();
    let hops = plan.head.flatten_hops();
    let hosts: Vec<&str> = hops.iter().map(|hop| hop.host.as_str()).collect();
    assert_eq!(hosts, vec!["edge", "middle", "inner"]);
    assert_eq!(plan.head.local_port, Some(15432));
    assert!(hops[1..].iter().all(|hop| hop.local_port.is_none()));
    assert_eq!(plan.jumps.len(), 2);
}

#[test]
fn a_three_hop_chain_round_trips_through_the_saved_shape_without_changing_identity() {
    let plan = three_hop_chain();
    let saved = plan.head.clone();
    let hops = saved.flatten_hops();
    let forms: Vec<JumpHopForm> = hops[1..]
        .iter()
        .map(|hop| JumpHopForm {
            hop_id: Some(hop.hop_id),
            host: hop.host.clone(),
            port: hop.port,
            username: hop.username.clone(),
            auth: match hop.auth {
                SavedSshAuth::Password => JumpAuth::Password,
                SavedSshAuth::PrivateKey { .. } => JumpAuth::PrivateKey,
            },
            key_path: match &hop.auth {
                SavedSshAuth::PrivateKey { path, .. } => path.to_string_lossy().into_owned(),
                SavedSshAuth::Password => String::new(),
            },
            agent_identity: String::new(),
            password: None,
            passphrase: None,
            stored_password: matches!(hop.auth, SavedSshAuth::Password),
            stored_passphrase: matches!(
                hop.auth,
                SavedSshAuth::PrivateKey {
                    has_passphrase: true,
                    ..
                }
            ),
        })
        .collect();
    let again = plan_chain(saved.clone(), &forms, Some(&saved)).unwrap();
    assert_eq!(again.head, saved);
    assert!(again.writes.is_empty());
    assert!(again.obsolete.is_empty());
}

#[test]
fn a_new_hop_gets_an_id_and_the_first_secret_revision() {
    let plan = three_hop_chain();
    let hops = plan.head.flatten_hops();
    assert_eq!(hops[1].credential_revision, 1);
    assert_eq!(hops[2].credential_revision, 1);
    assert_ne!(hops[1].hop_id, hops[2].hop_id);
    assert_eq!(plan.writes.len(), 2);
}

#[test]
fn the_revision_bumps_only_when_a_secret_changes() {
    let saved = three_hop_chain().head;
    let existing = saved.flatten_hops()[2].clone();
    let unchanged = JumpHopForm {
        hop_id: Some(existing.hop_id),
        auth: JumpAuth::Password,
        password: None,
        stored_password: true,
        ..hop("inner")
    };
    let kept = plan_chain(saved.clone(), std::slice::from_ref(&unchanged), Some(&saved)).unwrap();
    assert_eq!(
        kept.head.jump.as_ref().unwrap().credential_revision,
        existing.credential_revision
    );
    assert!(kept.writes.is_empty());

    let rotated = JumpHopForm {
        password: secret("new"),
        ..unchanged
    };
    let plan = plan_chain(saved.clone(), &[rotated], Some(&saved)).unwrap();
    let hop = plan.head.jump.as_ref().unwrap();
    assert_eq!(hop.hop_id, existing.hop_id);
    assert_eq!(hop.credential_revision, existing.credential_revision + 1);
    assert_eq!(plan.writes[0].secret.revision, existing.credential_revision + 1);
    assert_eq!(plan.writes[0].value.expose_secret(), "new");
    assert!(plan.obsolete.contains(&HopSecretRef {
        hop_id: existing.hop_id,
        revision: existing.credential_revision,
        kind: HopSecretKind::Password,
    }));
}

#[test]
fn removing_a_hop_schedules_its_secrets_for_deletion() {
    let saved = three_hop_chain().head;
    let hops: Vec<SavedSshConfig> = saved.flatten_hops().into_iter().cloned().collect();
    let keep = JumpHopForm {
        hop_id: Some(hops[1].hop_id),
        stored_passphrase: true,
        ..hop("middle")
    };
    let plan = plan_chain(saved.clone(), &[keep], Some(&saved)).unwrap();
    assert_eq!(plan.head.flatten_hops().len(), 2);
    let removed = &hops[2];
    assert!(plan.obsolete.iter().any(|secret| secret.hop_id == removed.hop_id
        && secret.revision == removed.credential_revision
        && secret.kind == HopSecretKind::Password));
    assert!(plan.obsolete.iter().all(|secret| secret.hop_id != hops[1].hop_id));
}

#[test]
fn dropping_every_jump_host_deletes_all_their_secrets() {
    let saved = three_hop_chain().head;
    let plan = plan_chain(saved.clone(), &[], Some(&saved)).unwrap();
    assert!(plan.head.jump.is_none());
    assert!(
        plan.obsolete
            .iter()
            .any(|secret| secret.kind == HopSecretKind::Passphrase)
    );
    assert!(
        plan.obsolete
            .iter()
            .any(|secret| secret.kind == HopSecretKind::Password)
    );
}

#[test]
fn switching_a_hop_from_password_to_key_deletes_the_password() {
    let saved = three_hop_chain().head;
    let existing = saved.flatten_hops()[2].clone();
    let switched = JumpHopForm {
        hop_id: Some(existing.hop_id),
        ..hop("inner")
    };
    let plan = plan_chain(saved.clone(), &[hop("middle"), switched], Some(&saved)).unwrap();
    assert!(plan.obsolete.contains(&HopSecretRef {
        hop_id: existing.hop_id,
        revision: existing.credential_revision,
        kind: HopSecretKind::Password,
    }));
}

#[test]
fn validation_names_the_hop_that_is_wrong() {
    let cases: [(JumpHopForm, ChainFormError); 4] = [
        (
            JumpHopForm {
                host: "  ".into(),
                ..hop("x")
            },
            ChainFormError::EmptyHost { position: 2 },
        ),
        (
            JumpHopForm { port: 0, ..hop("x") },
            ChainFormError::InvalidPort { position: 2 },
        ),
        (
            JumpHopForm {
                username: String::new(),
                ..hop("x")
            },
            ChainFormError::EmptyUsername { position: 2 },
        ),
        (
            JumpHopForm {
                key_path: " ".into(),
                ..hop("x")
            },
            ChainFormError::EmptyKeyPath { position: 2 },
        ),
    ];
    for (form, expected) in cases {
        assert_eq!(plan_chain(head(), &[form], None).unwrap_err(), expected);
    }
}

#[test]
fn a_password_hop_needs_a_new_or_stored_password() {
    let form = JumpHopForm {
        auth: JumpAuth::Password,
        ..hop("x")
    };
    assert_eq!(
        plan_chain(head(), std::slice::from_ref(&form), None).unwrap_err(),
        ChainFormError::PasswordRequired { position: 2 }
    );
    let stored = JumpHopForm {
        stored_password: true,
        ..form
    };
    assert!(plan_chain(head(), &[stored], None).is_ok());
}

#[test]
fn a_hop_that_repeats_an_earlier_host_is_refused_whether_adjacent_or_not() {
    let adjacent = [hop("edge")];
    assert_eq!(
        plan_chain(head(), &adjacent, None).unwrap_err(),
        ChainFormError::RepeatedHop { position: 2 }
    );
    let distant = [hop("a"), hop("b"), hop("A")];
    assert_eq!(
        plan_chain(head(), &distant, None).unwrap_err(),
        ChainFormError::RepeatedHop { position: 4 }
    );
    let other_port = JumpHopForm {
        port: 2222,
        ..hop("edge")
    };
    assert!(plan_chain(head(), &[other_port], None).is_ok());
}

#[test]
fn a_chain_longer_than_the_storage_limit_is_refused() {
    let hops: Vec<JumpHopForm> = (0..MAX_CHAIN_HOPS).map(|index| hop(&format!("h{index}"))).collect();
    assert_eq!(
        plan_chain(head(), &hops, None).unwrap_err(),
        ChainFormError::TooManyHops { limit: MAX_CHAIN_HOPS }
    );
    assert!(plan_chain(head(), &hops[..MAX_CHAIN_HOPS - 1], None).is_ok());
}

#[test]
fn the_system_openssh_client_cannot_carry_saved_jump_hosts() {
    let openssh = SavedSshConfig {
        client: SshClient::OpenSsh,
        ..head()
    };
    assert_eq!(
        plan_chain(openssh.clone(), &[hop("a")], None).unwrap_err(),
        ChainFormError::OpenSshRefusesJumpHops
    );
    assert!(plan_chain(openssh, &[], None).is_ok());
}

#[test]
fn an_agent_hop_stores_no_secret_and_keeps_its_identity() {
    let agent = JumpHopForm {
        auth: JumpAuth::Agent,
        agent_identity: " /home/me/.ssh/work.pub ".into(),
        key_path: String::new(),
        ..hop("a")
    };
    let plan = plan_chain(head(), &[agent], None).unwrap();
    let hop = plan.head.jump.unwrap();
    assert!(hop.agent);
    assert_eq!(
        hop.agent_identity.as_deref(),
        Some(std::path::Path::new("/home/me/.ssh/work.pub"))
    );
    assert!(plan.writes.is_empty());
}

#[test]
fn a_hop_without_a_new_secret_connects_with_its_stored_one() {
    let form = JumpHopForm {
        hop_id: Some(Uuid::new_v4()),
        auth: JumpAuth::Password,
        stored_password: true,
        ..hop("x")
    };
    let mut previous = head();
    let id = form.hop_id.unwrap();
    previous.jump = Some(Box::new(SavedSshConfig {
        hop_id: id,
        credential_revision: 3,
        host: "x".into(),
        auth: SavedSshAuth::Password,
        ..head()
    }));
    let plan = plan_chain(previous.clone(), &[form], Some(&previous)).unwrap();
    match &plan.jumps[0].auth {
        JumpConnectAuth::Password(SecretSource::Stored(reference)) => {
            assert_eq!(reference.hop_id, id);
            assert_eq!(reference.revision, 3);
        }
        other => panic!("expected a stored password, got {other:?}"),
    }
}
