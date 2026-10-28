#[test]
fn the_key_derivation_matches_the_value_released_builds_produce() {
    let salt = [7u8; SALT_LEN];
    let kdf = KdfHeader {
        algorithm: KDF_ALGORITHM.into(),
        salt: BASE64.encode(salt),
        memory_kib: 19_456,
        iterations: 2,
        parallelism: 1,
    };
    let key = derive_key("correct horse battery staple", &salt, &kdf).unwrap();
    let hex: String = key.iter().map(|byte| format!("{byte:02x}")).collect();
    assert_eq!(hex, "799f12b9e17710824482d829835acb69f5a9355bf774c4f07342823b11b90928");
}

use super::*;

fn envelope() -> EnvelopeHeader {
    EnvelopeHeader {
        version: 1,
        producer: "bookie 0.2.0".into(),
        exported_at: "2026-09-19T10:00:00Z".into(),
    }
}

fn sealed(plaintext: &[u8]) -> SealedBundle {
    seal(plaintext, "correct horse", &envelope()).expect("seal")
}

#[test]
fn a_sealed_body_opens_with_the_same_passphrase() {
    let body = b"{\"connections\":[]}";
    let bundle = sealed(body);
    let opened = open(&bundle, "correct horse", &envelope()).expect("open");
    assert_eq!(opened.as_slice(), body);
}

#[test]
fn the_export_parameters_are_memory_hard() {
    let bundle = sealed(b"x");
    assert_eq!(bundle.kdf.algorithm, "argon2id");
    assert_eq!(bundle.kdf.memory_kib, EXPORT_MEMORY_KIB);
    assert!(bundle.kdf.memory_kib >= MIN_MEMORY_KIB);
    assert_eq!(bundle.cipher.algorithm, "aes-256-gcm");
}

#[test]
fn every_seal_uses_a_fresh_salt_and_nonce() {
    let first = sealed(b"x");
    let second = sealed(b"x");
    assert_ne!(first.kdf.salt, second.kdf.salt);
    assert_ne!(first.cipher.nonce, second.cipher.nonce);
    assert_ne!(first.ciphertext, second.ciphertext);
}

#[test]
fn a_wrong_passphrase_and_a_tampered_body_fail_the_same_way() {
    let bundle = sealed(b"{\"connections\":[]}");
    let wrong = open(&bundle, "wrong horse", &envelope()).unwrap_err();
    let mut tampered = SealedBundle {
        kdf: bundle.kdf.clone(),
        cipher: bundle.cipher.clone(),
        ciphertext: bundle.ciphertext.clone(),
    };
    tampered.ciphertext[0] ^= 0x01;
    let changed = open(&tampered, "correct horse", &envelope()).unwrap_err();
    assert!(matches!(wrong, BundleError::DecryptionFailed));
    assert!(matches!(changed, BundleError::DecryptionFailed));
    assert_eq!(wrong.to_string(), changed.to_string());
}

#[test]
fn downgrading_the_kdf_header_cannot_produce_a_cheap_bundle() {
    let bundle = sealed(b"{\"connections\":[]}");
    let mut downgraded = SealedBundle {
        kdf: bundle.kdf.clone(),
        cipher: bundle.cipher.clone(),
        ciphertext: bundle.ciphertext.clone(),
    };
    downgraded.kdf.memory_kib = MIN_MEMORY_KIB;
    let error = open(&downgraded, "correct horse", &envelope()).unwrap_err();
    assert!(matches!(error, BundleError::DecryptionFailed));
}

#[test]
fn rewriting_the_envelope_invalidates_the_tag() {
    let bundle = sealed(b"{\"connections\":[]}");
    let other = EnvelopeHeader {
        version: 1,
        producer: "attacker 9.9".into(),
        exported_at: "2026-09-19T10:00:00Z".into(),
    };
    assert!(matches!(
        open(&bundle, "correct horse", &other).unwrap_err(),
        BundleError::DecryptionFailed
    ));
}

#[test]
fn a_memory_bomb_header_is_rejected_before_derivation() {
    let header = KdfHeader {
        algorithm: KDF_ALGORITHM.into(),
        salt: BASE64.encode([0u8; SALT_LEN]),
        memory_kib: u32::MAX,
        iterations: 3,
        parallelism: 1,
    };
    assert!(matches!(
        check_kdf_parameters(&header).unwrap_err(),
        BundleError::KdfParameters
    ));
}

#[test]
fn kdf_parameters_outside_the_accepted_range_are_refused() {
    let base = KdfHeader {
        algorithm: KDF_ALGORITHM.into(),
        salt: BASE64.encode([0u8; SALT_LEN]),
        memory_kib: EXPORT_MEMORY_KIB,
        iterations: EXPORT_ITERATIONS,
        parallelism: EXPORT_PARALLELISM,
    };
    assert!(check_kdf_parameters(&base).is_ok());

    for mutate in [
        |h: &mut KdfHeader| h.memory_kib = MIN_MEMORY_KIB - 1,
        |h: &mut KdfHeader| h.memory_kib = MAX_MEMORY_KIB + 1,
        |h: &mut KdfHeader| h.iterations = 0,
        |h: &mut KdfHeader| h.iterations = MAX_ITERATIONS + 1,
        |h: &mut KdfHeader| h.parallelism = 0,
        |h: &mut KdfHeader| h.parallelism = MAX_PARALLELISM + 1,
        |h: &mut KdfHeader| h.algorithm = "pbkdf2".into(),
        |h: &mut KdfHeader| h.salt = BASE64.encode([0u8; 8]),
        |h: &mut KdfHeader| h.salt = "not base64!!".into(),
    ] {
        let mut header = base.clone();
        mutate(&mut header);
        assert!(
            matches!(check_kdf_parameters(&header), Err(BundleError::KdfParameters)),
            "accepted {header:?}"
        );
    }
}

#[test]
fn an_empty_passphrase_is_refused_on_both_paths() {
    let bundle = sealed(b"x");
    assert!(matches!(seal(b"x", "", &envelope()), Err(BundleError::EmptyPassphrase)));
    assert!(matches!(
        open(&bundle, "", &envelope()).unwrap_err(),
        BundleError::EmptyPassphrase
    ));
}

#[test]
fn the_associated_data_layout_is_length_prefixed_and_unambiguous() {
    let kdf = KdfHeader {
        algorithm: KDF_ALGORITHM.into(),
        salt: BASE64.encode([7u8; SALT_LEN]),
        memory_kib: 1,
        iterations: 2,
        parallelism: 3,
    };
    let first = associated_data(
        &EnvelopeHeader {
            version: 1,
            producer: "ab".into(),
            exported_at: "c".into(),
        },
        &kdf,
        &[7u8; SALT_LEN],
        &[9u8; NONCE_LEN],
    );
    let second = associated_data(
        &EnvelopeHeader {
            version: 1,
            producer: "a".into(),
            exported_at: "bc".into(),
        },
        &kdf,
        &[7u8; SALT_LEN],
        &[9u8; NONCE_LEN],
    );
    assert_ne!(first, second);
    assert!(first.starts_with(b"tablepro.connection-bundle\0"));
}
