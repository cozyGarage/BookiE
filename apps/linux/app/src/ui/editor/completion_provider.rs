mod model;

use std::cell::{OnceCell, RefCell};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use glib::subclass::prelude::*;
use relm4::gtk::{self, gio, glib};
use sourceview5::prelude::*;
use sourceview5::subclass::prelude::*;

use super::completion::{SchemaIndex, SchemaRequest};
use super::completion_proposals::{candidates_with, candidates_with_plan, completion_plan};

pub(super) fn dismiss(view: &sourceview5::View) {
    view.completion().hide();
}

pub(super) fn dismiss_on_focus_loss(view: &sourceview5::View) {
    let focus = gtk::EventControllerFocus::new();
    let weak_view = view.downgrade();
    focus.connect_leave(move |_| {
        if let Some(view) = weak_view.upgrade() {
            dismiss(&view);
        }
    });
    view.add_controller(focus);
}

struct Insertion {
    label: String,
    sql: String,
    source: Rc<str>,
    cursor: i32,
    range: std::ops::Range<usize>,
    request: SchemaRequest,
}

mod proposal {
    use super::*;

    #[derive(Default)]
    pub struct Proposal {
        pub(super) insertion: OnceCell<Insertion>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Proposal {
        const NAME: &str = "BookieSqlCompletionProposal";
        type Type = super::Proposal;
        type Interfaces = (sourceview5::CompletionProposal,);
    }

    impl ObjectImpl for Proposal {}
    impl CompletionProposalImpl for Proposal {}
}

glib::wrapper! {
    pub struct Proposal(ObjectSubclass<proposal::Proposal>) @implements sourceview5::CompletionProposal;
}

mod provider {
    use super::*;

    #[derive(Default)]
    pub struct Provider {
        pub index: RefCell<Rc<RefCell<SchemaIndex>>>,
        pub(crate) lexing: RefCell<super::super::session_lexing::KnownLexing>,
        pub latest_request: Arc<AtomicU64>,
        pub pending_refilter: RefCell<Option<glib::SourceId>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Provider {
        const NAME: &str = "BookieSqlCompletionProvider";
        type Type = super::Provider;
        type Interfaces = (sourceview5::CompletionProvider,);
    }

    impl ObjectImpl for Provider {}

    impl CompletionProviderImpl for Provider {
        fn title(&self) -> Option<glib::GString> {
            Some("SQL".into())
        }

        fn populate(&self, context: &sourceview5::CompletionContext) -> Result<gio::ListModel, glib::Error> {
            Ok(self.obj().model(context).upcast())
        }

        fn populate_future(
            &self,
            context: &sourceview5::CompletionContext,
        ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<gio::ListModel, glib::Error>>>> {
            let provider = self.obj().clone();
            let context = context.clone();
            let request = provider.begin_request();
            Box::pin(async move {
                let model = provider.model_async(&context, request).await;
                let model = if provider.is_current_request(request) {
                    model
                } else {
                    glib::Object::new()
                };
                Ok(model.upcast())
            })
        }

        fn refilter(&self, context: &sourceview5::CompletionContext, _model: &gio::ListModel) {
            let provider = self.obj().clone();
            let request = provider.begin_request();
            provider.cancel_pending_refilter();
            let Some(buffer) = context.buffer() else {
                context.set_proposals_for_provider(&provider, None::<&gio::ListModel>);
                return;
            };
            if buffer.char_count() as usize <= super::super::sql_text::MAX_ANALYSIS_BYTES {
                if let Some(text) = super::super::sql_text::analysis_text(buffer.upcast_ref()) {
                    context.set_proposals_for_provider(&provider, Some(&provider.model_from_text(context, text)));
                    return;
                }
            }
            let weak_provider = provider.downgrade();
            let context = context.clone();
            let source = glib::timeout_add_local_once(std::time::Duration::from_millis(75), move || {
                let Some(provider) = weak_provider.upgrade() else {
                    return;
                };
                provider.imp().pending_refilter.borrow_mut().take();
                glib::MainContext::default().spawn_local(async move {
                    let model = provider.model_async(&context, request).await;
                    if provider.is_current_request(request) {
                        context.set_proposals_for_provider(&provider, Some(&model));
                    }
                });
            });
            if provider.is_current_request(request) {
                provider.imp().pending_refilter.replace(Some(source));
            } else {
                source.remove();
            }
        }

        fn activate(&self, context: &sourceview5::CompletionContext, proposal: &sourceview5::CompletionProposal) {
            self.obj().insert(context, proposal);
        }

        fn display(
            &self,
            _context: &sourceview5::CompletionContext,
            proposal: &sourceview5::CompletionProposal,
            cell: &sourceview5::CompletionCell,
        ) {
            let Some(insertion) = proposal
                .downcast_ref::<super::Proposal>()
                .and_then(|proposal| proposal.imp().insertion.get())
            else {
                return;
            };
            match cell.column() {
                sourceview5::CompletionColumn::TypedText => cell.set_text(Some(&insertion.label)),
                sourceview5::CompletionColumn::Details => cell.set_text(Some(&insertion.sql)),
                _ => cell.set_text(None),
            }
        }

        fn is_trigger(&self, _iter: &gtk::TextIter, character: char) -> bool {
            character == '.'
        }
    }
}

glib::wrapper! {
    pub struct Provider(ObjectSubclass<provider::Provider>) @implements sourceview5::CompletionProvider;
}

pub(super) fn install(index: Rc<RefCell<SchemaIndex>>, lexing: super::session_lexing::KnownLexing) -> Provider {
    let provider: Provider = glib::Object::new();
    provider.imp().index.replace(index);
    provider.imp().lexing.replace(lexing);
    provider
}

impl Provider {
    fn begin_request(&self) -> u64 {
        self.imp()
            .latest_request
            .fetch_add(1, Ordering::Relaxed)
            .wrapping_add(1)
    }

    fn is_current_request(&self, request: u64) -> bool {
        self.imp().latest_request.load(Ordering::Acquire) == request
    }

    fn cancel_pending_refilter(&self) {
        if let Some(source) = self.imp().pending_refilter.borrow_mut().take() {
            source.remove();
        }
    }

    async fn model_async(&self, context: &sourceview5::CompletionContext, request_id: u64) -> model::ProposalModel {
        let Some(buffer) = context.buffer() else {
            return glib::Object::new();
        };
        let Some(text) = super::sql_text::completion_text(buffer.upcast_ref()) else {
            return glib::Object::new();
        };
        if text.len() <= super::sql_text::MAX_ANALYSIS_BYTES {
            return self.model_from_text(context, text);
        }
        let cursor_chars = buffer.iter_at_mark(&buffer.get_insert()).offset();
        let buffer_chars = buffer.char_count();
        let source = text.to_string();
        let cursor = super::statement_cursor::cursor_byte_offset(&source, cursor_chars as usize);
        let index_handle = self.imp().index.borrow().clone();
        let Ok(index) = index_handle.try_borrow() else {
            return glib::Object::new();
        };
        let grammar = index.grammar.unwrap_or(bookie_core::sql_syntax::SqlGrammar::PostgreSql);
        let settings = self.imp().lexing.borrow().settings();
        let schema_request = index.request(String::new());
        drop(index);
        let latest_request = self.imp().latest_request.clone();
        let plan = gio::spawn_blocking(move || {
            (latest_request.load(Ordering::Acquire) == request_id).then(|| {
                let plan = completion_plan(&source, grammar, settings);
                (source, plan)
            })
        })
        .await;
        let Ok(plan) = plan else {
            return glib::Object::new();
        };
        if !self.is_current_request(request_id) {
            return glib::Object::new();
        }
        let Some((source, plan)) = plan else {
            return glib::Object::new();
        };
        if buffer.char_count() != buffer_chars
            || buffer.iter_at_mark(&buffer.get_insert()).offset() != cursor_chars
            || self.imp().lexing.borrow().settings() != settings
        {
            return glib::Object::new();
        }
        let Ok(index) = index_handle.try_borrow() else {
            return glib::Object::new();
        };
        if !index.accepts(&schema_request)
            || index.grammar.unwrap_or(bookie_core::sql_syntax::SqlGrammar::PostgreSql) != grammar
        {
            return glib::Object::new();
        }
        let source: Rc<str> = source.into();
        let candidates = candidates_with_plan(&source, cursor, &index, settings, &plan);
        model::ProposalModel::new(candidates, source, cursor_chars, schema_request)
    }

    fn model(&self, context: &sourceview5::CompletionContext) -> model::ProposalModel {
        let Some(buffer) = context.buffer() else {
            return glib::Object::new();
        };
        let Some(text) = super::sql_text::analysis_text(buffer.upcast_ref()) else {
            return glib::Object::new();
        };
        self.model_from_text(context, text)
    }

    fn model_from_text(&self, context: &sourceview5::CompletionContext, text: glib::GString) -> model::ProposalModel {
        let Some(buffer) = context.buffer() else {
            return glib::Object::new();
        };
        let source: Rc<str> = text.as_str().into();
        let cursor = super::statement_cursor::cursor_byte_offset(
            &source,
            buffer.iter_at_mark(&buffer.get_insert()).offset() as usize,
        );
        let index = self.imp().index.borrow();
        let Ok(index) = index.try_borrow() else {
            return glib::Object::new();
        };
        let settings = self.imp().lexing.borrow().settings();
        let candidates = candidates_with(&source, cursor, &index, settings);
        model::ProposalModel::new(
            candidates,
            source,
            buffer.iter_at_mark(&buffer.get_insert()).offset(),
            index.request(String::new()),
        )
    }

    fn insert(&self, context: &sourceview5::CompletionContext, proposal: &sourceview5::CompletionProposal) {
        let Some(insertion) = proposal
            .downcast_ref::<Proposal>()
            .and_then(|proposal| proposal.imp().insertion.get())
        else {
            return;
        };
        let Some(buffer) = context.buffer() else {
            return;
        };
        let index = self.imp().index.borrow();
        let Ok(index) = index.try_borrow() else {
            return;
        };
        let Some(text) = super::sql_text::completion_text(buffer.upcast_ref()) else {
            return;
        };
        if !index.accepts(&insertion.request)
            || text.as_str() != insertion.source.as_ref()
            || buffer.iter_at_mark(&buffer.get_insert()).offset() != insertion.cursor
        {
            return;
        }
        drop(index);
        let start = insertion.source[..insertion.range.start].chars().count() as i32;
        let end = insertion.source[..insertion.range.end].chars().count() as i32;
        let mut start = buffer.iter_at_offset(start);
        let mut end = buffer.iter_at_offset(end);
        buffer.begin_user_action();
        buffer.delete(&mut start, &mut end);
        buffer.insert(&mut start, &insertion.sql);
        buffer.end_user_action();
    }
}

fn debounce_refresh(refresh: std::rc::Rc<dyn Fn()>) -> std::rc::Rc<dyn Fn()> {
    let pending: Rc<RefCell<Option<glib::SourceId>>> = Rc::default();
    Rc::new(move || {
        if let Some(source) = pending.borrow_mut().take() {
            source.remove();
        }
        let finished = Rc::downgrade(&pending);
        let refresh = refresh.clone();
        *pending.borrow_mut() = Some(glib::timeout_add_local_once(
            std::time::Duration::from_millis(250),
            move || {
                if let Some(pending) = finished.upgrade() {
                    pending.borrow_mut().take();
                    refresh();
                }
            },
        ));
    })
}

pub(super) fn build_completion_refresh(
    view: sourceview5::View,
    schema_index: std::rc::Rc<std::cell::RefCell<SchemaIndex>>,
    known_lexing: super::session_lexing::KnownLexing,
    sender: relm4::ComponentSender<super::SqlEditor>,
) -> std::rc::Rc<dyn Fn()> {
    let view = view.downgrade();
    debounce_refresh(std::rc::Rc::new(move || {
        let Some(view) = view.upgrade() else {
            return;
        };
        let buffer = view.buffer();
        let Some(sql) = super::sql_text::analysis_text(&buffer) else {
            return;
        };
        let cursor_chars = buffer.iter_at_mark(&buffer.get_insert()).offset() as usize;
        let cursor_byte = super::statement_cursor::cursor_byte_offset(&sql, cursor_chars);
        let Ok(index) = schema_index.try_borrow() else {
            return;
        };
        let missing: Vec<String> =
            super::completion::referenced_tables_with(&sql, cursor_byte, index.grammar, known_lexing.settings())
                .into_iter()
                .filter(|table| index.needs_columns(table))
                .collect();
        drop(index);
        if !missing.is_empty() {
            sender.input(super::SqlEditorInput::FetchCompletionColumns(missing));
        }
    }))
}

#[cfg(test)]
#[path = "completion_provider/tests.rs"]
mod tests;
