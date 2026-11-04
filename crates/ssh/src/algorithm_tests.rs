use super::*;

#[test]
fn every_negotiated_kind_but_compression_has_a_named_class() {
    use russh::AlgorithmKind;
    let named = [
        (AlgorithmKind::Key, "host key"),
        (AlgorithmKind::Kex, "key exchange"),
        (AlgorithmKind::Cipher, "cipher"),
        (AlgorithmKind::Mac, "MAC"),
    ];
    for (kind, label) in named {
        assert_eq!(AlgorithmClass::from_russh(&kind).unwrap().label(), label);
    }
    assert!(AlgorithmClass::from_russh(&AlgorithmKind::Compression).is_none());
}

#[test]
fn openssh_wording_maps_to_the_same_classes() {
    assert_eq!(
        AlgorithmClass::from_openssh("host key type"),
        Some(AlgorithmClass::HostKey)
    );
    assert_eq!(
        AlgorithmClass::from_openssh("key exchange method"),
        Some(AlgorithmClass::KeyExchange)
    );
    assert_eq!(AlgorithmClass::from_openssh("cipher"), Some(AlgorithmClass::Cipher));
    assert_eq!(AlgorithmClass::from_openssh("MAC"), Some(AlgorithmClass::Mac));
    assert_eq!(AlgorithmClass::from_openssh("compression"), None);
}

#[test]
fn a_peer_offer_is_bounded_and_stripped_of_control_characters() {
    let hostile = format!("ssh-rsa\x1b[31m\n{}", "x".repeat(2000));
    let clean = sanitize_offer(&hostile);
    assert!(clean.chars().count() <= OFFER_LIMIT);
    assert!(!clean.chars().any(char::is_control));
    assert_eq!(sanitize_offer(""), "nothing");
    assert_eq!(offered_names(&["ssh-rsa", "ssh-dss"]), "ssh-rsa, ssh-dss");
}
