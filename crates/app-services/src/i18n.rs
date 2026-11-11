#[macro_export]
macro_rules! tr {
    ($s:expr $(,)?) => {
        ::gettextrs::gettext($s)
    };
}

#[macro_export]
macro_rules! trn {
    ($one:expr, $many:expr, $n:expr $(,)?) => {
        ::gettextrs::ngettext($one, $many, u32::try_from($n).unwrap_or(u32::MAX))
    };
}
