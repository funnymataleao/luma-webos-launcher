//! Home stacks: rows of media from linked sources.
//! Configuration lives in preferences; fetched items live in State.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CardShape {
    /// 2:3 posters, 6 per row plus a peek (movies, series, games).
    #[default]
    Poster,
    /// 1:1, 6 per row plus a peek (music).
    Square,
    /// 16:9, 3 per row (YouTube).
    Wide,
}

impl CardShape {
    pub const ALL: [CardShape; 3] = [CardShape::Poster, CardShape::Square, CardShape::Wide];
    pub fn index(self) -> usize {
        Self::ALL.iter().position(|&s| s == self).unwrap_or(0)
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Poster => "Poster 2:3",
            Self::Square => "Square",
            Self::Wide => "Wide 16:9",
        }
    }
    pub fn hint(self) -> &'static str {
        match self {
            Self::Poster => "Movies, series, games · 6 in a row",
            Self::Square => "Music · 6 in a row",
            Self::Wide => "Videos · 3 in a row",
        }
    }
    /// Card size on the 1920×1080 Home grid.
    pub fn size(self) -> (f32, f32) {
        match self {
            Self::Poster => (270., 405.),
            Self::Square => (270., 270.),
            Self::Wide => (566.4, 318.6),
        }
    }
    pub fn pitch(self) -> f32 {
        match self {
            Self::Wide => 592.8,
            _ => crate::state::HOME_TILE_PITCH,
        }
    }
}

/// Card pitch of the n-th VISIBLE stack row on Home (hidden/empty rows skipped).
pub fn row_pitch(state: &crate::state::State, row: usize) -> f32 {
    visible_rows(state)
        .nth(row)
        .map_or(crate::state::HOME_TILE_PITCH, |(_, s)| s.shape.pitch())
}

/// Cards a Home row shows before its View Library card: long
/// rows made the glide back to the first card stutter.
pub const HOME_ROW_MAX: usize = 20;

/// Whether rows of this source end with the View Library card.
pub fn has_library(source: &str) -> bool {
    matches!(source, "moonlight" | "jellyfin" | "plex")
}

/// Posters the n-th visible row shows on Home (the library card not counted).
pub fn row_shown(state: &crate::state::State, row: usize) -> usize {
    visible_rows(state).nth(row).map_or(0, |(i, _)| {
        state
            .stack_items
            .get(i)
            .map_or(0, |v| v.len().min(HOME_ROW_MAX))
    })
}

/// Number of cards in the n-th visible stack row, with the "View Library"
/// card that ends it.
pub fn row_len(state: &crate::state::State, row: usize) -> usize {
    // Desktop design QA rows (stacks_view): 12 placeholders and the library card.
    #[cfg(debug_assertions)]
    if row < 2
        && visible_rows(state).next().is_none()
        && std::env::var_os("LUMA_STACK_PREVIEW").is_some()
    {
        return 13;
    }
    visible_rows(state).nth(row).map_or(0, |(_, s)| {
        row_shown(state, row) + usize::from(has_library(&s.source))
    })
}

/// Index in preferences.stacks of the n-th visible row.
pub fn row_stack(state: &crate::state::State, row: usize) -> Option<usize> {
    visible_rows(state).nth(row).map(|(i, _)| i)
}

fn visible_rows(state: &crate::state::State) -> impl Iterator<Item = (usize, &StackConfig)> + '_ {
    state
        .preferences
        .stacks
        .iter()
        .enumerate()
        .filter(|(i, s)| s.show && state.stack_items.get(*i).is_some_and(|v| !v.is_empty()))
}

/// (id, name, available now).
pub const SOURCES: [(&str, &str, bool); 3] = [
    ("moonlight", "Moonlight", true),
    ("jellyfin", "Jellyfin", true),
    ("plex", "Plex", true),
];
/// Games stack categories (Moonlight); history is kept by Luma.
pub const GAME_COLLECTIONS: [(&str, &str); 4] = [
    ("recent", "Recently played"),
    ("added", "Recently added"),
    ("all", "All games"),
    ("favorites", "Favorites"),
];
/// Collection, Type, Card shape, Show on Home, Move up, Move down, Remove.
pub const OPTION_COUNT: usize = 7;

/// Sources whose stack options have no Type row and end with a sign-out row:
/// Collection, Card shape, Show on Home, Move up, Move down, Sign out, Remove.
pub fn compact(source: &str) -> bool {
    matches!(source, "moonlight" | "jellyfin" | "plex")
}
pub fn option_count(source: &str) -> usize {
    if source == "moonlight" {
        7
    } else {
        OPTION_COUNT
    }
}

/// Every "what" choice: the fixed ones, then bookmark folders and collections
/// (id, label) from the linked account.
pub fn collection_choices(state: &crate::state::State, stack: usize) -> Vec<(String, String)> {
    if state
        .preferences
        .stacks
        .get(stack)
        .is_some_and(|s| s.source == "moonlight")
    {
        return GAME_COLLECTIONS
            .iter()
            .map(|(id, label)| ((*id).to_owned(), (*label).to_owned()))
            .collect();
    }
    if state
        .preferences
        .stacks
        .get(stack)
        .is_some_and(|s| s.source == "jellyfin")
    {
        return crate::jellyfin::COLLECTIONS
            .iter()
            .map(|(id, label)| ((*id).to_owned(), (*label).to_owned()))
            .chain(state.jellyfin_views.iter().cloned())
            .collect();
    }
    if state
        .preferences
        .stacks
        .get(stack)
        .is_some_and(|s| s.source == "plex")
    {
        return crate::plex::COLLECTIONS
            .iter()
            .map(|(id, label)| ((*id).to_owned(), (*label).to_owned()))
            .chain(state.plex_views.iter().cloned())
            .collect();
    }
    vec![]
}

/// Home row title for a collection and type, e.g. "New Movies".
pub fn row_title(stack: &StackConfig) -> String {
    if stack.source == "moonlight" {
        return match stack.collection.as_str() {
            "recent" => "Recently played",
            "added" => "Recently added games",
            "favorites" => "Favorite games",
            _ => "Games",
        }
        .to_owned();
    }
    if matches!(stack.source.as_str(), "jellyfin" | "plex") {
        return collection_label(stack);
    }
    collection_label(stack)
}

pub fn source_name(id: &str) -> &'static str {
    SOURCES
        .iter()
        .find(|s| s.0 == id)
        .map_or("Media server", |s| s.1)
}
pub fn collection_label(stack: &StackConfig) -> String {
    if stack.source == "moonlight" {
        return GAME_COLLECTIONS
            .iter()
            .find(|c| c.0 == stack.collection)
            .map_or("All games", |c| c.1)
            .to_owned();
    }
    if stack.source == "jellyfin" {
        return crate::jellyfin::COLLECTIONS
            .iter()
            .find(|c| c.0 == stack.collection)
            .map_or_else(|| stack.list_title.clone(), |c| c.1.to_owned());
    }
    if stack.source == "plex" {
        return crate::plex::COLLECTIONS
            .iter()
            .find(|c| c.0 == stack.collection)
            .map_or_else(|| stack.list_title.clone(), |c| c.1.to_owned());
    }
    stack.list_title.clone()
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(default)]
pub struct StackConfig {
    pub name: String,
    /// "moonlight", "jellyfin", "plex".
    pub source: String,
    /// "fresh", "popular", "hot", "continue", "history", "bookmark:<id>", "pick:<id>".
    pub collection: String,
    /// Title of a bookmark folder or collection, shown in Settings.
    pub list_title: String,
    pub shape: CardShape,
    pub show: bool,
}

impl Default for StackConfig {
    fn default() -> Self {
        Self {
            name: "New Movies".into(),
            source: "jellyfin".into(),
            collection: "fresh".into(),
            list_title: String::new(),
            shape: CardShape::Poster,
            show: true,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct StackItem {
    pub title: String,
    /// Cached poster file, when downloaded.
    pub poster: Option<String>,
    /// Media item id (0 for placeholders).
    pub id: i64,
}
