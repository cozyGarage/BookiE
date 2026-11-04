use super::*;

#[derive(Debug, Clone)]
pub struct StatementOutcome {
    pub identity: StatementIdentity,
    pub sql_preview: String,
    pub elapsed_ms: u128,
    pub kind: StatementOutcomeKind,
    pub pinned: bool,
}

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct StatementIdentity {
    pub statement_index: usize,
    pub digest: [u8; 32],
}

#[derive(Debug, Clone)]
pub enum StatementOutcomeKind {
    Rows(Vec<std::sync::Arc<QueryResult>>),
    Streamed(Box<StreamedResult>),
    Error(StatementFailure),
    NotRun,
}

#[derive(Debug)]
pub enum SqlEditorInput {
    CatalogStatementSucceeded {
        origin: crate::services::catalog::CatalogOrigin,
        driver: String,
        sql: String,
    },
    Run,
    SessionAction {
        session_id: Uuid,
        commit: bool,
    },
    RunWithParameters {
        sql: String,
        anchor: Option<SourceAnchor>,
        planned: PlannedStatements,
        values: std::collections::HashMap<String, bookie_core::Value>,
    },
    LexingRead {
        source: session_lexing::LexingSource,
        sql: String,
        anchor: Option<SourceAnchor>,
        result: Result<Option<bookie_core::sql_syntax::script::LexicalSettings>, String>,
    },
    LexingRefreshed {
        source: session_lexing::LexingSource,
        settings: Option<bookie_core::sql_syntax::script::LexicalSettings>,
    },
    Cancel,
    TogglePin,
    NavigateError(error_navigation::ErrorTarget),
    Panel(bottom_panel::PanelAction),
    SplitToggled(bool),
    ShowOutcomes {
        generation: u64,
        outcomes: Vec<StatementOutcome>,
    },
    Stream(streaming::Input),
    FilterResults(Box<result_tools::FilterRequest>),
    FilterFinished(Box<result_tools::FilterDone>),
    OpenCompare,
    CompareResults(Box<result_compare::CompareRequest>),
    CompareFinished(Box<result_compare::CompareDone>),
    RunWithLongerTimeout,
    ShowCancelled(u64),
    ShowTimedOut {
        generation: u64,
        secs: u32,
    },
    InsertDroppedSql {
        request: DropRequest,
        text: String,
    },
    DroppedSqlFailed {
        request: DropRequest,
        message: String,
    },
    ReplaceQuery(String),
    Format,
    RunAtCursor,
    JumpStatement {
        forward: bool,
    },
    ToggleLineComment,
    ToggleFold,
    InsertSnippet(String),
    UnfoldAll,
    ShowFind,
    Explain,
    Grid(GridMsg),
    SessionToggled(bool),
    SessionOpened {
        connection_id: Uuid,
        connection_identity: crate::services::database_service::ConnectionIdentity,
        session_id: Uuid,
        result: Result<session_mode::OpenedSession, String>,
    },
    ConnectionIdentityChanged,
    CompletionCatalogChanged(SchemaIndex),
    FetchCompletionColumns(Vec<String>),
    CompletionColumnsFetched {
        identity: crate::services::database_service::ConnectionIdentity,
        request: SchemaRequest,
        columns: Result<Vec<String>, ()>,
    },
    SessionState {
        session_id: Uuid,
        transaction_open: bool,
        usable: bool,
    },
    SessionEnd {
        session_id: Uuid,
        commit: bool,
    },
    SessionCommitFinished {
        session_id: Uuid,
        result: Result<bookie_core::SessionCommitOutcome, String>,
    },
    SessionRollbackFinished {
        session_id: Uuid,
        result: Result<(), String>,
    },
    PrepareForTeardown(tokio::sync::oneshot::Sender<Result<(), String>>),
    SessionTeardownFinished(Result<(), String>),
}

#[derive(Debug)]
pub enum SqlEditorOutput {
    CatalogChanged(crate::services::catalog::CatalogOrigin),
    RunStateChanged(bool),
    QueryChanged(String),
    CopyToClipboard(String),
    ShowRowAsJson(String),
    ShowToast(String),
    ExportResults {
        result: std::sync::Arc<QueryResult>,
        name: String,
    },
    ExportStreamedResults(crate::ui::export_dialog::StreamExportRequest),
}
