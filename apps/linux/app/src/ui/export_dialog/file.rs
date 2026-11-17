use adw::prelude::*;
use bookie_core::export::{CsvOptions, ExportError, ResultExport, ResultFormat, SqlTarget};
use relm4::{adw, gtk};
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};
use tokio_util::sync::CancellationToken;

pub(super) enum JobSource {
    Loaded(Arc<bookie_core::QueryResult>),
    Paged {
        mode: super::PagedExportMode,
        columns: Vec<bookie_core::ColumnInfo>,
        total: Option<usize>,
        open: super::PageFetcherFactory,
    },
}

pub(super) struct ExportJob {
    pub(super) path: std::path::PathBuf,
    pub(super) source: JobSource,
    pub(super) retained_page: Option<super::StreamExportPage>,
    pub(super) format: super::ExportFormat,
    pub(super) options: CsvOptions,
    pub(super) driver_id: String,
    pub(super) schema: Option<String>,
    pub(super) table: String,
    pub(super) row_limit: Option<usize>,
}

fn write_export(job: ExportJob, cancel: &CancellationToken, completed: &AtomicUsize) -> Result<bool, ExportError> {
    let _retained_page = job.retained_page;
    if matches!(
        &job.source,
        JobSource::Paged {
            mode: super::PagedExportMode::FullTable,
            ..
        }
    ) {
        return Err(ExportError::Source(snapshot_refusal_message()));
    }
    let export = ResultExport {
        format: result_format(job.format),
        csv: &job.options,
        sql: Some(SqlTarget {
            driver_id: &job.driver_id,
            schema: job.schema.as_deref(),
            table: &job.table,
        }),
    };
    let cancelled = || cancel.is_cancelled();
    let progress = |rows| completed.store(rows, Ordering::Relaxed);
    match job.source {
        JobSource::Loaded(result) => {
            let (result, capped) = limited_result(result, job.row_limit);
            bookie_core::export::write_result_file(&job.path, &result, &export, cancelled, progress)?;
            Ok(capped)
        }
        JobSource::Paged {
            mode: super::PagedExportMode::StoredQuery | super::PagedExportMode::SnapshotTable,
            columns,
            total,
            open,
        } => {
            let mut pages = open(cancel.clone());
            let mut finish = pages.finish.take();
            let capped = Arc::new(AtomicBool::new(false));
            let total = job
                .row_limit
                .map_or(total.unwrap_or(0), |limit| total.unwrap_or(limit).min(limit));
            let mut next = capped_pages(&mut pages, job.row_limit, capped.clone());
            let next_and_finish = || finish_on_end(&mut next, &mut finish);
            let result = bookie_core::export::write_paged_file(
                &job.path,
                &columns,
                total,
                &export,
                next_and_finish,
                cancelled,
                progress,
            );
            drop(next);
            pages.finish = finish;
            finish_pages(result, pages.finish())?;
            Ok(capped.load(Ordering::SeqCst))
        }
        JobSource::Paged {
            mode: super::PagedExportMode::FullTable,
            ..
        } => Err(ExportError::Source(snapshot_refusal_message())),
    }
}

fn capped_pages(
    pages: &mut super::PageFetcher,
    limit: Option<usize>,
    capped: Arc<AtomicBool>,
) -> Box<dyn FnMut() -> Result<Option<bookie_core::export::RowPage>, ExportError> + '_> {
    match limit {
        Some(limit) => Box::new(bookie_core::export::cap_pages(|| pages.next_page(), limit, capped)),
        None => Box::new(|| pages.next_page()),
    }
}

fn finish_on_end(
    next: &mut dyn FnMut() -> Result<Option<bookie_core::export::RowPage>, ExportError>,
    finish: &mut Option<super::FinishPages>,
) -> Result<Option<bookie_core::export::RowPage>, ExportError> {
    let page = next()?;
    if page.is_none() {
        finish.take().map_or(Ok(()), |finish| finish())?;
    }
    Ok(page)
}

fn limited_result(
    result: Arc<bookie_core::QueryResult>,
    limit: Option<usize>,
) -> (Arc<bookie_core::QueryResult>, bool) {
    match limit {
        Some(limit) if result.rows.len() > limit => {
            let (cut, capped) = bookie_core::export::limit_result(&result, limit);
            (Arc::new(cut), capped)
        }
        _ => (result, false),
    }
}

fn finish_pages(result: Result<(), ExportError>, finish: Result<(), ExportError>) -> Result<(), ExportError> {
    match (result, finish) {
        (Err(error), Err(cleanup)) => {
            crate::services::driver_diagnostics::warn_export_cleanup(&cleanup, "finish_after_error");
            Err(error)
        }
        (Err(error), Ok(())) | (Ok(()), Err(error)) => Err(error),
        (Ok(()), Ok(())) => Ok(()),
    }
}

pub(super) fn snapshot_refusal_message() -> String {
    bookie_app_services::tr!(
        "Full-table export requires a consistent snapshot. Export the current page or a query result instead."
    )
}

fn result_format(format: super::ExportFormat) -> ResultFormat {
    match format {
        super::ExportFormat::Csv => ResultFormat::Csv,
        super::ExportFormat::Json => ResultFormat::Json,
        super::ExportFormat::JsonLines => ResultFormat::JsonLines,
        super::ExportFormat::Markdown => ResultFormat::Markdown,
        super::ExportFormat::Html => ResultFormat::Html,
        super::ExportFormat::Xml => ResultFormat::Xml,
        super::ExportFormat::Sql => ResultFormat::Sql,
        super::ExportFormat::Xlsx => ResultFormat::Xlsx,
    }
}

fn build_progress_dialog(cancel: &CancellationToken, paged: bool) -> (adw::Dialog, gtk::Label, gtk::ProgressBar) {
    let label = gtk::Label::new(Some(&if paged {
        bookie_app_services::tr!("Exporting all rows…")
    } else {
        bookie_app_services::tr!("Exporting loaded rows…")
    }));
    let progress = gtk::ProgressBar::new();
    let stop = gtk::Button::with_label(&bookie_app_services::tr!("Cancel"));
    let content = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(12)
        .margin_top(24)
        .margin_bottom(24)
        .margin_start(24)
        .margin_end(24)
        .build();
    content.append(&label);
    content.append(&progress);
    content.append(&stop);
    let dialog = adw::Dialog::builder()
        .title(bookie_app_services::tr!("Export Results"))
        .child(&content)
        .content_width(400)
        .build();
    stop.connect_clicked({
        let cancel = cancel.clone();
        move |button| {
            cancel.cancel();
            button.set_sensitive(false);
        }
    });
    dialog.connect_closed({
        let cancel = cancel.clone();
        move |_| cancel.cancel()
    });
    (dialog, label, progress)
}

fn start_progress_timer(
    progress: gtk::ProgressBar,
    label: gtk::Label,
    completed: Arc<AtomicUsize>,
    total: Option<usize>,
) -> glib::SourceId {
    glib::timeout_add_local(std::time::Duration::from_millis(100), move || {
        let done = completed.load(Ordering::Relaxed);
        match total {
            Some(total) => {
                progress.set_fraction(if total == 0 { 0.0 } else { done as f64 / total as f64 });
                label.set_label(
                    &bookie_app_services::tr!("{done} / {total} rows")
                        .replace("{done}", &done.to_string())
                        .replace("{total}", &total.to_string()),
                );
            }
            None => {
                progress.pulse();
                label.set_label(&bookie_app_services::tr!("{done} rows written").replace("{done}", &done.to_string()));
            }
        }
        glib::ControlFlow::Continue
    })
}

fn handle_export_outcome(
    parent: &glib::WeakRef<adw::ApplicationWindow>,
    toast: &glib::WeakRef<adw::ToastOverlay>,
    path: &std::path::Path,
    final_count: usize,
    outcome: Result<Result<bool, ExportError>, async_channel::RecvError>,
) {
    match outcome {
        Ok(Ok(capped)) => {
            if let Some(toast) = toast.upgrade() {
                let message = if capped {
                    bookie_app_services::tr!(
                        "Exported the first {n} rows to {path} (row limit reached, more rows exist)"
                    )
                } else {
                    bookie_app_services::tr!("Exported {n} rows to {path}")
                };
                toast.add_toast(crate::ui::plain_text::toast(
                    &message
                        .replace("{n}", &final_count.to_string())
                        .replace("{path}", &path.display().to_string()),
                ));
            }
        }
        Ok(Err(error)) if error.is_cancelled() => {}
        Ok(Err(error)) => {
            if let Some(parent) = parent.upgrade() {
                super::show_export_error(&parent, path, &error);
            }
        }
        Err(_) => {
            if let Some(parent) = parent.upgrade() {
                let stopped = ExportError::Write(std::io::Error::other("Export worker stopped"));
                super::show_export_error(&parent, path, &stopped);
            }
        }
    }
}

pub(super) fn start(parent: &adw::ApplicationWindow, toast: &adw::ToastOverlay, job: ExportJob) {
    let cancel = CancellationToken::new();
    let completed = Arc::new(AtomicUsize::new(0));
    let (total, paged) = match &job.source {
        JobSource::Loaded(result) => (Some(result.rows.len()), false),
        JobSource::Paged { total, .. } => (*total, true),
    };
    let path = job.path.clone();
    let (dialog, label, progress) = build_progress_dialog(&cancel, paged);
    dialog.present(Some(parent));

    let busy = bookie_app_services::busy_jobs::global()
        .begin_owned(crate::ui::progress_dialog::owner_id(parent), cancel.clone());
    let (send, recv) = async_channel::bounded(1);
    let count = completed.clone();
    let final_count = completed.clone();
    let worker = std::thread::Builder::new().name("result-export".into()).spawn(move || {
        let _ = send.send_blocking(write_export(job, &cancel, &count));
    });
    if let Err(error) = worker {
        dialog.close();
        super::show_export_error(parent, &path, &ExportError::Write(error));
        return;
    }
    let timer = start_progress_timer(progress, label, completed, total);
    let parent = parent.downgrade();
    let toast = toast.downgrade();
    glib::spawn_future_local(async move {
        let outcome = recv.recv().await;
        drop(busy);
        timer.remove();
        dialog.close();
        handle_export_outcome(&parent, &toast, &path, final_count.load(Ordering::Relaxed), outcome);
    });
}

#[cfg(test)]
#[path = "file_tests.rs"]
mod tests;
