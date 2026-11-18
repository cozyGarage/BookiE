pub(crate) const XS: i32 = 4;
pub(crate) const SM: i32 = 8;
pub(crate) const MD: i32 = 12;
pub(crate) const ICON: i32 = 14;
pub(crate) const ICON_SMALL: i32 = 12;
pub(crate) const TREE_ROW_PADDING: i32 = 2;
pub(crate) const TREE_MIN_WIDTH: i32 = 180;

pub(crate) fn tree_indent(depth: u8) -> i32 {
    XS + MD * i32::from(depth)
}

pub(crate) fn tree_text_indent(depth: u8) -> i32 {
    tree_indent(depth) + ICON + SM
}
