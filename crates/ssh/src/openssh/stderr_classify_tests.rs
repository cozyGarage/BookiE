use super::*;

const CONTEXT: ClassifyContext<'static> = ClassifyContext {
    host: "bastion",
    forward_target: Some("db:5432"),
    declined_host_key: None,
};

fn fixture(stderr: &str) -> OpenSshError {
    classify(Some(255), stderr, &CONTEXT)
}

#[test]
fn name_resolution_fixture() {
    assert!(matches!(
        fixture(include_str!("../../tests/fixtures/stderr/name_resolution.txt")),
        OpenSshError::NameResolution { host, .. } if host == "nonexistent-host.invalid"
    ));
}

#[test]
fn refused_fixture() {
    assert_eq!(
        fixture(include_str!("../../tests/fixtures/stderr/refused.txt")),
        OpenSshError::Refused {
            host: "127.0.0.1".to_owned(),
            port: 1
        }
    );
}

#[test]
fn timeout_fixture() {
    assert_eq!(
        fixture(include_str!("../../tests/fixtures/stderr/timeout.txt")),
        OpenSshError::Timeout {
            phase: TimeoutPhase::Handshake
        }
    );
}

#[test]
fn config_rejected_option_fixture() {
    assert!(matches!(
        fixture(include_str!("../../tests/fixtures/stderr/config_rejected_option.txt")),
        OpenSshError::ConfigRejected { .. }
    ));
}

#[test]
fn bad_config_permissions_are_config_rejected() {
    assert_eq!(
        fixture("Bad owner or permissions on /home/deploy/.ssh/config\n"),
        OpenSshError::ConfigRejected {
            detail: "Bad owner or permissions on /home/deploy/.ssh/config".to_owned()
        }
    );
}

#[test]
fn control_path_too_long_fixture() {
    assert!(matches!(
        fixture(include_str!("../../tests/fixtures/stderr/control_path_too_long.txt")),
        OpenSshError::ControlPathTooLong { .. }
    ));
}

#[test]
fn host_key_unknown_strict_fixture() {
    assert!(matches!(
        fixture(include_str!("../../tests/fixtures/stderr/host_key_unknown_strict.txt")),
        OpenSshError::HostKeyUnknown { algorithm, declined: false, .. } if algorithm == "ED25519"
    ));
}

#[test]
fn auth_rejected_fixture() {
    assert!(matches!(
        fixture(include_str!("../../tests/fixtures/stderr/auth_rejected.txt")),
        OpenSshError::AuthenticationRejected { user, host, methods }
            if user == "deploy" && host == "127.0.0.1" && methods == ["publickey", "password", "keyboard-interactive"]
    ));
}

#[test]
fn host_key_changed_fixture() {
    assert!(matches!(
        fixture(include_str!("../../tests/fixtures/stderr/host_key_changed.txt")),
        OpenSshError::HostKeyChanged {
            host,
            algorithm,
            fingerprint,
            known_hosts,
            line: 1,
        } if host == "[127.0.0.1]:32768"
            && algorithm == "ED25519"
            && fingerprint == "SHA256:hqQS4EIntVyXA99oGaFNB9YErcr85eUmEi96ZPSAEDg"
            && known_hosts.ends_with("known_hosts")
    ));
}

#[test]
fn host_key_revoked_fixture() {
    assert!(matches!(
        fixture(include_str!("../../tests/fixtures/stderr/host_key_revoked.txt")),
        OpenSshError::HostKeyRevoked { detail, .. } if detail.contains("revoked by file")
    ));
}

#[test]
fn crlf_line_endings_classify_the_same() {
    for stderr in [
        include_str!("../../tests/fixtures/stderr/auth_rejected.txt"),
        include_str!("../../tests/fixtures/stderr/host_key_changed.txt"),
    ] {
        assert_eq!(fixture(&stderr.replace('\n', "\r\n")), fixture(stderr));
    }
}

#[test]
fn a_forward_failure_names_the_target() {
    assert!(matches!(
        fixture("mux_client_forward: forwarding request failed: master forward request failed\n"),
        OpenSshError::ForwardRejected { target, .. } if target == "db:5432"
    ));
    assert!(matches!(
        fixture("channel 2: open failed: connect failed: Connection refused\n"),
        OpenSshError::ChannelOpenFailed { target, .. } if target == "db:5432"
    ));
}

#[test]
fn unknown_output_becomes_protocol_with_status() {
    assert_eq!(
        fixture("kex_exchange_identification: Connection closed by remote host\n"),
        OpenSshError::Protocol {
            detail: "ssh exited with status 255: kex_exchange_identification: Connection closed by remote host"
                .to_owned()
        }
    );
    assert_eq!(
        classify(None, "", &CONTEXT),
        OpenSshError::Protocol {
            detail: "ssh ended without a status: ".to_owned()
        }
    );
}

#[test]
fn a_long_unknown_output_is_cut_to_its_tail_on_a_char_boundary() {
    let stderr = format!("{}é{}", "x".repeat(10_000), "y".repeat(PROTOCOL_DETAIL_LIMIT - 1));
    let OpenSshError::Protocol { detail } = fixture(&stderr) else {
        panic!("expected a protocol error");
    };
    assert!(detail.len() <= PROTOCOL_DETAIL_LIMIT + 40);
    assert!(detail.ends_with('y'));
}

#[test]
fn a_failed_verification_without_a_declined_prompt_is_not_reported_as_declined() {
    assert_eq!(
        fixture("Host key verification failed.\n"),
        OpenSshError::HostKeyUnknown {
            host: "bastion".to_owned(),
            algorithm: String::new(),
            fingerprint: String::new(),
            declined: false,
        }
    );
}

#[test]
fn a_declined_prompt_reports_the_declined_key() {
    let declined = DeclinedHostKey {
        algorithm: "ED25519".to_owned(),
        fingerprint: "SHA256:abc".to_owned(),
    };
    let context = ClassifyContext {
        declined_host_key: Some(&declined),
        ..CONTEXT
    };
    assert_eq!(
        classify(Some(255), "Host key verification failed.\n", &context),
        OpenSshError::HostKeyUnknown {
            host: "bastion".to_owned(),
            algorithm: "ED25519".to_owned(),
            fingerprint: "SHA256:abc".to_owned(),
            declined: true,
        }
    );
}
