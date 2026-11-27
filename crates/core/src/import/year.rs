pub fn is_year_type(data_type: &str) -> bool {
    let base = data_type.trim().split('(').next().unwrap_or_default().trim();
    base.eq_ignore_ascii_case("year")
}
