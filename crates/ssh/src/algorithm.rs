use std::fmt;

const OFFER_LIMIT: usize = 512;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlgorithmClass {
    HostKey,
    KeyExchange,
    Cipher,
    Mac,
}

impl AlgorithmClass {
    pub fn label(self) -> &'static str {
        match self {
            Self::HostKey => "host key",
            Self::KeyExchange => "key exchange",
            Self::Cipher => "cipher",
            Self::Mac => "MAC",
        }
    }

    pub(crate) fn from_russh(kind: &russh::AlgorithmKind) -> Option<Self> {
        match kind {
            russh::AlgorithmKind::Key => Some(Self::HostKey),
            russh::AlgorithmKind::Kex => Some(Self::KeyExchange),
            russh::AlgorithmKind::Cipher => Some(Self::Cipher),
            russh::AlgorithmKind::Mac => Some(Self::Mac),
            russh::AlgorithmKind::Compression => None,
        }
    }

    pub(crate) fn from_openssh(kind: &str) -> Option<Self> {
        match kind {
            "host key type" => Some(Self::HostKey),
            "key exchange method" => Some(Self::KeyExchange),
            "cipher" => Some(Self::Cipher),
            "MAC" => Some(Self::Mac),
            _ => None,
        }
    }
}

impl fmt::Display for AlgorithmClass {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.label())
    }
}

pub(crate) fn offered_names<S: AsRef<str>>(names: &[S]) -> String {
    let joined = names.iter().map(AsRef::as_ref).collect::<Vec<_>>().join(", ");
    sanitize_offer(&joined)
}

pub(crate) fn sanitize_offer(offer: &str) -> String {
    let clean: String = offer.chars().filter(|c| !c.is_control()).take(OFFER_LIMIT).collect();
    if clean.is_empty() { "nothing".into() } else { clean }
}

#[cfg(test)]
#[path = "algorithm_tests.rs"]
mod tests;
