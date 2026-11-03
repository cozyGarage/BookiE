use std::sync::Arc;

use bookie_app_services::stream_result_spool::StreamResultSpoolManager;
use bookie_core::{ColumnInfo, QueryResult, Value};
use relm4::gtk;
use relm4::gtk::prelude::*;

use super::widget::{ToolsTarget, append};
use crate::ui::editor::stream_pages::page_row_starts_from_counts as page_row_starts;
use crate::ui::editor::stream_pages::{ResidentPageStore, StreamTerminal, StreamedResult};
use crate::ui::editor::stream_view::page_widget_name;
use crate::ui::editor::{SqlEditorInput, StatementIdentity};

const PAGES: usize = 3;
const PAGE_ROWS: usize = 50;
const GENERATION: u64 = 5;

fn text_column(name: &str) -> ColumnInfo {
    ColumnInfo {
        name: name.into(),
        data_type: "text".into(),
        nullable: true,
        primary_key: false,
        is_auto_increment: false,
        default_value: None,
        is_generated: false,
        comment: None,
        collation: None,
        enum_type: None,
        domain_type: None,
    }
}

fn spooled_result() -> StreamedResult {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let spool = StreamResultSpoolManager::default().create().unwrap();
    let columns = vec![text_column("id"), text_column("label")];
    for page in 0..PAGES {
        let rows = (0..PAGE_ROWS)
            .map(|row| {
                let id = (page * PAGE_ROWS + row) as i64;
                let label = if page == 2 && row == 7 {
                    "needle-xyz".to_string()
                } else {
                    format!("plain {id}")
                };
                vec![Value::Int(id), Value::Text(label)]
            })
            .collect::<Vec<_>>();
        let result = Arc::new(QueryResult {
            columns: columns.clone(),
            rows: rows.into(),
            truncated: false,
        });
        runtime.block_on(spool.append_page(page as u64, result)).unwrap();
    }
    let counts = vec![PAGE_ROWS; PAGES];
    StreamedResult {
        pages: Vec::new(),
        page_indices: Vec::new(),
        page_row_starts: page_row_starts(&counts),
        retained_rows: 0,
        columns,
        total_pages: PAGES as u64,
        total_rows: PAGES * PAGE_ROWS,
        terminal: StreamTerminal::Complete,
        export: None,
        generation: GENERATION,
        spool: Some(spool),
        resident_pages: ResidentPageStore::default(),
        page_row_counts: counts,
    }
}

struct Fixture {
    holder: gtk::Box,
    pages: gtk::Stack,
    receiver: relm4::Receiver<SqlEditorInput>,
    sender: relm4::Sender<SqlEditorInput>,
    result: StreamedResult,
    _window: gtk::Window,
}

fn identity_number(number: usize) -> StatementIdentity {
    StatementIdentity {
        statement_index: number,
        ..StatementIdentity::default()
    }
}

fn mount(
    holder: &gtk::Box,
    pages: &gtk::Stack,
    result: &StreamedResult,
    identity: StatementIdentity,
    sender: &relm4::Sender<SqlEditorInput>,
) -> relm4::adw::WrapBox {
    let header = relm4::adw::WrapBox::new();
    holder.prepend(&header);
    append(ToolsTarget {
        holder,
        header: &header,
        pages,
        result,
        identity,
        sender,
    });
    header
}

fn fixture() -> Fixture {
    fixture_for(StatementIdentity::default())
}

fn fixture_for(identity: StatementIdentity) -> Fixture {
    gtk::init().unwrap();
    let result = spooled_result();
    let holder = gtk::Box::new(gtk::Orientation::Vertical, 0);
    let pages = gtk::Stack::new();
    for page in 0..PAGES as u64 {
        let name = page_widget_name(GENERATION, page);
        pages.add_named(&gtk::Box::new(gtk::Orientation::Vertical, 0), Some(&name));
    }
    let (sender, receiver) = relm4::channel();
    mount(&holder, &pages, &result, identity, &sender);
    holder.append(&pages);
    let window = gtk::Window::builder().child(&holder).build();
    window.present();
    Fixture {
        holder,
        pages,
        receiver,
        sender,
        result,
        _window: window,
    }
}

fn named<T: IsA<gtk::Widget>>(root: &gtk::Box, name: &str) -> T {
    super::jump::find_descendant(root.upcast_ref(), |widget| widget.widget_name() == name)
        .unwrap_or_else(|| panic!("{name} is missing"))
}

fn pump_until(condition: impl Fn() -> bool, what: &str) {
    let context = gtk::glib::MainContext::default();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while !condition() {
        while context.iteration(false) {}
        assert!(std::time::Instant::now() < deadline, "timed out waiting for {what}");
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}

fn search(fixture: &Fixture, needle: &str) {
    let entry: gtk::SearchEntry = named(&fixture.holder, "tp-result-find-entry");
    entry.set_text(needle);
    entry.emit_activate();
}

#[test]
#[ignore = "requires an isolated GTK display"]
fn find_searches_every_page_and_jumps_to_the_page_with_the_match() {
    let fixture = fixture();
    let status: gtk::Label = named(&fixture.holder, "tp-result-find-status");
    search(&fixture, "NEEDLE-XYZ");
    pump_until(|| status.text().starts_with("Match 1 of 1"), "the match status");
    assert_eq!(status.text(), "Match 1 of 1 · row 108");
    assert_eq!(
        fixture.pages.visible_child_name().as_deref(),
        Some(page_widget_name(GENERATION, 2).as_str())
    );
}

#[test]
#[ignore = "requires an isolated GTK display"]
fn find_reports_no_match_and_a_new_query_clears_the_old_status() {
    let fixture = fixture();
    let status: gtk::Label = named(&fixture.holder, "tp-result-find-status");
    search(&fixture, "absent");
    pump_until(|| status.text() == "No match", "the no-match status");
    let entry: gtk::SearchEntry = named(&fixture.holder, "tp-result-find-entry");
    entry.set_text("absent again");
    pump_until(|| status.text().is_empty(), "the cleared status");
}

#[test]
#[ignore = "requires an isolated GTK display"]
fn find_buttons_step_through_matches_and_wrap() {
    let fixture = fixture();
    let status: gtk::Label = named(&fixture.holder, "tp-result-find-status");
    search(&fixture, "plain 1");
    pump_until(|| status.text().starts_with("Match 1 of"), "the first match");
    let next: gtk::Button = named(&fixture.holder, "tp-result-find-next");
    let previous: gtk::Button = named(&fixture.holder, "tp-result-find-previous");
    next.emit_clicked();
    assert!(status.text().starts_with("Match 2 of"), "{}", status.text());
    previous.emit_clicked();
    previous.emit_clicked();
    assert!(status.text().starts_with("Match 60 of 60"), "{}", status.text());
}

#[test]
#[ignore = "requires an isolated GTK display"]
fn show_matching_rows_requests_a_filter_and_shows_its_outcome() {
    let fixture = fixture();
    let filter: gtk::Button = named(&fixture.holder, "tp-result-filter-button");
    let status: gtk::Label = named(&fixture.holder, "tp-result-find-status");
    filter.emit_clicked();
    assert_eq!(status.text(), "Enter text to filter by");
    let entry: gtk::SearchEntry = named(&fixture.holder, "tp-result-find-entry");
    entry.set_text("needle");
    filter.emit_clicked();
    assert!(!filter.is_sensitive());
    let Some(SqlEditorInput::FilterResults(request)) = fixture.receiver.recv_sync() else {
        panic!("a filter request was not sent")
    };
    assert_eq!(request.label, "needle");
    request.ticket.finish("No rows match".into());
    pump_until(|| filter.is_sensitive(), "the filter button to re-enable");
    assert_eq!(status.text(), "No rows match");
}

#[test]
#[ignore = "requires an isolated GTK display"]
fn cancel_is_hidden_when_idle_and_cancels_a_running_filter() {
    let fixture = fixture();
    let stop: gtk::Button = named(&fixture.holder, "tp-result-find-stop");
    assert!(!stop.is_visible());
    let entry: gtk::SearchEntry = named(&fixture.holder, "tp-result-find-entry");
    entry.set_text("needle");
    let filter: gtk::Button = named(&fixture.holder, "tp-result-filter-button");
    filter.emit_clicked();
    assert!(stop.is_visible());
    let Some(SqlEditorInput::FilterResults(request)) = fixture.receiver.recv_sync() else {
        panic!("a filter request was not sent")
    };
    stop.emit_clicked();
    assert!(request.ticket.cancel().is_cancelled());
}

#[test]
#[ignore = "requires an isolated GTK display"]
fn pressing_enter_while_a_filter_runs_keeps_stop_and_starts_no_search() {
    let fixture = fixture();
    let entry: gtk::SearchEntry = named(&fixture.holder, "tp-result-find-entry");
    let stop: gtk::Button = named(&fixture.holder, "tp-result-find-stop");
    entry.set_text("needle");
    let filter: gtk::Button = named(&fixture.holder, "tp-result-filter-button");
    filter.emit_clicked();
    let Some(SqlEditorInput::FilterResults(request)) = fixture.receiver.recv_sync() else {
        panic!("a filter request was not sent")
    };
    entry.emit_activate();
    for _ in 0..20 {
        gtk::glib::MainContext::default().iteration(false);
    }
    assert!(stop.is_visible());
    assert!(!filter.is_sensitive());
    assert!(!request.ticket.cancel().is_cancelled());
    request.ticket.finish("done".into());
}

#[test]
#[ignore = "requires an isolated GTK display"]
fn a_filter_already_running_on_the_result_refuses_a_second_one() {
    let fixture = fixture();
    let running = Arc::new(super::ticket::FilterTicket::default());
    super::ticket::register(StatementIdentity::default(), &running);
    let entry: gtk::SearchEntry = named(&fixture.holder, "tp-result-find-entry");
    entry.set_text("needle");
    let filter: gtk::Button = named(&fixture.holder, "tp-result-filter-button");
    filter.emit_clicked();
    assert!(filter.is_sensitive());
    let stop: gtk::Button = named(&fixture.holder, "tp-result-find-stop");
    stop.emit_clicked();
    assert!(running.cancel().is_cancelled());
    running.finish(String::new());
}

fn header_window(width: i32) -> (gtk::Window, relm4::adw::WrapBox, gtk::Box) {
    gtk::init().unwrap();
    let result = spooled_result();
    let pages = gtk::Stack::new();
    for page in 0..PAGES as u64 {
        let name = page_widget_name(GENERATION, page);
        pages.add_titled(
            &gtk::Box::new(gtk::Orientation::Vertical, 0),
            Some(&name),
            "Rows 1-5,000",
        );
    }
    let progress = gtk::Label::new(Some(
        "Complete · Received 1,000,000 rows · 200 pages\nEarlier pages can be loaded from temporary storage.",
    ));
    let header = crate::ui::editor::stream_view::build_header(&progress);
    header.append(&crate::ui::editor::stream_view::build_pager(&pages));
    let holder = gtk::Box::new(gtk::Orientation::Vertical, 0);
    holder.append(&header);
    let (sender, _receiver) = relm4::channel();
    append(ToolsTarget {
        holder: &holder,
        header: &header,
        pages: &pages,
        result: &result,
        identity: StatementIdentity::default(),
        sender: &sender,
    });
    header.append(&gtk::Button::with_label("Export all rows…"));
    header.append(&gtk::ToggleButton::with_label("Continuous scrolling"));
    let grid = gtk::Box::new(gtk::Orientation::Vertical, 0);
    grid.set_vexpand(true);
    holder.append(&grid);
    let window = gtk::Window::builder()
        .default_width(width)
        .default_height(500)
        .child(&holder)
        .build();
    window.present();
    let context = gtk::glib::MainContext::default();
    for _ in 0..40 {
        while context.iteration(false) {}
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    (window, header, grid)
}

fn child_bounds(header: &relm4::adw::WrapBox) -> Vec<(String, gtk::graphene::Rect)> {
    let mut bounds = Vec::new();
    let mut child = header.first_child();
    while let Some(widget) = child {
        child = widget.next_sibling();
        if widget.is_visible() {
            let rect = widget.compute_bounds(header).unwrap();
            bounds.push((format!("{} {}", widget.type_().name(), widget.widget_name()), rect));
        }
    }
    bounds
}

fn overlaps(a: &gtk::graphene::Rect, b: &gtk::graphene::Rect) -> bool {
    let separated_x = a.x() + a.width() <= b.x() + 0.5 || b.x() + b.width() <= a.x() + 0.5;
    let separated_y = a.y() + a.height() <= b.y() + 0.5 || b.y() + b.height() <= a.y() + 0.5;
    !(separated_x || separated_y)
}

fn assert_header_layout(width: i32) {
    let (window, header, grid) = header_window(width);
    let bounds = child_bounds(&header);
    assert!(bounds.len() >= 6, "{bounds:?}");
    for (index, (name, rect)) in bounds.iter().enumerate() {
        assert!(rect.x() >= -0.5, "{name} starts left of the header");
        assert!(
            rect.x() + rect.width() <= f64::from(header.width()) as f32 + 0.5,
            "{name} is clipped at {width}px: {rect:?} in {}",
            header.width()
        );
        for (other, other_rect) in &bounds[index + 1..] {
            assert!(!overlaps(rect, other_rect), "{name} overlaps {other} at {width}px");
        }
    }
    assert!(
        header.height() < 220,
        "the header takes {} px at {width}px",
        header.height()
    );
    assert!(grid.height() > 200, "the grid keeps {} px at {width}px", grid.height());
    window.destroy();
}

#[test]
#[ignore = "requires an isolated GTK display"]
fn the_result_header_wraps_without_overlap_at_920_px() {
    assert_header_layout(920);
}

#[test]
#[ignore = "requires an isolated GTK display"]
fn the_result_header_wraps_without_overlap_at_600_px() {
    assert_header_layout(600);
}

#[test]
#[ignore = "requires an isolated GTK display"]
fn editing_the_text_during_a_filter_keeps_it_busy_and_cancellable() {
    let fixture = fixture();
    let entry: gtk::SearchEntry = named(&fixture.holder, "tp-result-find-entry");
    let filter: gtk::Button = named(&fixture.holder, "tp-result-filter-button");
    let stop: gtk::Button = named(&fixture.holder, "tp-result-find-stop");
    entry.set_text("needle");
    filter.emit_clicked();
    let Some(SqlEditorInput::FilterResults(request)) = fixture.receiver.recv_sync() else {
        panic!("a filter request was not sent")
    };
    entry.set_text("needle two");
    assert!(!filter.is_sensitive());
    assert!(stop.is_visible());
    filter.emit_clicked();
    stop.emit_clicked();
    assert!(request.ticket.cancel().is_cancelled());
}

#[test]
#[ignore = "requires an isolated GTK display"]
fn destroying_the_toolbar_keeps_a_running_filter_and_the_rebuilt_toolbar_resumes_it() {
    let fixture = fixture_for(identity_number(11));
    let entry: gtk::SearchEntry = named(&fixture.holder, "tp-result-find-entry");
    entry.set_text("needle");
    let filter: gtk::Button = named(&fixture.holder, "tp-result-filter-button");
    filter.emit_clicked();
    let Some(SqlEditorInput::FilterResults(request)) = fixture.receiver.recv_sync() else {
        panic!("a filter request was not sent")
    };
    let old = fixture
        .holder
        .first_child()
        .and_then(|widget| widget.downcast::<relm4::adw::WrapBox>().ok());
    fixture.holder.remove(old.as_ref().unwrap());
    drop(old);
    drop(filter);
    drop(entry);
    for _ in 0..20 {
        gtk::glib::MainContext::default().iteration(false);
    }
    assert!(!request.ticket.cancel().is_cancelled());
    mount(
        &fixture.holder,
        &fixture.pages,
        &fixture.result,
        identity_number(11),
        &fixture.sender,
    );
    let stop: gtk::Button = named(&fixture.holder, "tp-result-find-stop");
    let filter: gtk::Button = named(&fixture.holder, "tp-result-filter-button");
    assert!(stop.is_visible());
    assert!(!filter.is_sensitive());
    stop.emit_clicked();
    assert!(request.ticket.cancel().is_cancelled());
    request.ticket.finish("done".into());
}

#[test]
#[ignore = "requires an isolated GTK display"]
fn a_cell_filter_shows_stop_on_the_toolbar_of_its_own_result() {
    let fixture = fixture_for(identity_number(21));
    let first = fixture
        .holder
        .first_child()
        .and_then(|widget| widget.downcast::<relm4::adw::WrapBox>().ok())
        .unwrap();
    let second = mount(
        &fixture.holder,
        &fixture.pages,
        &fixture.result,
        identity_number(22),
        &fixture.sender,
    );
    let ticket = Arc::new(super::ticket::FilterTicket::default());
    super::ticket::register(identity_number(22), &ticket);
    super::widget::tools_for(identity_number(22))
        .unwrap()
        .adopt_filter(ticket.clone());
    let stop_in = |header: &relm4::adw::WrapBox| -> gtk::Button {
        super::jump::find_descendant(header.upcast_ref(), |widget| {
            widget.widget_name() == "tp-result-find-stop"
        })
        .unwrap()
    };
    assert!(stop_in(&second).is_visible());
    assert!(!stop_in(&first).is_visible());
    ticket.finish("done".into());
}

#[test]
#[ignore = "requires an isolated GTK display"]
fn a_search_finishing_does_not_clear_a_cell_filter_started_during_it() {
    let fixture = fixture();
    let stop: gtk::Button = named(&fixture.holder, "tp-result-find-stop");
    let filter: gtk::Button = named(&fixture.holder, "tp-result-filter-button");
    let status: gtk::Label = named(&fixture.holder, "tp-result-find-status");
    search(&fixture, "needle");
    let ticket = Arc::new(super::ticket::FilterTicket::default());
    super::ticket::register(StatementIdentity::default(), &ticket);
    super::widget::tools_for(StatementIdentity::default())
        .unwrap()
        .adopt_filter(ticket.clone());
    let context = gtk::glib::MainContext::default();
    for _ in 0..200 {
        while context.iteration(false) {}
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert!(stop.is_visible());
    assert!(!filter.is_sensitive());
    assert!(!status.label().contains("Match"), "{}", status.label());
    assert!(!ticket.cancel().is_cancelled());
    ticket.finish("done".into());
}
