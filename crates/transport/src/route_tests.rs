use bookie_ssh::AlgorithmClass;
use bookie_ssh::openssh::OpenSshError;

use super::*;

#[test]
fn a_refused_algorithm_reaches_the_user_with_its_class_and_the_peer_offer() {
    let error = map_openssh_error(OpenSshError::NoAcceptableAlgorithm {
        host: "bastion.example".into(),
        class: AlgorithmClass::Cipher,
        offered: "3des-cbc".into(),
    });
    let TransportError::Ssh(message) = error else {
        panic!("expected the generic SSH error, got {error:?}");
    };
    assert!(message.contains("bastion.example"), "{message}");
    assert!(message.contains("no acceptable cipher algorithm"), "{message}");
    assert!(message.contains("The server offered: 3des-cbc"), "{message}");
}
