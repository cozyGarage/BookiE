use std::path::{Path, PathBuf};

use tablepro_core::{AuthMode, DriverCapabilities};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocalSocketPathKind {
    File,
    Directory,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ConnectFormState {
    pub file_based: bool,
    pub supports_integrated: bool,
    pub selected: AuthMode,
    local_socket_path_kind: Option<LocalSocketPathKind>,
}

impl ConnectFormState {
    pub fn from_capabilities(capabilities: DriverCapabilities, selected: AuthMode) -> Self {
        Self {
            file_based: capabilities.is_file_based,
            supports_integrated: capabilities.supports_integrated_auth,
            selected,
            local_socket_path_kind: (!capabilities.is_file_based)
                .then(|| local_socket_path_kind(capabilities))
                .flatten(),
        }
    }

    pub fn mode(self) -> AuthMode {
        if self.shows_method() {
            self.selected
        } else {
            AuthMode::Password
        }
    }

    pub fn shows_method(self) -> bool {
        !self.file_based && self.supports_integrated
    }

    pub fn shows_credentials(self) -> bool {
        !self.file_based && self.mode() == AuthMode::Password
    }

    pub fn local_socket_path_kind(self) -> Option<LocalSocketPathKind> {
        self.local_socket_path_kind
    }

    pub fn endpoint(self, socket_selected: bool) -> EndpointFormState {
        EndpointFormState {
            file_based: self.file_based,
            supports_local_socket: self.local_socket_path_kind.is_some(),
            socket_selected,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EndpointFormState {
    pub file_based: bool,
    pub supports_local_socket: bool,
    pub socket_selected: bool,
}

impl EndpointFormState {
    pub fn shows_endpoint_choice(self) -> bool {
        !self.file_based && self.supports_local_socket
    }

    pub fn uses_local_socket(self) -> bool {
        self.shows_endpoint_choice() && self.socket_selected
    }

    pub fn shows_network(self) -> bool {
        !self.file_based && !self.uses_local_socket()
    }

    pub fn shows_port(self) -> bool {
        !self.file_based
    }

    pub fn shows_socket_rows(self) -> bool {
        self.uses_local_socket()
    }
}

pub fn local_socket_path_kind(capabilities: DriverCapabilities) -> Option<LocalSocketPathKind> {
    match (
        capabilities.supports_local_socket,
        capabilities.supports_local_socket_file,
        capabilities.supports_local_socket_directory,
    ) {
        (true, true, false) => Some(LocalSocketPathKind::File),
        (true, false, true) => Some(LocalSocketPathKind::Directory),
        _ => None,
    }
}

pub fn local_socket_options(kind: Option<LocalSocketPathKind>, endpoint: &str) -> (Option<PathBuf>, Option<PathBuf>) {
    match kind {
        Some(LocalSocketPathKind::Directory) => (Some(PathBuf::from(endpoint)), None),
        Some(LocalSocketPathKind::File) => (None, Some(PathBuf::from(endpoint))),
        None => (None, None),
    }
}

pub fn socket_path_is_absolute(path: &str) -> bool {
    Path::new(path).is_absolute()
}

pub fn resolved_postgres_socket_path(directory: &str, port: u16) -> PathBuf {
    Path::new(directory).join(format!(".s.PGSQL.{port}"))
}

#[cfg(test)]
mod tests {
    use tablepro_core::{AuthMode, DriverCapabilities};

    use super::*;

    fn capabilities() -> DriverCapabilities {
        DriverCapabilities::default()
    }

    #[test]
    fn auth_mode_and_credential_visibility_follow_driver_capabilities() {
        let capabilities = DriverCapabilities {
            supports_integrated_auth: true,
            ..capabilities()
        };
        let password = ConnectFormState::from_capabilities(capabilities, AuthMode::Password);
        let kerberos = ConnectFormState::from_capabilities(capabilities, AuthMode::Kerberos);

        assert_eq!(password.mode(), AuthMode::Password);
        assert!(password.shows_method());
        assert!(password.shows_credentials());
        assert_eq!(kerberos.mode(), AuthMode::Kerberos);
        assert!(kerberos.shows_method());
        assert!(!kerberos.shows_credentials());
    }

    #[test]
    fn file_based_or_unsupported_auth_forces_password_without_credentials() {
        let file_based = DriverCapabilities {
            is_file_based: true,
            supports_integrated_auth: true,
            ..capabilities()
        };
        let state = ConnectFormState::from_capabilities(file_based, AuthMode::Kerberos);

        assert_eq!(state.mode(), AuthMode::Password);
        assert!(!state.shows_method());
        assert!(!state.shows_credentials());
    }

    #[test]
    fn unsupported_integrated_auth_ignores_a_stale_kerberos_selection() {
        let state = ConnectFormState::from_capabilities(capabilities(), AuthMode::Kerberos);

        assert_eq!(state.mode(), AuthMode::Password);
        assert!(!state.shows_method());
        assert!(state.shows_credentials());
    }

    #[test]
    fn endpoint_visibility_switches_between_network_and_local_socket() {
        let capabilities = DriverCapabilities {
            supports_local_socket: true,
            supports_local_socket_directory: true,
            ..capabilities()
        };
        let state = ConnectFormState::from_capabilities(capabilities, AuthMode::Password);
        let socket = state.endpoint(true);
        let network = state.endpoint(false);

        assert!(socket.shows_endpoint_choice());
        assert!(socket.uses_local_socket());
        assert!(socket.shows_socket_rows());
        assert!(!socket.shows_network());
        assert!(socket.shows_port());
        assert!(!network.uses_local_socket());
        assert!(!network.shows_socket_rows());
        assert!(network.shows_network());
    }

    #[test]
    fn local_socket_path_shape_comes_from_capabilities() {
        let file = DriverCapabilities {
            supports_local_socket: true,
            supports_local_socket_file: true,
            ..capabilities()
        };
        let directory = DriverCapabilities {
            supports_local_socket: true,
            supports_local_socket_directory: true,
            ..capabilities()
        };
        assert_eq!(local_socket_path_kind(file), Some(LocalSocketPathKind::File));
        assert_eq!(local_socket_path_kind(directory), Some(LocalSocketPathKind::Directory));
    }

    #[test]
    fn ambiguous_or_inconsistent_socket_capabilities_do_not_offer_local_socket_mode() {
        let ambiguous = DriverCapabilities {
            supports_local_socket: true,
            supports_local_socket_file: true,
            supports_local_socket_directory: true,
            ..capabilities()
        };
        let inconsistent = DriverCapabilities {
            supports_local_socket_file: true,
            ..capabilities()
        };
        for capabilities in [ambiguous, inconsistent] {
            let state = ConnectFormState::from_capabilities(capabilities, AuthMode::Password);
            assert_eq!(state.local_socket_path_kind(), None);
            assert!(!state.endpoint(true).shows_endpoint_choice());
            assert!(!state.endpoint(true).uses_local_socket());
        }
    }

    #[test]
    fn a_driver_without_socket_support_stays_on_the_network_form() {
        let state = ConnectFormState::from_capabilities(capabilities(), AuthMode::Password);
        let endpoint = state.endpoint(true);

        assert!(!endpoint.shows_endpoint_choice());
        assert!(!endpoint.uses_local_socket());
        assert!(endpoint.shows_network());
        assert!(endpoint.shows_port());
        assert!(!endpoint.shows_socket_rows());
    }

    #[test]
    fn file_based_drivers_do_not_offer_endpoint_or_local_socket_options() {
        let capabilities = DriverCapabilities {
            is_file_based: true,
            supports_local_socket: true,
            supports_local_socket_directory: true,
            ..capabilities()
        };
        let state = ConnectFormState::from_capabilities(capabilities, AuthMode::Password);

        assert!(!state.endpoint(true).shows_endpoint_choice());
        assert!(!state.endpoint(true).shows_network());
        assert!(!state.endpoint(true).shows_port());
        assert_eq!(
            local_socket_options(state.local_socket_path_kind(), "/run/postgresql"),
            (None, None)
        );
    }

    #[test]
    fn selected_socket_options_preserve_the_declared_path_shape() {
        let file = local_socket_options(Some(LocalSocketPathKind::File), "/run/mysqld/mysqld.sock");
        let directory = local_socket_options(Some(LocalSocketPathKind::Directory), "/run/postgresql");

        assert_eq!(file, (None, Some(std::path::PathBuf::from("/run/mysqld/mysqld.sock"))));
        assert_eq!(directory, (Some(std::path::PathBuf::from("/run/postgresql")), None));
        assert_eq!(local_socket_options(None, "/tmp/socket"), (None, None));
    }

    #[test]
    fn local_socket_paths_must_be_absolute() {
        assert!(socket_path_is_absolute("/run/postgresql"));
        assert!(socket_path_is_absolute("/tmp/mysqld.sock"));
        assert!(!socket_path_is_absolute("run/postgresql"));
        assert!(!socket_path_is_absolute(""));
        assert!(!socket_path_is_absolute("~/sockets"));
    }

    #[test]
    fn postgres_socket_resolution_uses_the_directory_and_port() {
        assert_eq!(
            resolved_postgres_socket_path("/run/postgresql", 5432),
            std::path::PathBuf::from("/run/postgresql/.s.PGSQL.5432")
        );
        assert_eq!(
            resolved_postgres_socket_path("/tmp", 6543),
            std::path::PathBuf::from("/tmp/.s.PGSQL.6543")
        );
    }
}
