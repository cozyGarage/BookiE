use gtk4 as gtk;
use gtk4::gdk;
use gtk4::gio;
use gtk4::prelude::*;

pub(super) fn configure(menu: &gtk::PopoverMenu) {
    menu.set_has_arrow(false);
    menu.set_position(trailing_side(menu.direction()));
    menu.set_valign(gtk::Align::Start);
}

pub(super) fn from_model(model: Option<&impl IsA<gio::MenuModel>>) -> gtk::PopoverMenu {
    let menu = gtk::PopoverMenu::from_model(model);
    configure(&menu);
    menu
}

pub(super) fn popup_at(menu: &gtk::PopoverMenu, pointing: Option<gdk::Rectangle>) {
    let shown = menu
        .parent()
        .map(|parent| gdk::Rectangle::new(0, 0, parent.width(), parent.height()));
    let pointing = pointing.map(|rect| shown.map_or(rect, |shown| in_view(rect, shown)));
    menu.set_pointing_to(pointing.as_ref());
    menu.popup();
}

fn in_view(rect: gdk::Rectangle, shown: gdk::Rectangle) -> gdk::Rectangle {
    rect.intersect(&shown).unwrap_or(rect)
}

fn trailing_side(direction: gtk::TextDirection) -> gtk::PositionType {
    match direction {
        gtk::TextDirection::Rtl => gtk::PositionType::Left,
        _ => gtk::PositionType::Right,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_menu_opens_on_the_side_text_runs_toward() {
        assert_eq!(trailing_side(gtk::TextDirection::Ltr), gtk::PositionType::Right);
        assert_eq!(trailing_side(gtk::TextDirection::None), gtk::PositionType::Right);
        assert_eq!(trailing_side(gtk::TextDirection::Rtl), gtk::PositionType::Left);
    }

    #[test]
    fn a_menu_points_at_the_part_of_its_cell_in_view() {
        let shown = gdk::Rectangle::new(0, 0, 400, 300);

        assert_eq!(
            in_view(gdk::Rectangle::new(250, 40, 600, 24), shown),
            gdk::Rectangle::new(250, 40, 150, 24)
        );
        assert_eq!(
            in_view(gdk::Rectangle::new(-200, 40, 300, 24), shown),
            gdk::Rectangle::new(0, 40, 100, 24)
        );
        assert_eq!(
            in_view(gdk::Rectangle::new(40, 60, 1, 1), shown),
            gdk::Rectangle::new(40, 60, 1, 1)
        );
        assert_eq!(
            in_view(gdk::Rectangle::new(500, 40, 100, 24), shown),
            gdk::Rectangle::new(500, 40, 100, 24)
        );
    }
}
