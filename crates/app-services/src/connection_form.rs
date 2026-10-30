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
#[path = "connection_form_tests.rs"]
mod tests;
