use super::*;

const PASSPHRASE: &str = "correct horse battery staple";

#[test]
fn an_encrypted_bundle_written_by_release_0_2_2_still_unlocks() {
    let bytes = include_bytes!("../tests/fixtures/bundle-0.2.2-encrypted.json");

    let ParsedBundle::Encrypted(bundle) = parse_bundle(bytes).expect("parse") else {
        panic!("fixture is encrypted");
    };
    let body = bundle.unlock(PASSPHRASE).expect("unlock");

    assert_eq!(body.connections[0].name, "legacy fixture");
    assert_eq!(body.secrets.len(), 1);
    assert!(bundle.unlock("wrong passphrase").is_err());
}

#[test]
fn a_plaintext_bundle_written_by_release_0_2_2_still_parses() {
    let bytes = include_bytes!("../tests/fixtures/bundle-0.2.2-plaintext.json");

    let ParsedBundle::Plaintext(body) = parse_bundle(bytes).expect("parse") else {
        panic!("fixture is plaintext");
    };

    assert_eq!(body.connections[0].name, "legacy fixture");
}
