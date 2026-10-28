use std::cell::{Cell, RefCell};
use std::rc::Rc;

use relm4::adw;
use relm4::gtk::prelude::*;
use relm4::gtk::subclass::prelude::*;
use relm4::gtk::{self, gdk, glib, graphene};

use tablepro_app_services::er_diagram::layout::{self, Point, Size};
use tablepro_app_services::er_diagram::scene::{self, Card, EdgePath, Rect};
use tablepro_app_services::er_diagram::{SchemaDiagram, zoom};

use super::card_text::CardText;
use super::draw::{self, DETAIL_ZOOM, Frame};
use super::palette::{self, Palette};

const PAN_STEP: f64 = 60.0;
const WHEEL_PAN: f64 = 40.0;

type SelectedHandler = Rc<dyn Fn(Option<usize>)>;

#[derive(Default)]
struct Scene {
    diagram: SchemaDiagram,
    texts: Vec<CardText>,
    cards: Vec<Card>,
    edges: Vec<EdgePath>,
    order: Vec<usize>,
    extent: Option<Rect>,
}

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct DiagramCanvas {
        pub(super) scene: RefCell<Scene>,
        pub(super) selected: Cell<Option<usize>>,
        pub(super) zoom: Cell<f64>,
        pub(super) offset: Cell<(f64, f64)>,
        pub(super) fitted: Cell<bool>,
        pub(super) drag_origin: Cell<(f64, f64)>,
        pub(super) on_selected: RefCell<Option<SelectedHandler>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for DiagramCanvas {
        const NAME: &'static str = "BookieDiagramCanvas";
        type Type = super::DiagramCanvas;
        type ParentType = gtk::Widget;

        fn class_init(klass: &mut Self::Class) {
            klass.set_css_name("bookie-diagram");
            klass.set_accessible_role(gtk::AccessibleRole::Group);
        }
    }

    impl ObjectImpl for DiagramCanvas {
        fn constructed(&self) {
            self.parent_constructed();
            let canvas = self.obj();
            self.zoom.set(1.0);
            canvas.set_focusable(true);
            canvas.set_hexpand(true);
            canvas.set_vexpand(true);
            canvas.set_overflow(gtk::Overflow::Hidden);
            super::install_controllers(&canvas);
        }
    }

    impl WidgetImpl for DiagramCanvas {
        fn size_allocate(&self, width: i32, height: i32, baseline: i32) {
            self.parent_size_allocate(width, height, baseline);
            let canvas = self.obj();
            if !self.fitted.get() && width > 0 && height > 0 && self.scene.borrow().extent.is_some() {
                self.fitted.set(true);
                canvas.fit();
            }
        }

        fn measure(&self, _orientation: gtk::Orientation, _for_size: i32) -> (i32, i32, i32, i32) {
            (200, 400, -1, -1)
        }

        fn snapshot(&self, snapshot: &gtk::Snapshot) {
            self.obj().paint(snapshot);
        }
    }
}

glib::wrapper! {
    pub struct DiagramCanvas(ObjectSubclass<imp::DiagramCanvas>)
        @extends gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl Default for DiagramCanvas {
    fn default() -> Self {
        Self::new()
    }
}

impl DiagramCanvas {
    pub(crate) fn new() -> Self {
        glib::Object::new()
    }

    pub(crate) fn set_diagram(&self, diagram: SchemaDiagram) {
        let context = self.pango_context();
        let texts: Vec<CardText> = diagram
            .tables
            .iter()
            .map(|table| CardText::lay_out(&context, table))
            .collect();
        let sizes: Vec<Size> = texts.iter().map(|text| text.shape.size()).collect();
        let pairs: Vec<(usize, usize)> = diagram
            .relations
            .iter()
            .map(|relation| (relation.from, relation.to))
            .collect();
        let cards: Vec<Card> = layout::arrange(&sizes, &pairs)
            .into_iter()
            .zip(&texts)
            .map(|(at, text)| Card { at, shape: text.shape })
            .collect();
        let edges: Vec<EdgePath> = diagram
            .relations
            .iter()
            .map(|relation| {
                scene::route(
                    &cards[relation.from],
                    &relation.from_columns,
                    &cards[relation.to],
                    &relation.to_columns,
                )
            })
            .collect();
        let extent = scene::extent(&cards, &edges);
        let order = (0..cards.len()).collect();
        *self.imp().scene.borrow_mut() = Scene {
            diagram,
            texts,
            cards,
            edges,
            order,
            extent,
        };
        self.imp().selected.set(None);
        self.imp().fitted.set(false);
        if self.width() > 0 && self.height() > 0 {
            self.imp().fitted.set(true);
            self.fit();
        }
        self.queue_draw();
    }

    pub(crate) fn connect_selected(&self, handler: impl Fn(Option<usize>) + 'static) {
        *self.imp().on_selected.borrow_mut() = Some(Rc::new(handler));
    }

    #[cfg(test)]
    pub(crate) fn card_count(&self) -> usize {
        self.imp().scene.borrow().cards.len()
    }

    #[cfg(test)]
    pub(crate) fn card_rect(&self, index: usize) -> Rect {
        self.imp().scene.borrow().cards[index].rect()
    }

    pub(crate) fn zoom(&self) -> f64 {
        self.imp().zoom.get()
    }

    pub(crate) fn selected(&self) -> Option<usize> {
        self.imp().selected.get()
    }

    pub(crate) fn zoom_in(&self) {
        self.zoom_about_centre(zoom::step_in(self.zoom()));
    }

    pub(crate) fn zoom_out(&self) {
        self.zoom_about_centre(zoom::step_out(self.zoom()));
    }

    pub(crate) fn actual_size(&self) {
        self.zoom_about_centre(1.0);
    }

    pub(crate) fn fit(&self) {
        let Some(extent) = self.imp().scene.borrow().extent else {
            return;
        };
        let (width, height) = (f64::from(self.width()), f64::from(self.height()));
        let level = zoom::fit((extent.width, extent.height), (width, height)).min(1.0);
        self.imp().zoom.set(level);
        self.imp().offset.set((
            (width - extent.width * level) / 2.0 - extent.x * level,
            (height - extent.height * level) / 2.0 - extent.y * level,
        ));
        self.queue_draw();
    }

    pub(crate) fn select_at(&self, x: f64, y: f64) {
        let hit = {
            let scene = self.imp().scene.borrow();
            scene::hit(&scene.cards, &scene.order, Point { x, y })
        };
        self.select(hit);
    }

    pub(crate) fn select(&self, index: Option<usize>) {
        if let Some(found) = index {
            let mut scene = self.imp().scene.borrow_mut();
            scene.order.retain(|&at| at != found);
            scene.order.push(found);
        }
        if self.imp().selected.replace(index) == index {
            return;
        }
        self.queue_draw();
        let handler = self.imp().on_selected.borrow().clone();
        if let Some(handler) = handler {
            handler(index);
        }
    }

    pub(crate) fn reveal(&self, index: usize) {
        let Some(card) = self.imp().scene.borrow().cards.get(index).map(Card::rect) else {
            return;
        };
        let level = self.zoom();
        let (width, height) = (f64::from(self.width()), f64::from(self.height()));
        self.imp().offset.set((
            width / 2.0 - (card.x + card.width / 2.0) * level,
            height / 2.0 - (card.y + card.height / 2.0) * level,
        ));
        self.queue_draw();
    }

    fn zoom_about_centre(&self, level: f64) {
        self.zoom_about(level, f64::from(self.width()) / 2.0, f64::from(self.height()) / 2.0);
    }

    fn zoom_about(&self, level: f64, x: f64, y: f64) {
        let level = zoom::clamp(level);
        let before = self.zoom();
        let (ox, oy) = self.imp().offset.get();
        let ratio = level / before;
        self.imp().zoom.set(level);
        self.imp().offset.set((x - (x - ox) * ratio, y - (y - oy) * ratio));
        self.queue_draw();
    }

    fn pan_by(&self, dx: f64, dy: f64) {
        let (ox, oy) = self.imp().offset.get();
        self.imp().offset.set((ox + dx, oy + dy));
        self.queue_draw();
    }

    fn to_scene(&self, x: f64, y: f64) -> (f64, f64) {
        let (ox, oy) = self.imp().offset.get();
        let level = self.zoom();
        ((x - ox) / level, (y - oy) / level)
    }

    fn palette(&self) -> Palette {
        Palette::from_theme(palette::theme(adw::StyleManager::default().is_dark()))
    }

    fn paint(&self, snapshot: &gtk::Snapshot) {
        let palette = self.palette();
        let (width, height) = (self.width() as f32, self.height() as f32);
        snapshot.append_color(&palette.background, &graphene::Rect::new(0.0, 0.0, width, height));
        let level = self.zoom();
        let (ox, oy) = self.imp().offset.get();
        snapshot.save();
        snapshot.translate(&graphene::Point::new(ox as f32, oy as f32));
        snapshot.scale(level as f32, level as f32);
        let visible = Rect {
            x: -ox / level,
            y: -oy / level,
            width: f64::from(width) / level,
            height: f64::from(height) / level,
        };
        let scene = self.imp().scene.borrow();
        let focused = self.has_focus().then(|| self.imp().selected.get()).flatten();
        let frame = Frame {
            diagram: &scene.diagram,
            texts: &scene.texts,
            cards: &scene.cards,
            edges: &scene.edges,
            order: &scene.order,
            selected: self.imp().selected.get(),
            focused,
            detail: level >= DETAIL_ZOOM,
        };
        draw::draw(snapshot, &frame, &visible, &palette);
        snapshot.restore();
    }

    fn key(&self, key: gdk::Key) -> glib::Propagation {
        match key {
            gdk::Key::plus | gdk::Key::equal | gdk::Key::KP_Add => self.zoom_in(),
            gdk::Key::minus | gdk::Key::KP_Subtract => self.zoom_out(),
            gdk::Key::_0 | gdk::Key::KP_0 => self.actual_size(),
            gdk::Key::Left => self.pan_by(PAN_STEP, 0.0),
            gdk::Key::Right => self.pan_by(-PAN_STEP, 0.0),
            gdk::Key::Up => self.pan_by(0.0, PAN_STEP),
            gdk::Key::Down => self.pan_by(0.0, -PAN_STEP),
            gdk::Key::Escape if self.selected().is_some() => self.select(None),
            _ => return glib::Propagation::Proceed,
        }
        glib::Propagation::Stop
    }

    fn scroll(&self, state: gdk::ModifierType, dx: f64, dy: f64, x: f64, y: f64) {
        if state.contains(gdk::ModifierType::CONTROL_MASK) {
            let factor = if dy < 0.0 { 1.1 } else { 1.0 / 1.1 };
            self.zoom_about(self.zoom() * factor, x, y);
        } else {
            self.pan_by(-dx * WHEEL_PAN, -dy * WHEEL_PAN);
        }
    }
}

fn install_controllers(canvas: &DiagramCanvas) {
    let drag = gtk::GestureDrag::new();
    drag.connect_drag_begin({
        let canvas = canvas.clone();
        move |_, _, _| canvas.imp().drag_origin.set(canvas.imp().offset.get())
    });
    drag.connect_drag_update({
        let canvas = canvas.clone();
        move |_, dx, dy| {
            let (ox, oy) = canvas.imp().drag_origin.get();
            canvas.imp().offset.set((ox + dx, oy + dy));
            canvas.queue_draw();
        }
    });
    canvas.add_controller(drag);

    let click = gtk::GestureClick::new();
    click.connect_pressed({
        let canvas = canvas.clone();
        move |_, _, x, y| {
            canvas.grab_focus();
            let (sx, sy) = canvas.to_scene(x, y);
            canvas.select_at(sx, sy);
        }
    });
    canvas.add_controller(click);

    let keys = gtk::EventControllerKey::new();
    keys.connect_key_pressed({
        let canvas = canvas.clone();
        move |_, key, _, _| canvas.key(key)
    });
    canvas.add_controller(keys);

    let wheel = gtk::EventControllerScroll::new(gtk::EventControllerScrollFlags::BOTH_AXES);
    wheel.connect_scroll({
        let canvas = canvas.clone();
        move |controller, dx, dy| {
            let pointer = controller
                .current_event()
                .and_then(|event| event.position())
                .unwrap_or((0.0, 0.0));
            canvas.scroll(controller.current_event_state(), dx, dy, pointer.0, pointer.1);
            glib::Propagation::Stop
        }
    });
    canvas.add_controller(wheel);
}

#[cfg(test)]
mod tests {
    use relm4::gtk;
    use relm4::gtk::prelude::*;
    use tablepro_app_services::er_diagram::{SchemaDiagram, TableRead};
    use tablepro_core::{ColumnInfo, ForeignKeyInfo};

    use super::DiagramCanvas;

    fn column(name: &str, primary_key: bool) -> ColumnInfo {
        ColumnInfo {
            name: name.into(),
            data_type: "integer".into(),
            nullable: false,
            primary_key,
            is_auto_increment: false,
            default_value: None,
            is_generated: false,
            comment: None,
            collation: None,
            enum_type: None,
            domain_type: None,
        }
    }

    fn shop() -> SchemaDiagram {
        SchemaDiagram::build(vec![
            TableRead {
                schema: None,
                name: "orders".into(),
                columns: vec![column("id", true), column("customer_id", false)],
                foreign_keys: vec![ForeignKeyInfo {
                    name: "orders_customer".into(),
                    columns: vec!["customer_id".into()],
                    ref_schema: None,
                    ref_table: "customers".into(),
                    ref_columns: vec!["id".into()],
                    on_delete: None,
                    on_update: None,
                }],
            },
            TableRead {
                schema: None,
                name: "customers".into(),
                columns: vec![column("id", true)],
                foreign_keys: Vec::new(),
            },
        ])
    }

    fn shown() -> (gtk::Window, DiagramCanvas) {
        relm4::adw::init().unwrap();
        let canvas = DiagramCanvas::new();
        canvas.set_diagram(shop());
        let window = gtk::Window::builder()
            .default_width(900)
            .default_height(600)
            .child(&canvas)
            .build();
        window.present();
        let context = gtk::glib::MainContext::default();
        while context.iteration(false) {}
        (window, canvas)
    }

    #[test]
    #[ignore = "requires an isolated GTK display"]
    fn a_diagram_is_laid_out_with_one_card_per_table_and_fits_the_view() {
        let (window, canvas) = shown();

        assert_eq!(canvas.card_count(), 2);
        assert!(canvas.zoom() > 0.0 && canvas.zoom() <= 1.0);
        let customers = canvas.card_rect(0);
        let orders = canvas.card_rect(1);
        assert!(
            customers.right() <= orders.x,
            "the referenced table sits left of the table holding the key"
        );
        window.destroy();
    }

    #[test]
    #[ignore = "requires an isolated GTK display"]
    fn clicking_a_card_selects_it_and_clicking_empty_space_clears_the_selection() {
        let (window, canvas) = shown();
        let card = canvas.card_rect(1);

        canvas.select_at(card.x + card.width / 2.0, card.y + 4.0);
        assert_eq!(canvas.selected(), Some(1));
        canvas.select_at(card.x - 500.0, card.y - 500.0);
        assert_eq!(canvas.selected(), None);
        window.destroy();
    }

    #[test]
    #[ignore = "requires an isolated GTK display"]
    fn zoom_steps_stay_within_the_limits_and_fit_restores_the_whole_diagram() {
        let (window, canvas) = shown();
        let fitted = canvas.zoom();

        canvas.zoom_in();
        assert!(canvas.zoom() > fitted);
        for _ in 0..40 {
            canvas.zoom_out();
        }
        assert!(canvas.zoom() >= tablepro_app_services::er_diagram::zoom::MIN);
        canvas.fit();
        assert_eq!(canvas.zoom(), fitted);
        window.destroy();
    }
}
