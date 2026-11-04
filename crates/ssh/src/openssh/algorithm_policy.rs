pub(super) const REMOVED_ALGORITHMS: [(&str, &str); 5] = [
    ("HostKeyAlgorithms", "-ssh-rsa,ssh-rsa-cert-v01@openssh.com"),
    ("PubkeyAcceptedAlgorithms", "-ssh-rsa,ssh-rsa-cert-v01@openssh.com"),
    (
        "Ciphers",
        "-3des-cbc,blowfish-cbc,cast128-cbc,arcfour,arcfour128,arcfour256",
    ),
    (
        "KexAlgorithms",
        "-diffie-hellman-group1-sha1,diffie-hellman-group14-sha1,diffie-hellman-group-exchange-sha1",
    ),
    (
        "MACs",
        "-hmac-sha1,hmac-sha1-96,hmac-sha1-etm@openssh.com,hmac-sha1-96-etm@openssh.com,\
         hmac-md5,hmac-md5-96,hmac-md5-etm@openssh.com,hmac-md5-96-etm@openssh.com",
    ),
];

pub(super) fn config_lines() -> String {
    REMOVED_ALGORITHMS
        .iter()
        .map(|(keyword, value)| format!("{keyword}={value}\n"))
        .collect()
}

#[cfg(test)]
#[path = "algorithm_policy_tests.rs"]
mod tests;
