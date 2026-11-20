use std::path::{Path, PathBuf};

use super::{expand_tilde, expand_tilde_with};

fn expand(path: &str, home: Option<&str>) -> PathBuf {
    expand_tilde_with(Path::new(path), home.map(Path::new))
}

#[test]
fn a_leading_tilde_becomes_the_home_directory() {
    assert_eq!(
        expand("~/data/out.csv", Some("/home/ada")),
        PathBuf::from("/home/ada/data/out.csv")
    );
    assert_eq!(expand("~", Some("/home/ada")), PathBuf::from("/home/ada"));
    assert_eq!(expand("~/", Some("/home/ada")), PathBuf::from("/home/ada"));
}

#[test]
fn anything_else_is_left_exactly_as_typed() {
    assert_eq!(expand("/abs/x", Some("/home/ada")), PathBuf::from("/abs/x"));
    assert_eq!(expand("rel/x", Some("/home/ada")), PathBuf::from("rel/x"));
    assert_eq!(expand("~bob/x", Some("/home/ada")), PathBuf::from("~bob/x"));
    assert_eq!(expand("a/~/b", Some("/home/ada")), PathBuf::from("a/~/b"));
    assert_eq!(expand("", Some("/home/ada")), PathBuf::from(""));
}

#[test]
fn without_a_home_directory_the_tilde_stays() {
    assert_eq!(expand("~/x", None), PathBuf::from("~/x"));
    assert_eq!(expand("~", None), PathBuf::from("~"));
}

#[test]
fn the_environment_version_agrees_with_the_explicit_one() {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    assert_eq!(
        expand_tilde(Path::new("~/x")),
        expand_tilde_with(Path::new("~/x"), home.as_deref())
    );
}
