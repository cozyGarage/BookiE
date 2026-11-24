use bookie_app_services::result_chart::Kind;

use super::{Frame, bar_layout, label_stride, min_width, nearest};

#[test]
fn bars_cap_their_width_and_share_a_group() {
    assert_eq!(bar_layout(100., 1), (36., 0.));
    let (width, gap) = bar_layout(40., 3);
    assert_eq!(gap, 2.);
    assert!((width * 3. + gap * 2. - 32.).abs() < 1e-4);
    assert_eq!(bar_layout(2., 4).0, 1.);
}

#[test]
fn many_groups_need_room_to_scroll() {
    assert_eq!(min_width(200, 1, Kind::Bars), 1500.);
    assert_eq!(min_width(100, 2, Kind::Bars), 1750.);
    assert_eq!(min_width(1000, 3, Kind::Line), 3000.);
}

#[test]
fn labels_thin_out_to_fit_and_the_pointer_finds_its_group() {
    assert_eq!(label_stride(100., 40.), 1);
    assert_eq!(label_stride(10., 40.), 5);
    assert_eq!(nearest(-5., 10., 4), 0);
    assert_eq!(nearest(25., 10., 4), 2);
    assert_eq!(nearest(500., 10., 4), 3);
}

#[test]
fn the_frame_maps_ticks_between_its_margins() {
    let frame = Frame::new(200., 10., &[0., 50., 100.]);
    assert_eq!((frame.y(0.), frame.y(100.)), (frame.bottom, frame.top));
    assert_eq!(frame.bottom, 182.);
    assert!((frame.y(50.) - (frame.top + frame.bottom) / 2.).abs() < 1e-4);
}
