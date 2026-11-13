mod audit_journal;
mod connection_bundle;
mod connection_bundle_crypto;
mod connection_color;
mod connection_import_another_redis;
mod connection_import_datagrip;
mod connection_import_dbeaver;
mod connection_import_navicat;
mod connection_import_peer;
mod connection_import_resp;
mod connection_organization;
mod connection_url;
mod connections;
mod error;
mod favorites;
mod file_access;
mod legacy_state;
mod paths;
pub mod query_history;
mod secrets;

pub use audit_journal::{AuditJournal, AuditJournalRecovery, LegacyJournalRotation, sample_event};
pub use connection_bundle::{
    BUNDLE_EXCLUDED_FIELDS, BUNDLE_FORMAT, BUNDLE_INCLUDED_FIELDS, BUNDLE_VERSION, BundleBody, BundleConnection,
    BundleError, BundleExport, BundleOrganization, BundleSecrets, BundleSshSecret, BundleSshSecretKind,
    EncryptedBundle, ImportDisposition, ImportItem, ImportPlan, MAX_BUNDLE_BYTES, MAX_BUNDLE_CONNECTIONS,
    MAX_SSH_HOPS as BUNDLE_MAX_SSH_HOPS, ParsedBundle, bundle_organization, collect_bundle_secrets, export_encrypted,
    export_plaintext, forget_imported_bundle_secrets, forget_imported_secrets, parse_bundle, plan_import,
    store_bundle_secrets,
};
pub use connection_color::{
    CONNECTION_COLORS, CONNECTION_STRIP_CLASSES, connection_color, connection_color_css_class, connection_strip_class,
};
pub use connection_import_another_redis::{MAX_ANOTHER_REDIS_BYTES, parse_another_redis_connections};
pub use connection_import_datagrip::{MAX_DATAGRIP_BYTES, parse_datagrip_data_sources};
pub use connection_import_dbeaver::{MAX_DBEAVER_BYTES, parse_dbeaver_data_sources};
pub use connection_import_navicat::{MAX_NAVICAT_BYTES, parse_navicat_connections};
pub use connection_import_peer::PeerImport;
pub use connection_import_resp::{MAX_RESP_BYTES, looks_like_resp_export, parse_resp_connections};
pub use connection_organization::{
    ConnectionOrganization, ConnectionOrganizationIndex, GROUP_PATH_SEPARATOR, GroupTreeNode, MAX_GROUP_DEPTH,
    MAX_LABEL_LEN, MAX_ORGANIZED_CONNECTIONS, MAX_TAGS_PER_CONNECTION, arrange_connections, connection_group_tree,
    connection_matches_filter, join_group_path, load_organization, parse_group_path, save_organization,
    validate_group_path,
};
pub use connection_url::{ParsedConnectionUrl, parse_connection_url};
pub use connections::{
    SavedConnection, SavedSshAuth, SavedSshConfig, SshClient, apply_import, delete_connection, duplicate_connection,
    load_connections, restore_connection, save_connections, touch_last_opened, update_connection,
};
pub use error::{KeyringFailure, StorageError};
pub use favorites::{
    SavedQuery, SavedQueryFolder, SavedQueryLibrary, delete_favorite, delete_saved_query_folder, load_favorites,
    load_saved_query_library, matches_filter, organize_saved_query, rank_favorites, save_favorite,
    save_saved_query_folder, touch_favorite,
};
pub use legacy_state::migrate_legacy_state;
pub use paths::{config_path, connections_config_path, data_path, secret_schema, storage_dir_name};
pub use secrets::{
    SshHopSecretKind, delete_mcp_token, delete_password, delete_ssh_hop_passphrase, delete_ssh_hop_password,
    delete_ssh_passphrase, delete_ssh_password, load_mcp_token, load_password, load_ssh_hop_passphrase,
    load_ssh_hop_password, load_ssh_passphrase, load_ssh_password, store_mcp_token, store_password,
    store_ssh_hop_passphrase, store_ssh_hop_password, store_ssh_passphrase, store_ssh_password,
};
