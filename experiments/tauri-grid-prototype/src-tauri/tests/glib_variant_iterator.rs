#![cfg(target_os = "linux")]

use glib::{Variant, prelude::*};

#[test]
fn variant_string_iterator_returns_original_values_from_both_ends() {
    let values =
        Variant::array_from_iter::<String>(["first", "grüße", "last"].into_iter().map(|value| value.to_variant()));
    let mut iter = values.array_iter_str().unwrap();
    assert_eq!(iter.next(), Some("first"));
    assert_eq!(iter.next_back(), Some("last"));
    assert_eq!(iter.next(), Some("grüße"));
    assert_eq!(iter.next(), None);
    assert_eq!(values.array_iter_str().unwrap().nth(1), Some("grüße"));
    assert_eq!(values.array_iter_str().unwrap().nth_back(1), Some("grüße"));
    assert_eq!(values.array_iter_str().unwrap().last(), Some("last"));
}
