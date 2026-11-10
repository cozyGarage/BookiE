use std::net::SocketAddr;
use std::sync::{Mutex, PoisonError};

use async_trait::async_trait;
use scylla::cluster::metadata::Peer;
use scylla::errors::TranslationError;
use scylla::policies::address_translator::{AddressTranslator, UntranslatedPeer};
use scylla::policies::host_filter::HostFilter;
use uuid::Uuid;

#[derive(Debug)]
pub(crate) struct ContactPin {
    contact: SocketAddr,
    host: Mutex<Option<Uuid>>,
}

impl ContactPin {
    pub(crate) fn new(contact: SocketAddr) -> Self {
        Self {
            contact,
            host: Mutex::new(None),
        }
    }

    fn holds(&self, host: Uuid) -> bool {
        let mut held = self.host.lock().unwrap_or_else(PoisonError::into_inner);
        *held.get_or_insert(host) == host
    }
}

impl HostFilter for ContactPin {
    fn accept(&self, peer: &Peer) -> bool {
        self.holds(peer.host_id)
    }
}

#[async_trait]
impl AddressTranslator for ContactPin {
    async fn translate_address(&self, _peer: &UntranslatedPeer) -> Result<SocketAddr, TranslationError> {
        Ok(self.contact)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_host_asked_about_is_the_only_one_held() {
        let pin = ContactPin::new(SocketAddr::from(([127, 0, 0, 1], 9042)));
        let (first, second) = (Uuid::from_u128(1), Uuid::from_u128(2));
        assert!(pin.holds(first));
        assert!(!pin.holds(second), "a second node was enabled");
        assert!(pin.holds(first));
    }
}
