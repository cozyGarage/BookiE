use std::cell::RefCell;
use std::rc::Rc;

use relm4::adw::prelude::*;
use relm4::{adw, gtk};
use tokio_util::sync::CancellationToken;

use bookie_core::{
    ActivityQuery, ActivityUnsupported, QueryRowSet, Value, activity_kinds, activity_sql, parse_session_id,
};

use crate::services::database_service::DatabaseService;
use crate::services::preferences::PreferencesStore;
use bookie_app_services::tr;

fn validate_active_connection(
    parent: &gtk::Window,
    connection_id: Option<uuid::Uuid>,
    database: &DatabaseService,
) -> Option<(uuid::Uuid, String)> {
    let Some(connection) = connection_id else {
        let alert = adw::AlertDialog::new(
            Some(&tr!("No active connection")),
            Some(&tr!("Open a connection before viewing server activity.")),
        );
        alert.add_response("ok", &tr!("OK"));
        alert.present(Some(parent));
        return None;
    };
    let Some(meta) = database.metadata(connection) else {
        let alert = adw::AlertDialog::new(
            Some(&tr!("No active connection")),
            Some(&tr!("Open a connection before viewing server activity.")),
        );
        alert.add_response("ok", &tr!("OK"));
        alert.present(Some(parent));
        return None;
    };
    Some((connection, meta.driver_id))
}

fn build_activity_dialog() -> (adw::Dialog, adw::WrapBox, gtk::TextView, gtk::Label) {
    let dialog = adw::Dialog::builder().title(tr!("Server Activity")).build();
    dialog.set_content_width(900);
    dialog.set_content_height(560);

    let toolbar = adw::ToolbarView::new();
    let header = adw::HeaderBar::new();
    toolbar.add_top_bar(&header);

    let box_ = gtk::Box::new(gtk::Orientation::Vertical, 0);
    let button_row = adw::WrapBox::builder()
        .child_spacing(8)
        .line_spacing(8)
        .margin_start(12)
        .margin_end(12)
        .margin_top(8)
        .margin_bottom(8)
        .build();

    let scrolled = gtk::ScrolledWindow::builder().vexpand(true).hexpand(true).build();
    let text = gtk::TextView::builder()
        .editable(false)
        .monospace(true)
        .wrap_mode(gtk::WrapMode::None)
        .build();
    scrolled.set_child(Some(&text));

    let status = gtk::Label::builder()
        .label(tr!("Choose a query above."))
        .xalign(0.0)
        .margin_start(12)
        .margin_end(12)
        .margin_bottom(8)
        .build();
    status.add_css_class("dim-label");

    box_.append(&button_row);
    box_.append(&scrolled);
    box_.append(&status);
    toolbar.set_content(Some(&box_));
    dialog.set_child(Some(&toolbar));
    (dialog, button_row, text, status)
}

fn spawn_activity_query(
    conn: std::sync::Arc<dyn bookie_core::Connection>,
    sql: String,
    text_buf: gtk::TextBuffer,
    status_l: gtk::Label,
    token: CancellationToken,
    timeout_secs: u32,
) {
    glib::spawn_future_local(async move {
        let control = crate::services::operation_control::bounded_with(timeout_secs, token.clone());
        match conn.query_controlled(&sql, &control).await {
            Ok(result) => {
                if token.is_cancelled() {
                    return;
                }
                let cols: Vec<String> = result.columns.iter().map(|c| c.name.clone()).collect();
                let rendered = render_result(&cols, &result.rows);
                let n = result.rows.len();
                text_buf.set_text(&rendered);
                status_l.set_text(&bookie_app_services::trn!("{n} row", "{n} rows", n).replace("{n}", &n.to_string()));
            }
            Err(e) => {
                if token.is_cancelled() {
                    return;
                }
                text_buf.set_text(&e.to_string());
                status_l.set_text(&tr!("Query failed"));
            }
        }
    });
}

struct ActivityQueryButtonContext<'a> {
    button_row: &'a adw::WrapBox,
    text: &'a gtk::TextView,
    status: &'a gtk::Label,
    driver_id: &'a str,
    connection: uuid::Uuid,
    database: &'a std::sync::Arc<DatabaseService>,
    preferences: &'a PreferencesStore,
    in_flight: &'a Rc<RefCell<CancellationToken>>,
}

fn append_activity_query_buttons(context: ActivityQueryButtonContext<'_>) {
    let ActivityQueryButtonContext {
        button_row,
        text,
        status,
        driver_id,
        connection,
        database,
        preferences,
        in_flight,
    } = context;
    let actions = [
        (tr!("Sessions"), ActivityQuery::Sessions),
        (tr!("Blocking locks"), ActivityQuery::BlockingLocks),
        (tr!("Long-running"), ActivityQuery::LongRunning),
        (tr!("Replication lag"), ActivityQuery::ReplicationLag),
        (tr!("Server variables"), ActivityQuery::ServerVariables),
        (tr!("Server status"), ActivityQuery::ServerStatus),
    ];
    let supported = activity_kinds(driver_id);
    for (label, kind) in actions {
        let btn = gtk::Button::with_label(&label);
        let text_buf = text.buffer();
        let status_l = status.clone();
        let driver = driver_id.to_string();
        if !supported.contains(&kind) {
            btn.set_sensitive(false);
            btn.set_tooltip_text(Some(&unsupported_text(driver_id)));
        }
        let in_flight_for_query = in_flight.clone();
        let database_for_query = database.clone();
        let preferences_for_query = preferences.clone();
        btn.connect_clicked(move |_| {
            let sql = match activity_sql(&driver, kind, None) {
                Ok(sql) => sql,
                Err(reason) => {
                    status_l.set_text(&reason_text(reason));
                    return;
                }
            };
            let Some(conn) = database_for_query.get(connection) else {
                status_l.set_text(&tr!("Connection closed."));
                return;
            };
            status_l.set_text(&tr!("Running…"));
            let token = crate::services::request_generation::replace_in_flight(&in_flight_for_query);
            let timeout_secs = crate::services::operation_control::timeout_for(
                &preferences_for_query,
                &database_for_query,
                Some(connection),
            );
            spawn_activity_query(conn, sql, text_buf.clone(), status_l.clone(), token, timeout_secs);
        });
        button_row.append(&btn);
    }
}

fn spawn_kill_session(
    conn: std::sync::Arc<dyn bookie_core::Connection>,
    sql: String,
    text_buf: gtk::TextBuffer,
    status_l: gtk::Label,
    flight: crate::services::single_flight::Flight,
    token: CancellationToken,
    timeout_secs: u32,
) {
    glib::spawn_future_local(async move {
        let _flight = flight;
        let control = crate::services::operation_control::bounded_with(timeout_secs, token);
        match conn.execute_controlled(&sql, &control).await {
            Ok(r) => {
                text_buf.set_text(&format!("Kill issued; rows_affected={}", r.rows_affected));
                status_l.set_text(&tr!("Done"));
            }
            Err(e) => {
                text_buf.set_text(&e.to_string());
                status_l.set_text(&tr!("Kill failed"));
            }
        }
    });
}

fn append_kill_controls(
    button_row: &adw::WrapBox,
    text: &gtk::TextView,
    status: &gtk::Label,
    driver_id: &str,
    connection: uuid::Uuid,
    database: &std::sync::Arc<DatabaseService>,
    preferences: &PreferencesStore,
) {
    let supported = activity_kinds(driver_id);
    let kill_entry = gtk::Entry::builder()
        .placeholder_text(tr!("Session id to kill"))
        .width_chars(12)
        .build();
    let kill_btn = gtk::Button::with_label(&tr!("Kill"));
    kill_btn.add_css_class("destructive-action");
    if !supported.contains(&ActivityQuery::KillSession) {
        kill_entry.set_sensitive(false);
        kill_btn.set_sensitive(false);
        kill_btn.set_tooltip_text(Some(&unsupported_text(driver_id)));
    }
    let text_buf = text.buffer();
    let status_l = status.clone();
    let driver = driver_id.to_string();
    let kill_entry_c = kill_entry.clone();
    let kill_flight = crate::services::single_flight::SingleFlight::default();
    let database_for_kill = database.clone();
    let preferences_for_kill = preferences.clone();
    kill_btn.connect_clicked(move |_| {
        let id = match parse_session_id(&kill_entry_c.text()) {
            Some(id) => id,
            None => {
                status_l.set_text(&tr!("Session id must be a positive integer."));
                return;
            }
        };
        let sql = match activity_sql(&driver, ActivityQuery::KillSession, Some(id)) {
            Ok(sql) => sql,
            Err(reason) => {
                status_l.set_text(&reason_text(reason));
                return;
            }
        };
        let Some(conn) = database_for_kill.get(connection) else {
            return;
        };
        let Some(flight) = kill_flight.try_begin() else {
            status_l.set_text(&tr!("A kill is already running."));
            return;
        };
        let token = CancellationToken::new();
        let timeout_secs = crate::services::operation_control::timeout_for(
            &preferences_for_kill,
            &database_for_kill,
            Some(connection),
        );
        spawn_kill_session(
            conn,
            sql,
            text_buf.clone(),
            status_l.clone(),
            flight,
            token,
            timeout_secs,
        );
    });
    button_row.append(&kill_entry);
    button_row.append(&kill_btn);
}

pub fn present(
    parent: &gtk::Window,
    connection_id: Option<uuid::Uuid>,
    database: &std::sync::Arc<DatabaseService>,
    preferences: &PreferencesStore,
) {
    let Some((connection, driver_id)) = validate_active_connection(parent, connection_id, database) else {
        return;
    };
    let (dialog, button_row, text, status) = build_activity_dialog();
    let in_flight = Rc::new(RefCell::new(CancellationToken::new()));

    append_activity_query_buttons(ActivityQueryButtonContext {
        button_row: &button_row,
        text: &text,
        status: &status,
        driver_id: &driver_id,
        connection,
        database,
        preferences,
        in_flight: &in_flight,
    });
    append_kill_controls(
        &button_row,
        &text,
        &status,
        &driver_id,
        connection,
        database,
        preferences,
    );

    let in_flight_for_close = in_flight;
    dialog.connect_closed(move |_| {
        in_flight_for_close.borrow().cancel();
    });
    dialog.present(Some(parent));
}

/// The dialog offers every activity view, so a button the engine cannot
/// answer has to say which of the two reasons applies rather than a bare
/// "not supported".
fn reason_text(reason: ActivityUnsupported) -> String {
    match reason {
        ActivityUnsupported::Engine => tr!("This engine has no server activity views."),
        ActivityUnsupported::Query => tr!("This engine does not report this kind of activity."),
        ActivityUnsupported::MissingSessionId => tr!("Session id must be a positive integer."),
    }
}

fn unsupported_text(driver_id: &str) -> String {
    if activity_kinds(driver_id).is_empty() {
        reason_text(ActivityUnsupported::Engine)
    } else {
        reason_text(ActivityUnsupported::Query)
    }
}

fn render_result<R: QueryRowSet + ?Sized>(columns: &[String], rows: &R) -> String {
    if columns.is_empty() {
        return String::new();
    }
    let row_count = rows.row_count();
    let mut string_rows = Vec::with_capacity(row_count);
    let mut widths: Vec<usize> = columns.iter().map(|c| c.chars().count()).collect();

    for row_index in 0..row_count {
        let Some(row) = rows.row(row_index) else { continue };
        let cells: Vec<String> = row.iter().map(value_to_string).collect();
        for (i, cell) in cells.iter().enumerate() {
            if i < widths.len() {
                widths[i] = widths[i].max(cell.chars().count());
            }
        }
        string_rows.push(cells);
    }

    let mut out = String::new();
    for (i, col) in columns.iter().enumerate() {
        if i > 0 {
            out.push_str("  ");
        }
        let w = widths.get(i).copied().unwrap_or(col.chars().count());
        out.push_str(col);
        if col.chars().count() < w {
            out.push_str(&" ".repeat(w - col.chars().count()));
        }
    }
    out.push('\n');

    for (i, w) in widths.iter().enumerate() {
        if i > 0 {
            out.push_str("  ");
        }
        out.push_str(&"-".repeat(*w));
    }
    out.push('\n');

    for row in string_rows {
        for (i, cell) in row.iter().enumerate() {
            if i > 0 {
                out.push_str("  ");
            }
            let w = widths.get(i).copied().unwrap_or(cell.chars().count());
            out.push_str(cell);
            if cell.chars().count() < w {
                out.push_str(&" ".repeat(w - cell.chars().count()));
            }
        }
        out.push('\n');
    }
    out
}

fn value_to_string(v: &Value) -> String {
    match v {
        Value::Null => "NULL".into(),
        Value::Bool(b) => b.to_string(),
        Value::Int(i) => i.to_string(),
        Value::Float(f) => f.to_string(),
        Value::Text(s) => s.replace(['\t', '\n'], " "),
        Value::Bytes(b) => format!("<{} bytes>", b.len()),
        Value::Date(d) => d.to_string(),
        Value::Time(t) => t.to_string(),
        Value::DateTime(dt) => dt.to_string(),
        Value::TimestampTz(ts) => ts.to_rfc3339(),
        Value::Decimal(d) => d.to_string(),
        Value::Uuid(u) => u.to_string(),
        Value::Json(j) => j.to_string(),
        Value::JsonText(j) => j.clone(),
        Value::Undecodable(type_name) => format!("<undecodable {type_name}>"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_binary_cell_reads_as_its_byte_count_not_a_hex_escape() {
        assert_eq!(value_to_string(&Value::Bytes(vec![0xAB; 16])), "<16 bytes>");
        assert_eq!(value_to_string(&Value::Bytes(Vec::new())), "<0 bytes>");
    }

    #[test]
    fn render_result_aligns_columns_with_headers_and_separator() {
        let cols = vec!["pid".to_string(), "query".to_string(), "state".to_string()];
        let rows = vec![
            vec![
                Value::Int(1234),
                Value::Text("SELECT 1".to_string()),
                Value::Text("active".to_string()),
            ],
            vec![
                Value::Int(56),
                Value::Text("idle in transaction".to_string()),
                Value::Text("idle".to_string()),
            ],
        ];
        let rendered = render_result(&cols, &rows);
        let lines: Vec<&str> = rendered.lines().collect();
        assert_eq!(lines.len(), 4);
        assert_eq!(lines[0], "pid   query                state ");
        assert_eq!(lines[1], "----  -------------------  ------");
        assert_eq!(lines[2], "1234  SELECT 1             active");
        assert_eq!(lines[3], "56    idle in transaction  idle  ");
    }
}
