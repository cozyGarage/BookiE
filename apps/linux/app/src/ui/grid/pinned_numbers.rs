use std::cell::{Cell, OnceCell, RefCell};

use gtk4::prelude::*;
use gtk4::subclass::prelude::*;
use gtk4::{self as gtk, glib};
use libadwaita as adw;
use libadwaita::prelude::BinExt;
use libadwaita::subclass::prelude::*;

use super::row_number_column;

mod imp {
    use super::*;

    #[derive(glib::Properties)]
    #[properties(wrapper_type = super::PinnedNumbers)]
    pub struct PinnedNumbers {
        #[property(override_interface = gtk::Scrollable, get, set = Self::set_hadjustment)]
        hadjustment: RefCell<Option<gtk::Adjustment>>,
        #[property(override_interface = gtk::Scrollable, get, set = Self::set_vadjustment)]
        vadjustment: RefCell<Option<gtk::Adjustment>>,
        #[property(override_interface = gtk::Scrollable, get, set = Self::set_hscroll_policy)]
        hscroll_policy: Cell<gtk::ScrollablePolicy>,
        #[property(override_interface = gtk::Scrollable, get, set = Self::set_vscroll_policy)]
        vscroll_policy: Cell<gtk::ScrollablePolicy>,
        pub view: OnceCell<gtk::ColumnView>,
        pub gutter_backing: OnceCell<gtk::Box>,
        pub gutter_header: OnceCell<gtk::Box>,
        pub hadjust_handler: RefCell<Option<glib::SignalHandlerId>>,
    }

    impl Default for PinnedNumbers {
        fn default() -> Self {
            Self {
                hadjustment: RefCell::default(),
                vadjustment: RefCell::default(),
                hscroll_policy: Cell::new(gtk::ScrollablePolicy::Minimum),
                vscroll_policy: Cell::new(gtk::ScrollablePolicy::Minimum),
                view: OnceCell::new(),
                gutter_backing: OnceCell::new(),
                gutter_header: OnceCell::new(),
                hadjust_handler: RefCell::default(),
            }
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for PinnedNumbers {
        const NAME: &'static str = "BookiePinnedNumbers";
        type Type = super::PinnedNumbers;
        type ParentType = adw::Bin;
        type Interfaces = (gtk::Scrollable,);
    }

    #[glib::derived_properties]
    impl ObjectImpl for PinnedNumbers {
        fn constructed(&self) {
            self.parent_constructed();
            self.obj().set_layout_manager(None::<gtk::LayoutManager>);
        }

        fn dispose(&self) {
            if let Some(adjustment) = self.hadjustment.borrow().as_ref()
                && let Some(handler) = self.hadjust_handler.borrow_mut().take()
            {
                adjustment.disconnect(handler);
            }
            self.parent_dispose();
        }
    }

    impl WidgetImpl for PinnedNumbers {
        fn measure(&self, orientation: gtk::Orientation, for_size: i32) -> (i32, i32, i32, i32) {
            self.obj()
                .child()
                .map_or((0, 0, -1, -1), |child| child.measure(orientation, for_size))
        }

        fn size_allocate(&self, width: i32, height: i32, baseline: i32) {
            let Some(child) = self.obj().child() else {
                return;
            };
            child.allocate(width, height, baseline, None);
            super::sync_header_state(&self.obj());
        }

        fn snapshot(&self, snapshot: &gtk::Snapshot) {
            self.parent_snapshot(snapshot);
            super::snapshot_numbers(&self.obj(), snapshot);
        }
    }

    impl BinImpl for PinnedNumbers {}

    impl ScrollableImpl for PinnedNumbers {
        fn border(&self) -> Option<gtk::Border> {
            self.view.get().and_then(gtk::prelude::ScrollableExt::border)
        }
    }

    impl PinnedNumbers {
        fn set_hadjustment(&self, adjustment: Option<gtk::Adjustment>) {
            if let Some(previous) = self.hadjustment.borrow().as_ref()
                && let Some(handler) = self.hadjust_handler.borrow_mut().take()
            {
                previous.disconnect(handler);
            }
            self.hadjustment.replace(adjustment.clone());
            if let Some(view) = self.view.get() {
                view.set_hadjustment(adjustment.as_ref());
            }
            if let Some(adjustment) = adjustment {
                let pinned = self.obj().downgrade();
                let handler = adjustment.connect_value_changed(move |_| {
                    if let Some(pinned) = pinned.upgrade() {
                        super::sync(&pinned);
                    }
                });
                self.hadjust_handler.replace(Some(handler));
            }
        }

        fn set_vadjustment(&self, adjustment: Option<gtk::Adjustment>) {
            self.vadjustment.replace(adjustment.clone());
            if let Some(view) = self.view.get() {
                view.set_vadjustment(adjustment.as_ref());
            }
        }

        fn set_hscroll_policy(&self, policy: gtk::ScrollablePolicy) {
            self.hscroll_policy.set(policy);
            if let Some(view) = self.view.get() {
                view.set_hscroll_policy(policy);
            }
        }

        fn set_vscroll_policy(&self, policy: gtk::ScrollablePolicy) {
            self.vscroll_policy.set(policy);
            if let Some(view) = self.view.get() {
                view.set_vscroll_policy(policy);
            }
        }
    }
}

glib::wrapper! {
    pub struct PinnedNumbers(ObjectSubclass<imp::PinnedNumbers>)
        @extends adw::Bin, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget, gtk::Scrollable;
}

pub fn wrap_in_scrolled(view: &gtk::ColumnView) -> gtk::ScrolledWindow {
    let gutter_backing = gtk::Box::builder()
        .halign(gtk::Align::Start)
        .can_target(false)
        .can_focus(false)
        .visible(false)
        .css_classes(["view"])
        .build();
    gutter_backing.set_accessible_role(gtk::AccessibleRole::Presentation);
    let gutter_header = gtk::Box::builder().can_focus(false).visible(false).build();
    gutter_header.set_accessible_role(gtk::AccessibleRole::Presentation);
    gutter_header.set_cursor_from_name(Some("default"));
    let overlay = gtk::Overlay::new();
    overlay.set_child(Some(view));
    overlay.add_overlay(&gutter_backing);
    overlay.add_overlay(&gutter_header);
    let pinned: PinnedNumbers = glib::Object::new();
    pinned.set_child(Some(&overlay));
    install(&pinned, view, &gutter_backing, &gutter_header);
    gtk::ScrolledWindow::builder()
        .child(&pinned)
        .hexpand(true)
        .vexpand(true)
        .build()
}

fn install(pinned: &PinnedNumbers, view: &gtk::ColumnView, backing: &gtk::Box, header: &gtk::Box) {
    let imp = pinned.imp();
    let _ = imp.view.set(view.clone());
    let _ = imp.gutter_backing.set(backing.clone());
    let _ = imp.gutter_header.set(header.clone());
    view.set_hadjustment(pinned.hadjustment().as_ref());
    view.set_vadjustment(pinned.vadjustment().as_ref());
    view.set_hscroll_policy(pinned.hscroll_policy());
    view.set_vscroll_policy(pinned.vscroll_policy());
    let weak = pinned.downgrade();
    header.connect_state_flags_changed(move |proxy, previous| {
        let current = proxy.state_flags();
        if (previous ^ current).contains(gtk::StateFlags::PRELIGHT)
            && let Some(pinned) = weak.upgrade()
        {
            set_header_hover(&pinned, current.contains(gtk::StateFlags::PRELIGHT));
        }
    });
    if let Some(overlay) = pinned.child().and_downcast::<gtk::Overlay>() {
        let weak = pinned.downgrade();
        overlay.connect_get_child_position(move |overlay, child| {
            let Some(pinned) = weak.upgrade() else {
                return None;
            };
            let Some(header) = pinned.imp().gutter_header.get() else {
                return None;
            };
            if child != header.upcast_ref::<gtk::Widget>() {
                return None;
            }
            let Some(view) = pinned.imp().view.get() else {
                return Some(gtk::gdk::Rectangle::new(0, 0, 0, 0));
            };
            let area = column_header(view).and_then(|header| header.compute_bounds(overlay));
            let Some((area, column)) = area.zip(row_number_column::row_numbers_column(view)) else {
                return Some(gtk::gdk::Rectangle::new(0, 0, 0, 0));
            };
            let width = column.fixed_width();
            let x = match overlay.direction() {
                gtk::TextDirection::Rtl => overlay.width() - width,
                _ => 0,
            };
            Some(gtk::gdk::Rectangle::new(
                x,
                area.y() as i32,
                width,
                area.height() as i32,
            ))
        });
    }
    sync(pinned);
}

pub(super) fn sync(pinned: &PinnedNumbers) {
    let imp = pinned.imp();
    let (Some(view), Some(backing), Some(header)) = (imp.view.get(), imp.gutter_backing.get(), imp.gutter_header.get())
    else {
        return;
    };
    if let Some(column) = row_number_column::row_numbers_column(view) {
        backing.set_width_request(column.fixed_width());
    }
    let scrolled = imp
        .hadjustment
        .borrow()
        .as_ref()
        .is_some_and(|adjustment| adjustment.value() > 0.0);
    backing.set_visible(scrolled);
    header.set_visible(row_number_column::row_numbers_column(view).is_some());
    pinned.queue_draw();
}

fn bounds(pinned: &PinnedNumbers) -> Option<gtk::graphene::Rect> {
    let backing = pinned.imp().gutter_backing.get()?;
    let view = pinned.imp().view.get()?;
    backing.is_visible().then(|| backing.compute_bounds(view)).flatten()
}

fn column_header(view: &gtk::ColumnView) -> Option<gtk::Widget> {
    let mut child = view.first_child();
    while let Some(widget) = child {
        if widget.css_name() == "header" {
            return Some(widget);
        }
        child = widget.next_sibling();
    }
    None
}

fn sync_header_state(pinned: &PinnedNumbers) {
    let Some(proxy) = pinned.imp().gutter_header.get() else {
        return;
    };
    if proxy.is_visible() {
        set_header_hover(pinned, proxy.state_flags().contains(gtk::StateFlags::PRELIGHT));
    }
}

fn set_header_hover(pinned: &PinnedNumbers, hovered: bool) {
    let Some(view) = pinned.imp().view.get() else {
        return;
    };
    if let Some(title) = column_header(view).and_then(|header| header.first_child()) {
        if hovered {
            title.set_state_flags(gtk::StateFlags::PRELIGHT, false);
        } else {
            title.unset_state_flags(gtk::StateFlags::PRELIGHT);
        }
    }
}

fn snapshot_numbers(pinned: &PinnedNumbers, snapshot: &gtk::Snapshot) {
    let Some(gutter) = bounds(pinned) else {
        return;
    };
    let Some(view) = pinned.imp().view.get() else {
        return;
    };
    let mut child = view.first_child();
    while let Some(widget) = child {
        child = widget.next_sibling();
        let Some(area) = widget.compute_bounds(view) else {
            continue;
        };
        snapshot.push_clip(&gtk::graphene::Rect::new(
            gutter.x(),
            area.y(),
            gutter.width(),
            area.height(),
        ));
        if widget.css_name() == "header" {
            if let Some(column) = widget.first_child() {
                snapshot_column(view, &widget, &column, gutter.x(), snapshot);
            }
        } else if widget.is::<gtk::ListView>() {
            let mut row = widget.first_child();
            while let Some(current) = row {
                row = current.next_sibling();
                if let Some(column) = current.first_child()
                    && let Some(content) = column.first_child()
                    && content.has_css_class("row-number-cell")
                    && let Some(rect) = current.compute_bounds(view)
                    && rect.y() < view.height() as f32
                    && rect.y() + rect.height() > area.y()
                {
                    snapshot_column(view, &current, &column, gutter.x(), snapshot);
                }
            }
        }
        snapshot.pop();
    }
}

fn snapshot_column(
    view: &gtk::ColumnView,
    row: &gtk::Widget,
    column: &gtk::Widget,
    target_x: f32,
    snapshot: &gtk::Snapshot,
) {
    let Some(parent) = row.parent() else {
        return;
    };
    let (Some(source), Some(parent_bounds)) = (column.compute_bounds(&parent), parent.compute_bounds(view)) else {
        return;
    };
    let native = gtk::Snapshot::new();
    parent.snapshot_child(row, &native);
    if let Some(node) = native.to_node() {
        snapshot.save();
        snapshot.translate(&gtk::graphene::Point::new(target_x - source.x(), parent_bounds.y()));
        snapshot.append_node(&node);
        snapshot.restore();
    }
}

pub(super) fn gutter_backing(view: &gtk::ColumnView) -> Option<gtk::Box> {
    pinned_for(view)?.imp().gutter_backing.get().cloned()
}

fn pinned_for(view: &gtk::ColumnView) -> Option<PinnedNumbers> {
    view.parent()
        .and_then(|overlay| overlay.parent())
        .and_then(|parent| parent.downcast::<PinnedNumbers>().ok())
}
