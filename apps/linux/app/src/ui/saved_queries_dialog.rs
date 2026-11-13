use relm4::adw::prelude::*;
use relm4::prelude::*;
use relm4::{adw, gtk};

use bookie_storage::{SavedQuery, SavedQueryFolder, SavedQueryLibrary};
use uuid::Uuid;

pub struct SavedQueriesDialog {
    root: adw::Dialog,
    search: gtk::SearchEntry,
    folder_filter: gtk::DropDown,
    listbox: gtk::ListBox,
    stack: gtk::Stack,
    status_page: adw::StatusPage,
    library: SavedQueryLibrary,
    selected_folder: FolderSelection,
    load_generation: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FolderSelection {
    All,
    Unfiled,
    Folder(Uuid),
}

#[derive(Debug, Clone)]
pub enum SavedQueriesInput {
    SearchChanged,
    FolderChanged,
    CreateFolder,
    CreateFolderNamed(String),
    RenameFolder,
    RenameFolderNamed(Uuid, String),
    DeleteFolder,
    DeleteFolderConfirmed(Uuid),
    Organize(Uuid),
    SaveOrganization(Uuid, Option<Uuid>, Vec<String>),
    Open(Uuid),
    AskRename(Uuid),
    Rename(Uuid, String),
    AskDelete(Uuid),
    Delete(Uuid),
}

#[derive(Debug)]
pub enum SavedQueriesOutput {
    OpenInNewTab(String),
    Changed(Vec<SavedQuery>),
}

#[derive(Debug)]
pub enum SavedQueriesCmd {
    Loaded(u64, SavedQueryLibrary),
    Mutated(Result<(), String>),
    Failed(u64, String),
}

pub fn visible_saved_queries(saved: &[SavedQuery], needle: &str) -> Vec<SavedQuery> {
    bookie_storage::rank_favorites(saved)
        .into_iter()
        .filter(|candidate| bookie_storage::matches_filter(candidate, needle))
        .collect()
}

fn visible_library_queries(library: &SavedQueryLibrary, needle: &str, selection: FolderSelection) -> Vec<SavedQuery> {
    visible_saved_queries(&library.favorites, needle)
        .into_iter()
        .filter(|query| match selection {
            FolderSelection::All => true,
            FolderSelection::Unfiled => query.folder_id.is_none(),
            FolderSelection::Folder(id) => query.folder_id == Some(id),
        })
        .collect()
}

pub fn statement_preview(sql: &str) -> String {
    let line = sql
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or_default();
    if line.chars().count() <= 80 {
        return line.to_string();
    }
    let head: String = line.chars().take(79).collect();
    format!("{head}…")
}

pub fn cleaned_name(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    Some(trimmed.to_string())
}

pub fn cleaned_keywords(raw: &str) -> Vec<String> {
    let mut keywords = Vec::new();
    for item in raw.split(',').map(str::trim).filter(|item| !item.is_empty()) {
        if !keywords
            .iter()
            .any(|keyword: &String| keyword.eq_ignore_ascii_case(item))
        {
            keywords.push(item.to_string());
        }
    }
    keywords
}

impl SavedQueriesDialog {
    pub fn dialog(&self) -> &adw::Dialog {
        &self.root
    }

    fn renamed(&self, id: Uuid, name: String) -> Option<SavedQuery> {
        let existing = self.library.favorites.iter().find(|candidate| candidate.id == id)?;
        Some(SavedQuery {
            name,
            ..existing.clone()
        })
    }

    fn sql_of(&self, id: Uuid) -> Option<String> {
        self.library
            .favorites
            .iter()
            .find(|candidate| candidate.id == id)
            .map(|found| found.sql.clone())
    }

    fn ask_rename(&self, id: Uuid, sender: &ComponentSender<Self>) {
        let Some(existing) = self.library.favorites.iter().find(|candidate| candidate.id == id) else {
            return;
        };
        ask_text(
            &self.root,
            &bookie_app_services::tr!("Rename saved query"),
            &bookie_app_services::tr!("Name"),
            &existing.name,
            &bookie_app_services::tr!("Rename"),
            move |name| SavedQueriesInput::Rename(id, name),
            sender,
        );
    }

    fn ask_delete(&self, id: Uuid, sender: &ComponentSender<Self>) {
        let Some(existing) = self.library.favorites.iter().find(|candidate| candidate.id == id) else {
            return;
        };
        let body = bookie_app_services::tr!("\"{name}\" will be removed from your saved queries.")
            .replace("{name}", &existing.name);
        confirm(
            &self.root,
            &bookie_app_services::tr!("Delete saved query?"),
            &body,
            &bookie_app_services::tr!("Delete"),
            move || SavedQueriesInput::Delete(id),
            sender,
        );
    }

    fn ask_folder_name(&self, id: Option<Uuid>, sender: &ComponentSender<Self>) {
        let existing = id.and_then(|id| self.library.folders.iter().find(|folder| folder.id == id));
        let title = if existing.is_some() {
            bookie_app_services::tr!("Rename folder")
        } else {
            bookie_app_services::tr!("Create folder")
        };
        ask_text(
            &self.root,
            &title,
            &bookie_app_services::tr!("Folder name"),
            existing.map_or("", |folder| folder.name.as_str()),
            &title,
            move |name| match id {
                Some(id) => SavedQueriesInput::RenameFolderNamed(id, name),
                None => SavedQueriesInput::CreateFolderNamed(name),
            },
            sender,
        );
    }

    fn ask_delete_folder(&self, sender: &ComponentSender<Self>) {
        let FolderSelection::Folder(id) = self.selected_folder else {
            return;
        };
        let Some(folder) = self.library.folders.iter().find(|folder| folder.id == id) else {
            return;
        };
        let body = bookie_app_services::tr!("Queries in \"{name}\" will be kept without a folder.")
            .replace("{name}", &folder.name);
        confirm(
            &self.root,
            &bookie_app_services::tr!("Delete folder?"),
            &body,
            &bookie_app_services::tr!("Delete"),
            move || SavedQueriesInput::DeleteFolderConfirmed(id),
            sender,
        );
    }

    fn ask_organize(&self, id: Uuid, sender: &ComponentSender<Self>) {
        let Some(query) = self.library.favorites.iter().find(|query| query.id == id) else {
            return;
        };
        let dialog = adw::AlertDialog::new(Some(&bookie_app_services::tr!("Organize saved query")), None);
        let labels = std::iter::once(bookie_app_services::tr!("No folder"))
            .chain(self.library.folders.iter().map(|item| item.name.clone()))
            .collect::<Vec<_>>();
        let label_refs = labels.iter().map(String::as_str).collect::<Vec<_>>();
        let folder = adw::ComboRow::builder()
            .use_markup(false)
            .title(bookie_app_services::tr!("Folder"))
            .model(&gtk::StringList::new(&label_refs))
            .build();
        let folder_ids = self.library.folders.iter().map(|folder| folder.id).collect::<Vec<_>>();
        folder.set_selected(
            query
                .folder_id
                .and_then(|id| self.library.folders.iter().position(|item| item.id == id))
                .map_or(0, |index| index as u32 + 1),
        );
        let keywords = adw::EntryRow::builder()
            .title(bookie_app_services::tr!("Keywords, separated by commas"))
            .use_markup(false)
            .build();
        keywords.set_text(&query.keywords.join(", "));
        let group = adw::PreferencesGroup::new();
        group.add(&folder);
        group.add(&keywords);
        dialog.set_extra_child(Some(&group));
        dialog.add_response("cancel", &bookie_app_services::tr!("Cancel"));
        dialog.add_response("save", &bookie_app_services::tr!("Save"));
        dialog.set_response_appearance("save", adw::ResponseAppearance::Suggested);
        dialog.set_close_response("cancel");
        let sender = sender.clone();
        dialog.connect_response(None, move |_, response| {
            if response != "save" {
                return;
            }
            let folder_id = folder
                .selected()
                .checked_sub(1)
                .and_then(|index| folder_ids.get(index as usize).copied());
            sender.input(SavedQueriesInput::SaveOrganization(
                id,
                folder_id,
                cleaned_keywords(&keywords.text()),
            ));
        });
        dialog.present(Some(&self.root));
    }

    fn render(&self, sender: &ComponentSender<Self>) {
        while let Some(child) = self.listbox.first_child() {
            self.listbox.remove(&child);
        }
        let needle = self.search.text().to_string();
        let visible = visible_library_queries(&self.library, &needle, self.selected_folder);
        if visible.is_empty() {
            self.show_empty(!needle.trim().is_empty());
            return;
        }
        self.stack.set_visible_child_name("list");
        for saved in visible {
            self.listbox.append(&build_row(&saved, sender));
        }
    }

    fn show_empty(&self, searching: bool) {
        self.stack.set_visible_child_name("empty");
        if searching {
            self.status_page.set_title(&bookie_app_services::tr!("No matches"));
            self.status_page
                .set_description(Some(&bookie_app_services::tr!("Try a different search term.")));
            return;
        }
        self.status_page
            .set_title(&bookie_app_services::tr!("No saved queries here"));
        self.status_page.set_description(Some(&bookie_app_services::tr!(
            "Press Ctrl+D in the SQL editor to save the query you are writing."
        )));
    }

    fn reload(&mut self, sender: &ComponentSender<Self>) {
        self.load_generation = self.load_generation.wrapping_add(1);
        let generation = self.load_generation;
        sender.oneshot_command(async move {
            match bookie_storage::load_saved_query_library().await {
                Ok(library) => SavedQueriesCmd::Loaded(generation, library),
                Err(error) => SavedQueriesCmd::Failed(generation, error.to_string()),
            }
        });
    }

    fn mutate(&self, sender: &ComponentSender<Self>, operation: MutationFuture) {
        sender.oneshot_command(async move { SavedQueriesCmd::Mutated(operation.await) });
    }

    fn fill_folders(&mut self) {
        let labels = std::iter::once(bookie_app_services::tr!("All queries"))
            .chain(std::iter::once(bookie_app_services::tr!("Unfiled")))
            .chain(self.library.folders.iter().map(|folder| folder.name.clone()))
            .collect::<Vec<_>>();
        let label_refs = labels.iter().map(String::as_str).collect::<Vec<_>>();
        self.folder_filter.set_model(Some(&gtk::StringList::new(&label_refs)));
        let selected = match self.selected_folder {
            FolderSelection::All => Some(0),
            FolderSelection::Unfiled => Some(1),
            FolderSelection::Folder(id) => self
                .library
                .folders
                .iter()
                .position(|folder| folder.id == id)
                .map(|i| i + 2),
        };
        let Some(selected) = selected else {
            self.selected_folder = FolderSelection::All;
            self.folder_filter.set_selected(0);
            return;
        };
        self.folder_filter.set_selected(selected as u32);
    }

    fn select_folder(&mut self, sender: &ComponentSender<Self>) {
        self.selected_folder = match self.folder_filter.selected() {
            0 => FolderSelection::All,
            1 => FolderSelection::Unfiled,
            index => self
                .library
                .folders
                .get(index as usize - 2)
                .map_or(FolderSelection::All, |folder| FolderSelection::Folder(folder.id)),
        };
        self.render(sender);
    }

    fn update_folder(&mut self, message: SavedQueriesInput, sender: &ComponentSender<Self>) {
        match message {
            SavedQueriesInput::CreateFolder => self.ask_folder_name(None, sender),
            SavedQueriesInput::CreateFolderNamed(name) => {
                let Some(name) = cleaned_name(&name) else {
                    self.show_mutation_error();
                    return;
                };
                self.mutate(
                    sender,
                    Box::pin(async move {
                        bookie_storage::save_saved_query_folder(SavedQueryFolder::new(name))
                            .await
                            .map(|_| ())
                            .map_err(|error| error.to_string())
                    }),
                );
            }
            SavedQueriesInput::RenameFolder => {
                if let FolderSelection::Folder(id) = self.selected_folder {
                    self.ask_folder_name(Some(id), sender);
                }
            }
            SavedQueriesInput::RenameFolderNamed(id, name) => self.rename_folder(id, &name, sender),
            SavedQueriesInput::DeleteFolder => self.ask_delete_folder(sender),
            SavedQueriesInput::DeleteFolderConfirmed(id) => self.mutate(
                sender,
                Box::pin(async move {
                    bookie_storage::delete_saved_query_folder(id)
                        .await
                        .map(|_| ())
                        .map_err(|error| error.to_string())
                }),
            ),
            _ => {}
        }
    }

    fn rename_folder(&self, id: Uuid, name: &str, sender: &ComponentSender<Self>) {
        let Some(name) = cleaned_name(name) else {
            self.show_mutation_error();
            return;
        };
        let Some(mut folder) = self.library.folders.iter().find(|folder| folder.id == id).cloned() else {
            return;
        };
        folder.name = name;
        self.mutate(
            sender,
            Box::pin(async move {
                bookie_storage::save_saved_query_folder(folder)
                    .await
                    .map(|_| ())
                    .map_err(|error| error.to_string())
            }),
        );
    }

    fn update_query(&mut self, message: SavedQueriesInput, sender: &ComponentSender<Self>) {
        match message {
            SavedQueriesInput::Organize(id) => self.ask_organize(id, sender),
            SavedQueriesInput::SaveOrganization(id, folder_id, keywords) => self.mutate(
                sender,
                Box::pin(async move {
                    bookie_storage::organize_saved_query(id, folder_id, keywords)
                        .await
                        .map(|_| ())
                        .map_err(|error| error.to_string())
                }),
            ),
            SavedQueriesInput::AskRename(id) => self.ask_rename(id, sender),
            SavedQueriesInput::Rename(id, name) => self.persist_rename(id, &name, sender),
            SavedQueriesInput::AskDelete(id) => self.ask_delete(id, sender),
            SavedQueriesInput::Delete(id) => self.mutate(
                sender,
                Box::pin(async move {
                    bookie_storage::delete_favorite(id)
                        .await
                        .map(|_| ())
                        .map_err(|error| error.to_string())
                }),
            ),
            _ => {}
        }
    }

    fn persist_rename(&self, id: Uuid, name: &str, sender: &ComponentSender<Self>) {
        let Some(name) = cleaned_name(name) else {
            self.show_mutation_error();
            return;
        };
        let Some(renamed) = self.renamed(id, name) else { return };
        self.mutate(
            sender,
            Box::pin(async move {
                bookie_storage::save_favorite(renamed)
                    .await
                    .map(|_| ())
                    .map_err(|error| error.to_string())
            }),
        );
    }

    fn show_mutation_error(&self) {
        let dialog = adw::AlertDialog::new(
            Some(&bookie_app_services::tr!("Saved queries could not be saved")),
            Some(&bookie_app_services::tr!(
                "The change was not written. Correct any invalid input and try again."
            )),
        );
        dialog.add_response("close", &bookie_app_services::tr!("Close"));
        dialog.set_close_response("close");
        dialog.present(Some(&self.root));
    }
}

type MutationFuture = std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), String>> + Send>>;

struct DialogWidgets {
    search: gtk::SearchEntry,
    folder_filter: gtk::DropDown,
    listbox: gtk::ListBox,
    stack: gtk::Stack,
    status_page: adw::StatusPage,
}

fn build_dialog_widgets(root: &adw::Dialog, sender: &ComponentSender<SavedQueriesDialog>) -> DialogWidgets {
    let toolbar = adw::ToolbarView::new();
    let header = adw::HeaderBar::builder().show_end_title_buttons(true).build();
    toolbar.add_top_bar(&header);
    let search = build_search(sender);
    let (folder_filter, folder_actions) = build_folder_controls(sender);
    let (listbox, stack, status_page) = build_query_list();
    let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
    content.append(&search);
    content.append(&folder_filter);
    content.append(&folder_actions);
    content.append(&stack);
    toolbar.set_content(Some(&content));
    root.set_child(Some(&toolbar));
    DialogWidgets {
        search,
        folder_filter,
        listbox,
        stack,
        status_page,
    }
}

fn build_search(sender: &ComponentSender<SavedQueriesDialog>) -> gtk::SearchEntry {
    let search = gtk::SearchEntry::builder()
        .placeholder_text(bookie_app_services::tr!("Search saved queries"))
        .margin_top(12)
        .margin_bottom(6)
        .margin_start(12)
        .margin_end(12)
        .build();
    search.update_property(&[gtk::accessible::Property::Label(&bookie_app_services::tr!(
        "Search saved queries"
    ))]);
    let search_sender = sender.clone();
    search.connect_search_changed(move |_| search_sender.input(SavedQueriesInput::SearchChanged));
    search
}

fn build_folder_controls(sender: &ComponentSender<SavedQueriesDialog>) -> (gtk::DropDown, gtk::Box) {
    let folder_filter = gtk::DropDown::from_strings(&[
        &bookie_app_services::tr!("All queries"),
        &bookie_app_services::tr!("Unfiled"),
    ]);
    folder_filter.update_property(&[gtk::accessible::Property::Label(&bookie_app_services::tr!(
        "Filter by folder"
    ))]);
    folder_filter.set_margin_start(12);
    folder_filter.set_margin_end(12);
    folder_filter.set_margin_bottom(6);
    let folder_sender = sender.clone();
    folder_filter.connect_selected_notify(move |_| folder_sender.input(SavedQueriesInput::FolderChanged));
    let folder_actions = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(6)
        .margin_start(12)
        .margin_end(12)
        .build();
    append_action(
        &folder_actions,
        &bookie_app_services::tr!("New folder"),
        SavedQueriesInput::CreateFolder,
        sender,
    );
    append_action(
        &folder_actions,
        &bookie_app_services::tr!("Rename folder"),
        SavedQueriesInput::RenameFolder,
        sender,
    );
    append_action(
        &folder_actions,
        &bookie_app_services::tr!("Delete folder"),
        SavedQueriesInput::DeleteFolder,
        sender,
    );
    (folder_filter, folder_actions)
}

fn build_query_list() -> (gtk::ListBox, gtk::Stack, adw::StatusPage) {
    let listbox = gtk::ListBox::builder()
        .selection_mode(gtk::SelectionMode::None)
        .css_classes(["boxed-list"])
        .build();
    let group = adw::PreferencesGroup::builder()
        .margin_top(6)
        .margin_bottom(12)
        .margin_start(12)
        .margin_end(12)
        .build();
    group.add(&listbox);
    let status_page = adw::StatusPage::builder().icon_name("starred-symbolic").build();
    let stack = gtk::Stack::new();
    let scrolled = gtk::ScrolledWindow::builder().vexpand(true).child(&group).build();
    stack.add_named(&scrolled, Some("list"));
    stack.add_named(&status_page, Some("empty"));
    (listbox, stack, status_page)
}

fn build_row(saved: &SavedQuery, sender: &ComponentSender<SavedQueriesDialog>) -> adw::ActionRow {
    let subtitle = if saved.keywords.is_empty() {
        statement_preview(&saved.sql)
    } else {
        format!("{} · {}", statement_preview(&saved.sql), saved.keywords.join(", "))
    };
    let row = adw::ActionRow::builder()
        .title(glib_escaped(&saved.name))
        .subtitle(glib_escaped(&subtitle))
        .activatable(true)
        .build();
    row.add_suffix(&row_button(
        "folder-symbolic",
        &bookie_app_services::tr!("Organize"),
        SavedQueriesInput::Organize(saved.id),
        sender,
    ));
    row.add_suffix(&row_button(
        "document-edit-symbolic",
        &bookie_app_services::tr!("Rename"),
        SavedQueriesInput::AskRename(saved.id),
        sender,
    ));
    let delete = row_button(
        "user-trash-symbolic",
        &bookie_app_services::tr!("Delete"),
        SavedQueriesInput::AskDelete(saved.id),
        sender,
    );
    delete.add_css_class("destructive-action");
    row.add_suffix(&delete);
    let activate_sender = sender.clone();
    let id = saved.id;
    row.connect_activated(move |_| activate_sender.input(SavedQueriesInput::Open(id)));
    row
}

fn ask_text(
    root: &adw::Dialog,
    title: &str,
    field_title: &str,
    initial: &str,
    response_title: &str,
    message: impl Fn(String) -> SavedQueriesInput + 'static,
    sender: &ComponentSender<SavedQueriesDialog>,
) {
    let dialog = adw::AlertDialog::new(Some(title), None);
    let entry = adw::EntryRow::builder().use_markup(false).title(field_title).build();
    entry.set_text(initial);
    let list = gtk::ListBox::builder()
        .selection_mode(gtk::SelectionMode::None)
        .css_classes(["boxed-list"])
        .build();
    list.append(&entry);
    dialog.set_extra_child(Some(&list));
    dialog.add_response("cancel", &bookie_app_services::tr!("Cancel"));
    dialog.add_response("save", response_title);
    dialog.set_response_appearance("save", adw::ResponseAppearance::Suggested);
    dialog.set_default_response(Some("save"));
    dialog.set_close_response("cancel");
    let sender = sender.clone();
    dialog.connect_response(None, move |_, response| {
        if response == "save" {
            sender.input(message(entry.text().to_string()));
        }
    });
    dialog.present(Some(root));
}

fn confirm(
    root: &adw::Dialog,
    title: &str,
    body: &str,
    response_title: &str,
    message: impl FnOnce() -> SavedQueriesInput + 'static,
    sender: &ComponentSender<SavedQueriesDialog>,
) {
    let dialog = adw::AlertDialog::new(Some(title), Some(body));
    dialog.add_response("cancel", &bookie_app_services::tr!("Cancel"));
    dialog.add_response("confirm", response_title);
    dialog.set_response_appearance("confirm", adw::ResponseAppearance::Destructive);
    dialog.set_close_response("cancel");
    let sender = sender.clone();
    let message = std::cell::RefCell::new(Some(message));
    dialog.connect_response(None, move |_, response| {
        if response == "confirm"
            && let Some(message) = message.borrow_mut().take()
        {
            sender.input(message());
        }
    });
    dialog.present(Some(root));
}

fn row_button(
    icon: &str,
    tooltip: &str,
    message: SavedQueriesInput,
    sender: &ComponentSender<SavedQueriesDialog>,
) -> gtk::Button {
    let button = gtk::Button::builder()
        .icon_name(icon)
        .tooltip_text(tooltip)
        .valign(gtk::Align::Center)
        .build();
    button.add_css_class("flat");
    let sender = sender.clone();
    let message = std::cell::RefCell::new(Some(message));
    button.connect_clicked(move |_| {
        if let Some(message) = message.borrow_mut().take() {
            sender.input(message);
        }
    });
    button
}

fn glib_escaped(text: &str) -> String {
    gtk::glib::markup_escape_text(text).to_string()
}

impl Component for SavedQueriesDialog {
    type Init = Vec<SavedQuery>;
    type Input = SavedQueriesInput;
    type Output = SavedQueriesOutput;
    type CommandOutput = SavedQueriesCmd;
    type Root = adw::Dialog;
    type Widgets = ();

    fn init_root() -> Self::Root {
        adw::Dialog::builder()
            .title(bookie_app_services::tr!("Saved Queries"))
            .content_width(620)
            .content_height(600)
            .build()
    }

    fn init(init: Self::Init, root: Self::Root, sender: ComponentSender<Self>) -> ComponentParts<Self> {
        let widgets = build_dialog_widgets(&root, &sender);
        let mut model = SavedQueriesDialog {
            root,
            search: widgets.search,
            folder_filter: widgets.folder_filter,
            listbox: widgets.listbox,
            stack: widgets.stack,
            status_page: widgets.status_page,
            library: SavedQueryLibrary {
                favorites: init,
                folders: Vec::new(),
            },
            selected_folder: FolderSelection::All,
            load_generation: 0,
        };
        model.fill_folders();
        model.render(&sender);
        model.reload(&sender);
        ComponentParts { model, widgets: () }
    }

    fn update(&mut self, message: Self::Input, sender: ComponentSender<Self>, _root: &Self::Root) {
        match message {
            SavedQueriesInput::SearchChanged => self.render(&sender),
            SavedQueriesInput::FolderChanged => self.select_folder(&sender),
            message @ (SavedQueriesInput::CreateFolder
            | SavedQueriesInput::CreateFolderNamed(_)
            | SavedQueriesInput::RenameFolder
            | SavedQueriesInput::RenameFolderNamed(_, _)
            | SavedQueriesInput::DeleteFolder
            | SavedQueriesInput::DeleteFolderConfirmed(_)) => self.update_folder(message, &sender),
            message @ (SavedQueriesInput::Organize(_)
            | SavedQueriesInput::SaveOrganization(_, _, _)
            | SavedQueriesInput::AskRename(_)
            | SavedQueriesInput::Rename(_, _)
            | SavedQueriesInput::AskDelete(_)
            | SavedQueriesInput::Delete(_)) => self.update_query(message, &sender),
            SavedQueriesInput::Open(id) => {
                let Some(sql) = self.sql_of(id) else { return };
                sender.output(SavedQueriesOutput::OpenInNewTab(sql)).ok();
                self.root.close();
            }
        }
    }

    fn update_cmd(&mut self, message: Self::CommandOutput, sender: ComponentSender<Self>, _root: &Self::Root) {
        match message {
            SavedQueriesCmd::Loaded(generation, library) if generation == self.load_generation => {
                self.library = library;
                self.fill_folders();
                self.render(&sender);
                sender
                    .output(SavedQueriesOutput::Changed(self.library.favorites.clone()))
                    .ok();
            }
            SavedQueriesCmd::Loaded(_, _) => {}
            SavedQueriesCmd::Mutated(Ok(())) => self.reload(&sender),
            SavedQueriesCmd::Mutated(Err(error)) => {
                tracing::warn!(error = %error, "saved queries update failed");
                self.show_mutation_error();
            }
            SavedQueriesCmd::Failed(generation, error) if generation == self.load_generation => {
                tracing::warn!(error = %error, "saved queries load failed");
                self.status_page
                    .set_title(&bookie_app_services::tr!("Saved queries are unavailable"));
                self.stack.set_visible_child_name("empty");
            }
            SavedQueriesCmd::Failed(_, _) => {}
        }
    }
}

fn append_action(
    parent: &gtk::Box,
    label: &str,
    message: SavedQueriesInput,
    sender: &ComponentSender<SavedQueriesDialog>,
) {
    let button = gtk::Button::with_label(label);
    let sender = sender.clone();
    button.connect_clicked(move |_| sender.input(message.clone()));
    parent.append(&button);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn saved(name: &str, sql: &str) -> SavedQuery {
        SavedQuery::new(name.into(), sql.into(), None, None)
    }

    #[test]
    fn folder_selection_and_search_are_combined() {
        let folder = SavedQueryFolder::new("Reports".into());
        let mut filed = saved("daily", "SELECT 1");
        filed.folder_id = Some(folder.id);
        filed.keywords = vec!["finance".into()];
        let unfiled = saved("weekly", "SELECT 2");
        let library = SavedQueryLibrary {
            favorites: vec![filed.clone(), unfiled],
            folders: vec![folder.clone()],
        };

        let visible = visible_library_queries(&library, "finance", FolderSelection::Folder(folder.id));

        assert_eq!(visible, vec![filed]);
        assert!(visible_library_queries(&library, "weekly", FolderSelection::Folder(folder.id)).is_empty());
    }

    #[test]
    fn keywords_are_cleaned_case_insensitively() {
        assert_eq!(cleaned_keywords(" finance, Daily,FINANCE, ,"), ["finance", "Daily"]);
    }

    #[test]
    fn an_empty_search_lists_every_saved_query_in_rank_order() {
        let mut recent = saved("zeta", "SELECT 1");
        recent.last_used_at = Some(chrono::Utc::now());
        let older = saved("alpha", "SELECT 2");
        let visible = visible_saved_queries(&[older, recent], "  ");
        assert_eq!(visible.len(), 2);
        assert_eq!(visible[0].name, "zeta");
    }

    #[test]
    fn a_search_matches_the_name_or_the_statement() {
        let list = vec![saved("people", "SELECT 1"), saved("orders", "SELECT * FROM invoices")];
        assert_eq!(visible_saved_queries(&list, "PEOPLE").len(), 1);
        assert_eq!(visible_saved_queries(&list, "invoices")[0].name, "orders");
        assert!(visible_saved_queries(&list, "nothing here").is_empty());
    }

    #[test]
    fn the_preview_is_the_first_non_empty_line() {
        assert_eq!(statement_preview("\n\n  SELECT 1\nFROM t"), "SELECT 1");
        assert_eq!(statement_preview("   "), "");
    }

    #[test]
    fn a_long_preview_is_shortened_on_a_character_boundary() {
        let preview = statement_preview(&"é".repeat(200));
        assert_eq!(preview.chars().count(), 80);
        assert!(preview.ends_with('…'));
    }

    #[test]
    fn a_blank_rename_is_rejected() {
        assert_eq!(cleaned_name("  "), None);
        assert_eq!(cleaned_name("  daily report "), Some("daily report".to_string()));
    }
}
