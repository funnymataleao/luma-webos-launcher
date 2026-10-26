//! Album card (mockup "Музыка: альбом", 25.10): the movie card's panel with
//! the cover on the left and the songs on the right. Jellyfin fills it.
use super::{Direction, Effect, State, Target, movie};
use crate::stacks::StackItem;
use std::time::Instant;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Song {
    pub number: i64,
    pub title: String,
    pub artist: String,
    pub ms: u64,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Album {
    pub id: i64,
    pub title: String,
    pub artist: String,
    pub year: i64,
    pub songs: Vec<Song>,
    /// Large square cover, then its blurred wide crop for the panel.
    pub cover: Option<String>,
    pub backdrop: Option<String>,
}

impl Album {
    pub fn minutes(&self) -> u64 {
        self.songs.iter().map(|s| s.ms).sum::<u64>() / 60_000
    }
    /// "2001 · 12 songs · 41 min", leaving out what is unknown.
    pub fn meta(&self) -> String {
        let mut parts = vec![];
        if self.year > 0 {
            parts.push(self.year.to_string());
        }
        match self.songs.len() {
            0 => {}
            1 => parts.push("1 song".into()),
            n => parts.push(format!("{n} songs")),
        }
        if self.minutes() > 0 {
            parts.push(format!("{} min", self.minutes()));
        }
        parts.join(" · ")
    }
}

#[derive(Clone, Debug)]
pub struct AlbumCard {
    pub id: i64,
    pub title: String,
    pub poster: Option<String>,
    pub restore: Target,
    pub opened: Instant,
    /// Text arrived (songs listed); the blocks come in from here.
    pub shown: Option<Instant>,
    pub backdrop_at: Option<Instant>,
    pub album: Option<Album>,
    /// First song row in view.
    pub first: usize,
}

/// Song rows that fit under the buttons.
pub const SONGS_VISIBLE: usize = 8;

impl AlbumCard {
    pub fn new(item: &StackItem, restore: Target) -> Self {
        Self {
            id: item.id,
            title: item.title.clone(),
            poster: item.poster.clone(),
            restore,
            opened: Instant::now(),
            shown: None,
            backdrop_at: None,
            album: None,
            first: 0,
        }
    }
    pub fn songs(&self) -> usize {
        self.album.as_ref().map_or(0, |a| a.songs.len())
    }
    /// Keep song `index` in the window of visible rows.
    pub fn keep_visible(&mut self, index: usize) {
        if index < self.first {
            self.first = index;
        } else if index >= self.first + SONGS_VISIBLE {
            self.first = index + 1 - SONGS_VISIBLE;
        }
        self.first = self.first.min(self.songs().saturating_sub(SONGS_VISIBLE));
    }
}

impl State {
    /// OK on an album card in a stack.
    pub fn open_album(&mut self, item: StackItem, restore: Target) -> Vec<Effect> {
        let id = item.id;
        self.album = Some(AlbumCard::new(&item, restore));
        self.focus = Target::AlbumPlay;
        self.dirty = true;
        vec![Effect::AlbumDetails(id)]
    }
    /// Songs first, then again with the cover and its blurred backdrop.
    pub fn album_details(&mut self, album: Album) {
        let Some(card) = self.album.as_mut().filter(|c| c.id == album.id) else {
            return;
        };
        let now = Instant::now();
        card.shown.get_or_insert(now);
        if album.backdrop.is_some() && card.backdrop_at.is_none() {
            card.backdrop_at = Some(now);
        }
        let mut album = album;
        if let Some(old) = &card.album {
            if album.cover.is_none() {
                album.cover = old.cover.clone();
            }
            if album.backdrop.is_none() {
                album.backdrop = old.backdrop.clone();
            }
        }
        card.album = Some(album);
        self.dirty = true;
    }
    pub fn close_album(&mut self) {
        if let Some(card) = self.album.take() {
            self.focus = card.restore.clone();
            self.album_closing = Some((Box::new(card), Instant::now()));
            self.dirty = true;
        }
    }
    pub fn album_tick(&mut self) {
        if self
            .album_closing
            .as_ref()
            .is_some_and(|(_, t)| t.elapsed().as_secs_f32() >= movie::CLOSE_TIME)
        {
            self.album_closing = None;
            self.dirty = true;
        }
        if self.album_closing.is_some() && !self.preferences.reduced_motion {
            self.dirty = true;
        }
        // Nothing came back in time: show the card with what the stack had.
        if let Some(card) = self.album.as_mut()
            && card.shown.is_none()
            && card.opened.elapsed().as_secs_f32() >= super::game::GAME_WAIT * 4.
        {
            card.shown = Some(Instant::now());
            self.dirty = true;
        }
        if self.album_animating() {
            self.dirty = true;
        }
    }
    pub fn album_animating(&self) -> bool {
        if self.preferences.reduced_motion {
            return false;
        }
        self.album.as_ref().is_some_and(|c| {
            c.opened.elapsed().as_secs_f32() < movie::OPEN_TIME
                || c.shown
                    .is_some_and(|t| t.elapsed().as_secs_f32() < movie::REVEAL_TIME)
                || c.backdrop_at
                    .is_some_and(|t| t.elapsed().as_secs_f32() < movie::BACKDROP_TIME)
        })
    }
    /// Close on top; Play and Shuffle; the songs below, one per row.
    pub fn album_direction(&mut self, direction: Direction) -> Vec<Effect> {
        let Some(card) = self.album.as_mut() else {
            return vec![];
        };
        let songs = card.songs();
        let next = match (&self.focus, direction) {
            (Target::AlbumClose, Direction::Down | Direction::Left) => Some(Target::AlbumPlay),
            (Target::AlbumPlay, Direction::Right) => Some(Target::AlbumShuffle),
            (Target::AlbumShuffle, Direction::Left) => Some(Target::AlbumPlay),
            (Target::AlbumPlay | Target::AlbumShuffle, Direction::Up) => Some(Target::AlbumClose),
            (Target::AlbumShuffle, Direction::Right) => Some(Target::AlbumClose),
            (Target::AlbumPlay | Target::AlbumShuffle, Direction::Down) if songs > 0 => {
                Some(Target::AlbumSong(card.first))
            }
            (Target::AlbumSong(0), Direction::Up) => Some(Target::AlbumPlay),
            (Target::AlbumSong(n), Direction::Up) => Some(Target::AlbumSong(n - 1)),
            (Target::AlbumSong(n), Direction::Down) if n + 1 < songs => {
                Some(Target::AlbumSong(n + 1))
            }
            (Target::AlbumSong(_), _) => None,
            (Target::AlbumClose | Target::AlbumPlay | Target::AlbumShuffle, _) => None,
            _ => Some(Target::AlbumPlay),
        };
        if let Some(next) = next {
            if let Target::AlbumSong(n) = next {
                card.keep_visible(n);
            } else if next == Target::AlbumPlay {
                card.first = 0;
            }
            self.focus = next;
            self.dirty = true;
        }
        vec![]
    }
    /// The music queue is this album.
    pub fn album_playing(&self, id: i64) -> bool {
        self.music.now.as_ref().is_some_and(|n| n.album_id == id)
    }

    pub fn album_activate(&mut self) -> Vec<Effect> {
        let Some(card) = self.album.as_ref() else {
            return vec![];
        };
        let id = card.id;
        let songs = card.songs();
        match self.focus {
            Target::AlbumClose => {
                self.close_album();
                vec![]
            }
            // This album already plays: the buttons steer it.
            Target::AlbumPlay if self.album_playing(id) => {
                self.music.toggle();
                vec![]
            }
            Target::AlbumShuffle if self.album_playing(id) => {
                self.music.toggle_shuffle();
                vec![]
            }
            Target::AlbumSong(n) if n < songs && self.album_playing(id) => {
                self.music.play_song(n);
                vec![]
            }
            Target::AlbumPlay if songs > 0 => vec![Effect::AlbumPlay(id, 0, false)],
            Target::AlbumShuffle if songs > 0 => vec![Effect::AlbumPlay(id, 0, true)],
            Target::AlbumSong(n) if n < songs => vec![Effect::AlbumPlay(id, n, false)],
            _ => vec![],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn meta_leaves_out_unknown_parts() {
        let mut album = Album {
            year: 2001,
            songs: vec![
                Song {
                    ms: 1_200_000,
                    ..Default::default()
                },
                Song {
                    ms: 1_260_000,
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        assert_eq!(album.meta(), "2001 · 2 songs · 41 min");
        album.year = 0;
        album.songs.truncate(1);
        assert_eq!(album.meta(), "1 song · 20 min");
    }

    #[test]
    fn song_window_follows_focus() {
        let mut card = AlbumCard::new(&StackItem::default(), Target::GamePlay);
        card.album = Some(Album {
            songs: vec![Song::default(); 12],
            ..Default::default()
        });
        card.keep_visible(9);
        assert_eq!(card.first, 2);
        card.keep_visible(11);
        assert_eq!(card.first, 4);
        card.keep_visible(0);
        assert_eq!(card.first, 0);
    }
}
