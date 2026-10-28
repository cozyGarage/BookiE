pub(super) fn with_on_update(default: String, column_type: &str, extra: &str) -> String {
    let lower_type = column_type.to_ascii_lowercase();
    let temporal = lower_type.starts_with("timestamp") || lower_type.starts_with("datetime");
    let Some((_, action)) = extra.split_once("on update ").filter(|_| temporal) else {
        return default;
    };
    format!("{default} ON UPDATE {}", action.trim().to_ascii_uppercase())
}

pub(super) fn with_invisible(column_type: String, extra: &str) -> String {
    if extra.contains("invisible") {
        format!("{column_type} INVISIBLE")
    } else {
        column_type
    }
}
