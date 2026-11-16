const MAX_DETAIL_CHARS: usize = 240;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TlsFailureKind {
    UnknownIssuer,
    NameMismatch,
    Expired,
    NotYetValid,
    Revoked,
    ServerRefusedTls,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TlsRemedy {
    ChooseCertificateAuthority,
    CheckServerName,
}

impl TlsFailureKind {
    pub fn remedy(self) -> Option<TlsRemedy> {
        match self {
            Self::UnknownIssuer => Some(TlsRemedy::ChooseCertificateAuthority),
            Self::NameMismatch => Some(TlsRemedy::CheckServerName),
            Self::Expired | Self::NotYetValid | Self::Revoked | Self::ServerRefusedTls | Self::Other => None,
        }
    }

    pub fn is_verification_failure(self) -> bool {
        !matches!(self, Self::ServerRefusedTls | Self::Other)
    }
}

const SERVER_REFUSED: [&str; 7] = [
    "does not support tls",
    "does not support ssl",
    "tls is not enabled",
    "ssl is not enabled",
    "server does not use tls",
    "does not offer the requested encryption",
    "server does not use encryption",
];
const REVOCATION_LIST: [&str; 4] = [
    "revocation list",
    "revocationlist",
    "unknownrevocationstatus",
    "unknown revocation status",
];
const NOT_YET_VALID: [&str; 3] = ["not valid yet", "notvalidyet", "not yet valid"];
const NAME_MISMATCH: [&str; 7] = [
    "not valid for name",
    "notvalidforname",
    "hostname mismatch",
    "ip address mismatch",
    "invaliddnsname",
    "invalid tls server name",
    "invalid dnsname",
];
const UNKNOWN_ISSUER: [&str; 9] = [
    "unknownissuer",
    "unknown issuer",
    "unknown ca",
    "unknownca",
    "unknown certificate authority",
    "unable to get local issuer",
    "unable to get issuer",
    "self-signed certificate",
    "self signed certificate",
];

pub fn classify_tls_failure(detail: &str) -> TlsFailureKind {
    let lower = detail.to_ascii_lowercase();
    let has = |markers: &[&str]| markers.iter().any(|marker| lower.contains(marker));
    if has(&SERVER_REFUSED) {
        return TlsFailureKind::ServerRefusedTls;
    }
    if has(&REVOCATION_LIST) {
        return TlsFailureKind::Other;
    }
    if lower.contains("revoked") {
        return TlsFailureKind::Revoked;
    }
    if has(&NOT_YET_VALID) {
        return TlsFailureKind::NotYetValid;
    }
    if lower.contains("expired") {
        return TlsFailureKind::Expired;
    }
    if has(&NAME_MISMATCH) {
        return TlsFailureKind::NameMismatch;
    }
    if has(&UNKNOWN_ISSUER) || lower.contains("unable to verify the first certificate") {
        return TlsFailureKind::UnknownIssuer;
    }
    TlsFailureKind::Other
}

pub fn redacted_tls_detail(detail: &str) -> String {
    let words: Vec<&str> = detail
        .split_whitespace()
        .map(|word| {
            let core = word.trim_matches(|c: char| matches!(c, '"' | '\'' | '(' | ')' | ',' | ';' | ':'));
            if core.starts_with('/') || core.starts_with('~') || core.contains('/') || core.contains('\\') {
                "<path>"
            } else {
                word
            }
        })
        .collect();
    let joined = words.join(" ");
    joined.chars().take(MAX_DETAIL_CHARS).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use TlsFailureKind::*;

    #[test]
    fn rustls_certificate_errors_are_told_apart() {
        let cases = [
            ("invalid peer certificate: UnknownIssuer", UnknownIssuer),
            (
                "invalid peer certificate: certificate not valid for name \"db.example\"; certificate is only valid for other.example",
                NameMismatch,
            ),
            ("invalid peer certificate: NotValidForName", NameMismatch),
            (
                "invalid peer certificate: certificate expired: verification time 1700000000 (UNIX), but certificate is not valid after 1600000000 (100000000 seconds ago)",
                Expired,
            ),
            ("invalid peer certificate: Expired", Expired),
            (
                "invalid peer certificate: certificate not valid yet: verification time 1 (UNIX), but certificate is not valid before 9 (8 seconds in future)",
                NotYetValid,
            ),
            ("invalid peer certificate: NotValidYet", NotYetValid),
            ("invalid peer certificate: Revoked", Revoked),
            ("received fatal alert: UnknownCA", UnknownIssuer),
            ("invalid peer certificate: BadSignature", Other),
        ];
        for (text, expected) in cases {
            assert_eq!(classify_tls_failure(text), expected, "{text}");
        }
    }

    #[test]
    fn revocation_list_problems_are_not_mistaken_for_an_expired_certificate() {
        assert_eq!(
            classify_tls_failure("invalid peer certificate: certificate revocation list expired: verification time 1"),
            Other
        );
        assert_eq!(
            classify_tls_failure("invalid peer certificate: ExpiredRevocationList"),
            Other
        );
        assert_eq!(
            classify_tls_failure("invalid peer certificate: UnknownRevocationStatus"),
            Other
        );
    }

    #[test]
    fn openssl_verify_results_are_told_apart() {
        let cases = [
            (
                "the handshake failed: certificate verify failed: self-signed certificate",
                UnknownIssuer,
            ),
            (
                "certificate verify failed: self-signed certificate in certificate chain",
                UnknownIssuer,
            ),
            (
                "certificate verify failed: unable to get local issuer certificate",
                UnknownIssuer,
            ),
            (
                "certificate verify failed: unable to get issuer certificate",
                UnknownIssuer,
            ),
            (
                "certificate verify failed: unable to verify the first certificate",
                UnknownIssuer,
            ),
            ("certificate verify failed: certificate has expired", Expired),
            (
                "certificate verify failed: certificate is not yet valid or the system clock is incorrect",
                NotYetValid,
            ),
            ("certificate verify failed: certificate is not yet valid", NotYetValid),
            ("certificate verify failed: certificate revoked", Revoked),
            ("certificate verify failed: hostname mismatch", NameMismatch),
            ("certificate verify failed: Hostname mismatch", NameMismatch),
            ("certificate verify failed: IP address mismatch", NameMismatch),
            ("certificate verify failed: certificate signature failure", Other),
        ];
        for (text, expected) in cases {
            assert_eq!(classify_tls_failure(text), expected, "{text}");
        }
    }

    #[test]
    fn a_server_without_tls_is_not_a_verification_failure() {
        for text in [
            "error occurred while attempting to establish a TLS connection: server does not support TLS",
            "the server does not offer the requested encryption",
        ] {
            let kind = classify_tls_failure(text);
            assert_eq!(kind, ServerRefusedTls, "{text}");
            assert!(!kind.is_verification_failure());
        }
    }

    #[test]
    fn driver_written_name_mismatch_text_is_recognised() {
        assert_eq!(
            classify_tls_failure("certificate hostname mismatch; connection closed during TLS verification"),
            NameMismatch
        );
        assert_eq!(classify_tls_failure("invalid TLS server name \"x\": bad"), NameMismatch);
    }

    #[test]
    fn unknown_text_is_other() {
        assert_eq!(classify_tls_failure("handshake timed out"), Other);
        assert_eq!(classify_tls_failure(""), Other);
    }

    #[test]
    fn only_trust_and_name_failures_offer_a_form_remedy() {
        assert_eq!(UnknownIssuer.remedy(), Some(TlsRemedy::ChooseCertificateAuthority));
        assert_eq!(NameMismatch.remedy(), Some(TlsRemedy::CheckServerName));
        for kind in [Expired, NotYetValid, Revoked, ServerRefusedTls, Other] {
            assert_eq!(kind.remedy(), None);
        }
    }

    #[test]
    fn redaction_removes_paths_and_bounds_the_text() {
        let redacted = redacted_tls_detail("cannot trust \"/home/me/.secrets/ca.pem\" or C:\\keys\\ca.pem here");
        assert_eq!(redacted, "cannot trust <path> or <path> here");
        assert!(redacted_tls_detail(&"word ".repeat(500)).chars().count() <= MAX_DETAIL_CHARS);
    }
}
