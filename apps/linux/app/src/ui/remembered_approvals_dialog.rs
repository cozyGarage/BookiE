use std::{cell::RefCell, rc::Rc};

use bookie_policy::ApprovalGrant;
use relm4::{adw, adw::prelude::*, gtk};

pub fn grant_title(grant: &ApprovalGrant) -> String {
    bookie_app_services::tr!("{rule} on {connection}")
        .replace("{rule}", &grant.rule)
        .replace("{connection}", &grant.connection_name)
}

pub fn present(parent: &adw::ApplicationWindow) {
    let page = adw::PreferencesPage::new();
    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&adw::HeaderBar::new());
    toolbar.set_content(Some(&page));
    let dialog = adw::Dialog::builder()
        .title(bookie_app_services::tr!("Remembered Approvals"))
        .content_width(480)
        .content_height(360)
        .child(&toolbar)
        .build();
    rebuild(&page, &Rc::new(RefCell::new(None)));
    dialog.present(Some(parent));
}

fn rebuild(page: &adw::PreferencesPage, current: &Rc<RefCell<Option<adw::PreferencesGroup>>>) {
    if let Some(old) = current.borrow_mut().take() {
        page.remove(&old);
    }
    let group = adw::PreferencesGroup::builder()
        .description(bookie_app_services::tr!(
            "Writes you chose not to be asked about again. They end when the connection closes or the app quits."
        ))
        .build();
    let grants = crate::services::approval_grants::shared().list();
    if grants.is_empty() {
        group.add(
            &adw::ActionRow::builder()
                .title(bookie_app_services::tr!("None"))
                .build(),
        );
    }
    for grant in grants {
        let row = adw::ActionRow::builder()
            .title(glib::markup_escape_text(&grant_title(&grant)))
            .subtitle(glib::markup_escape_text(&grant.principal.label()))
            .build();
        let revoke = gtk::Button::builder()
            .label(bookie_app_services::tr!("Revoke"))
            .valign(gtk::Align::Center)
            .build();
        let (page, current) = (page.clone(), current.clone());
        revoke.connect_clicked(move |_| {
            crate::services::approval_grants::shared().revoke(&grant);
            rebuild(&page, &current);
        });
        row.add_suffix(&revoke);
        group.add(&row);
    }
    page.add(&group);
    *current.borrow_mut() = Some(group);
}

#[cfg(test)]
mod tests {
    use bookie_policy::Principal;

    use super::*;

    #[test]
    fn a_grant_is_titled_by_its_rule_and_connection() {
        let grant = ApprovalGrant {
            connection_id: uuid::Uuid::nil(),
            connection_name: "orders".into(),
            rule: "linked_server_approval".into(),
            principal: Principal::human_gui(),
        };

        assert_eq!(grant_title(&grant), "linked_server_approval on orders");
    }
}
