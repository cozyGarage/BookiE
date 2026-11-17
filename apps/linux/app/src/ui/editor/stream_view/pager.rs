use relm4::gtk;
use relm4::gtk::prelude::*;

struct Controls {
    first: gtk::Button,
    previous: gtk::Button,
    range: gtk::Label,
    next: gtk::Button,
    last: gtk::Button,
}

pub(in crate::ui::editor) fn build(pages: &gtk::Stack) -> gtk::Box {
    let container = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let controls = Controls {
        first: icon_button("go-first-symbolic", &bookie_app_services::tr!("First page")),
        previous: gtk::Button::with_label(&bookie_app_services::tr!("Previous page")),
        range: gtk::Label::new(None),
        next: gtk::Button::with_label(&bookie_app_services::tr!("Next page")),
        last: icon_button("go-last-symbolic", &bookie_app_services::tr!("Last page")),
    };
    controls.range.add_css_class("numeric");
    controls
        .first
        .connect_clicked(go_to(pages, |pages| pages.first_child()));
    controls.previous.connect_clicked(go_to(pages, |pages| {
        pages.visible_child().and_then(|child| child.prev_sibling())
    }));
    controls.next.connect_clicked(go_to(pages, |pages| {
        pages.visible_child().and_then(|child| child.next_sibling())
    }));
    controls.last.connect_clicked(go_to(pages, |pages| pages.last_child()));
    update(pages, &controls);
    let controls = std::rc::Rc::new(controls);
    pages.connect_visible_child_notify({
        let controls = controls.clone();
        move |pages| update(pages, &controls)
    });
    container.append(&controls.first);
    container.append(&controls.previous);
    container.append(&controls.range);
    container.append(&controls.next);
    container.append(&controls.last);
    container
}

fn icon_button(icon: &str, label: &str) -> gtk::Button {
    let button = gtk::Button::from_icon_name(icon);
    button.set_tooltip_text(Some(label));
    button.update_property(&[gtk::accessible::Property::Label(label)]);
    button
}

fn go_to(
    pages: &gtk::Stack,
    target: impl Fn(&gtk::Stack) -> Option<gtk::Widget> + 'static,
) -> impl Fn(&gtk::Button) + 'static {
    let pages = pages.downgrade();
    move |_| {
        if let Some(pages) = pages.upgrade()
            && let Some(child) = target(&pages)
        {
            pages.set_visible_child(&child);
        }
    }
}

fn update(pages: &gtk::Stack, controls: &Controls) {
    let current = pages.visible_child();
    let has_previous = current.as_ref().and_then(|child| child.prev_sibling()).is_some();
    let has_next = current.as_ref().and_then(|child| child.next_sibling()).is_some();
    controls.first.set_sensitive(has_previous);
    controls.previous.set_sensitive(has_previous);
    controls.next.set_sensitive(has_next);
    controls.last.set_sensitive(has_next);
    let text = current.map(|child| pages.page(&child)).and_then(|page| page.title());
    controls.range.set_text(text.as_deref().unwrap_or_default());
}

#[cfg(test)]
mod tests;
