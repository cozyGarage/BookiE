#[path = "like_escape.rs"]
mod like_escape;
pub(crate) use super::{connect, start_mariadb, start_mysql};

#[path = "mysql_call_results.rs"]
mod mysql_call_results;
#[path = "result_charset.rs"]
mod result_charset;
