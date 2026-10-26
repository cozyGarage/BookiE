use relm4::adw::prelude::*;

use super::ConnectDialog;
use super::toggle_error;
use crate::services::connection_form::{
    EndpointFormState, LocalSocketPathKind, local_socket_options, resolved_postgres_socket_path,
    socket_path_is_absolute,
};

impl ConnectDialog {
    pub(super) fn local_socket_is_file(&self) -> bool {
        self.form.local_socket_path_kind() == Some(LocalSocketPathKind::File)
    }

    pub(super) fn configure_socket_field(&self, kind: Option<LocalSocketPathKind>) {
        if kind == Some(LocalSocketPathKind::File) {
            self.socket_dir.set_title(&crate::tr!("Socket path"));
            if self.socket_dir.text() == "/run/postgresql" {
                self.socket_dir.set_text("/run/mysqld/mysqld.sock");
            }
            return;
        }
        self.socket_dir.set_title(&crate::tr!("Socket directory"));
        if self.socket_dir.text() == "/run/mysqld/mysqld.sock" {
            self.socket_dir.set_text("/run/postgresql");
        }
    }

    pub(super) fn refresh_socket_path(&self, selected: bool) {
        let text = self.socket_dir.text();
        toggle_error(&self.socket_dir, selected && !socket_path_is_absolute(text.as_str()));
        let path = if self.local_socket_is_file() {
            std::path::PathBuf::from(text.as_str())
        } else {
            resolved_postgres_socket_path(text.as_str(), self.port.value() as u16)
        };
        self.resolved_socket.set_subtitle(&path.display().to_string());
    }

    pub(super) fn apply_socket_visibility(&self, endpoint: EndpointFormState) {
        let selected = endpoint.uses_local_socket();
        self.port_row
            .set_visible(endpoint.shows_port() && !(selected && self.local_socket_is_file()));
        self.socket_dir.set_visible(endpoint.shows_socket_rows());
        self.resolved_socket
            .set_visible(endpoint.shows_socket_rows() && !self.local_socket_is_file());
    }

    pub(super) fn selected_socket_options(
        &self,
        kind: Option<LocalSocketPathKind>,
        selected: bool,
    ) -> (Option<std::path::PathBuf>, Option<std::path::PathBuf>) {
        if selected {
            local_socket_options(kind, self.socket_dir.text().as_str())
        } else {
            (None, None)
        }
    }
}
