use serde::{Deserialize, Serialize};

pub const MAX_NAME_CHARS: usize = 80;
pub const MAX_TAGS: usize = 8;
pub const MAX_TAG_CHARS: usize = 24;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TabColor {
    Red,
    Orange,
    Yellow,
    Green,
    Teal,
    Blue,
    Purple,
    Pink,
}

impl TabColor {
    pub const ALL: [TabColor; 8] = [
        TabColor::Red,
        TabColor::Orange,
        TabColor::Yellow,
        TabColor::Green,
        TabColor::Teal,
        TabColor::Blue,
        TabColor::Purple,
        TabColor::Pink,
    ];

    pub fn id(self) -> &'static str {
        match self {
            TabColor::Red => "red",
            TabColor::Orange => "orange",
            TabColor::Yellow => "yellow",
            TabColor::Green => "green",
            TabColor::Teal => "teal",
            TabColor::Blue => "blue",
            TabColor::Purple => "purple",
            TabColor::Pink => "pink",
        }
    }

    fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|color| color.id() == id)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "RawTabMark")]
pub struct TabMark {
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    color: Option<TabColor>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tags: Vec<String>,
}

#[derive(Deserialize, Default)]
struct RawTabMark {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    color: Option<String>,
    #[serde(default)]
    tags: Vec<String>,
}

impl From<RawTabMark> for TabMark {
    fn from(raw: RawTabMark) -> Self {
        let color = raw.color.as_deref().and_then(TabColor::from_id);
        TabMark::new(
            raw.name.as_deref().unwrap_or(""),
            color,
            raw.tags.iter().map(String::as_str),
        )
    }
}

impl TabMark {
    pub fn new<'a>(name: &str, color: Option<TabColor>, tags: impl IntoIterator<Item = &'a str>) -> Self {
        let name = clean(name, MAX_NAME_CHARS);
        let mut kept: Vec<String> = Vec::new();
        for tag in tags {
            let tag = clean(tag, MAX_TAG_CHARS);
            let Some(tag) = tag else { continue };
            if kept.iter().any(|seen| seen.to_lowercase() == tag.to_lowercase()) {
                continue;
            }
            kept.push(tag);
            if kept.len() == MAX_TAGS {
                break;
            }
        }
        Self {
            name,
            color,
            tags: kept,
        }
    }

    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    pub fn color(&self) -> Option<TabColor> {
        self.color
    }

    pub fn tags(&self) -> &[String] {
        &self.tags
    }

    pub fn is_empty(&self) -> bool {
        self.name.is_none() && self.color.is_none() && self.tags.is_empty()
    }

    pub fn title_or<'a>(&'a self, automatic: &'a str) -> &'a str {
        self.name().unwrap_or(automatic)
    }

    #[must_use]
    pub fn with_name(&self, name: &str) -> Self {
        Self::new(name, self.color, self.tags.iter().map(String::as_str))
    }

    #[must_use]
    pub fn with_color(&self, color: Option<TabColor>) -> Self {
        Self { color, ..self.clone() }
    }

    #[must_use]
    pub fn with_tags<'a>(&self, tags: impl IntoIterator<Item = &'a str>) -> Self {
        Self::new(self.name().unwrap_or(""), self.color, tags)
    }
}

fn clean(text: &str, max_chars: usize) -> Option<String> {
    let flattened: String = text.chars().map(|c| if c.is_control() { ' ' } else { c }).collect();
    let trimmed = flattened.split_whitespace().collect::<Vec<_>>().join(" ");
    if trimmed.is_empty() {
        return None;
    }
    Some(
        trimmed
            .chars()
            .take(max_chars)
            .collect::<String>()
            .trim_end()
            .to_owned(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_name_falls_back_to_the_automatic_title_and_a_set_name_wins() {
        assert_eq!(TabMark::default().title_or("SELECT 1"), "SELECT 1");
        assert_eq!(TabMark::default().with_name("   ").title_or("SELECT 1"), "SELECT 1");
        assert_eq!(
            TabMark::default().with_name("Orders audit").title_or("SELECT 1"),
            "Orders audit"
        );
    }

    #[test]
    fn names_and_tags_are_flattened_trimmed_and_capped() {
        let mark = TabMark::new(
            "  line\none\u{7}\t two  ",
            None,
            [" a ", "A", "", "b\nc", &"x".repeat(60)],
        );
        assert_eq!(mark.name(), Some("line one two"));
        assert_eq!(mark.tags()[..2], ["a".to_owned(), "b c".to_owned()]);
        assert_eq!(mark.tags()[2].chars().count(), MAX_TAG_CHARS);
        let long = TabMark::new(&"é".repeat(500), None, []);
        assert_eq!(long.name().unwrap().chars().count(), MAX_NAME_CHARS);
        let many: Vec<String> = (0..20).map(|n| format!("t{n}")).collect();
        let capped = TabMark::new("", None, many.iter().map(String::as_str));
        assert_eq!(capped.tags().len(), MAX_TAGS);
    }

    #[test]
    fn a_mark_round_trips_and_an_empty_mark_writes_nothing() {
        let mark = TabMark::new("Orders audit", Some(TabColor::Teal), ["audit", "q4"]);
        let json = serde_json::to_string(&mark).unwrap();
        assert_eq!(serde_json::from_str::<TabMark>(&json).unwrap(), mark);
        assert_eq!(serde_json::to_string(&TabMark::default()).unwrap(), "{}");
    }

    #[test]
    fn unknown_colours_and_oversized_values_in_a_file_are_tolerated_not_fatal() {
        let loaded: TabMark =
            serde_json::from_str(r#"{"name":"  ","color":"chartreuse","tags":["a","a"],"future":1}"#).unwrap();
        assert!(loaded.name().is_none() && loaded.color().is_none());
        assert_eq!(loaded.tags(), ["a".to_owned()]);
        assert!(serde_json::from_str::<TabMark>("{}").unwrap().is_empty());
    }
}
