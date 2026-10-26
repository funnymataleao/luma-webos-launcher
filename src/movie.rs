//! Title card: the popup that OK on a stack card opens (movie or series).
//! Navigation is explicit per row, like the approved mockup; the pointer
//! uses the same targets through the scene's controls.
use super::{Direction, Effect, State, Target};
use crate::media::{Detail, PlayRequest};
use crate::motion;
use crate::stacks::StackItem;
use std::time::Instant;

/// Episode stills: 352 wide, 28 apart; four fit next to each other.
pub const EPISODE_PITCH: f32 = 380.;
pub const EPISODES_VISIBLE: usize = 4;
/// "More like this" posters: the Home stack poster (270 wide) and pitch.
pub const SIMILAR_PITCH: f32 = crate::state::HOME_TILE_PITCH;
pub const SIMILAR_VISIBLE: usize = 5;
/// The movie page slides up by this much while "More like this" has focus:
/// the 405 high posters then end 72 above the panel bottom.
pub const SIMILAR_LIFT: f32 = 420.;
/// Side panel rows: 68 high, 4 apart.
pub const OPTION_PITCH: f32 = 72.;
pub const OPTIONS_VISIBLE: usize = 10;
pub const OPEN_TIME: f32 = 0.42;
/// How long a stack card holds focus before its details are prefetched.
pub const PREFETCH_DWELL: f32 = 0.3;
/// Longest wait for one prefetch before the next may start.
const PREFETCH_BUSY: f32 = 6.;
pub const CLOSE_TIME: f32 = 0.22;
pub const PANEL_TIME: f32 = 0.32;
/// The content waits for the details (the stack title is only a stand-in,
/// "RU / EN") and then comes in block by block; slow details give up after
/// REVEAL_WAIT and show what there is.
pub const REVEAL_WAIT: f32 = 0.5;
/// One block slides in over BLOCK_TIME, each one STAGGER after the previous.
pub const BLOCK_TIME: f32 = 0.38;
pub const STAGGER: f32 = 0.05;
/// Bottom row cards come in from the right, CARD_STAGGER apart.
pub const ROW_DELAY: f32 = 0.18;
pub const CARD_STAGGER: f32 = 0.045;
pub const REVEAL_TIME: f32 = ROW_DELAY + 6. * CARD_STAGGER + BLOCK_TIME;
pub const BACKDROP_TIME: f32 = 0.35;

/// Pills: 0 audio, 1 subtitles, 2 quality.
pub const AUDIO: usize = 0;
pub const SUBTITLES: usize = 1;
pub const QUALITY: usize = 2;

#[derive(Clone, Debug)]
pub struct MovieCard {
    pub id: i64,
    pub title: String,
    /// Small poster of the stack card, shown until the big one arrives.
    pub poster: Option<String>,
    pub detail: Option<Detail>,
    /// Index into detail.seasons.
    pub season: usize,
    /// 0 = Auto, n = detail.qualities[n - 1].
    pub quality: usize,
    /// 0 = Off, n = detail.subtitles[n - 1].
    pub subtitle: usize,
    /// Open side panel (AUDIO, SUBTITLES or QUALITY) and when it opened.
    pub panel: Option<(usize, Instant)>,
    pub opened: Instant,
    /// When the title started to come in (None while waiting for details).
    pub shown: Option<Instant>,
    /// When the details arrived: everything but the title comes in from here.
    pub detail_at: Option<Instant>,
    /// When the backdrop picture arrived, for its fade.
    pub backdrop_at: Option<Instant>,
    /// The blurred stand-in a sharper backdrop replaced: drawn underneath
    /// while the new one fades in, so the panel never goes black.
    pub backdrop_under: Option<String>,
    /// Cards of the stack row the title came from ("More like this").
    pub pool: Vec<StackItem>,
    /// Focus to return to on close.
    pub restore: Target,
    /// Seasons whose stills were already asked for.
    pub requested: Vec<usize>,
    /// (current, goal) offsets that the tick chases.
    pub episodes_x: (f32, f32),
    pub similar_x: (f32, f32),
    pub lift: (f32, f32),
    pub options_y: (f32, f32),
    pub last_tick: Instant,
}

impl MovieCard {
    pub fn serial(&self) -> bool {
        self.detail.as_ref().is_some_and(|d| d.serial)
    }
    /// Pills shown, in order.
    pub fn pills(&self) -> Vec<usize> {
        let Some(d) = &self.detail else {
            return vec![];
        };
        // No audio pill: the voice-over can only be switched inside the
        // player, so a picker here did nothing (remove it).
        let mut out = vec![];
        if !d.subtitles.is_empty() {
            out.push(SUBTITLES);
        }
        if d.qualities.len() > 1 {
            out.push(QUALITY);
        }
        out
    }
    pub fn pill_label(&self, kind: usize) -> String {
        let Some(d) = &self.detail else {
            return String::new();
        };
        match kind {
            AUDIO => d.audio.first().cloned().unwrap_or_default(),
            SUBTITLES if self.subtitle > 0 => d
                .subtitles
                .get(self.subtitle - 1)
                .map_or("Off".into(), |s| s.0.clone()),
            SUBTITLES => "Off".into(),
            _ if self.quality > 0 => d
                .qualities
                .get(self.quality - 1)
                .cloned()
                .unwrap_or_else(|| "Auto".into()),
            _ => "Auto".into(),
        }
    }
    /// Rows of the open side panel.
    pub fn options(&self, kind: usize) -> Vec<String> {
        let Some(d) = &self.detail else {
            return vec![];
        };
        match kind {
            AUDIO => d.audio.clone(),
            SUBTITLES => std::iter::once("Off".to_owned())
                .chain(d.subtitles.iter().map(|s| s.0.clone()))
                .collect(),
            _ => std::iter::once("Auto".to_owned())
                .chain(d.qualities.iter().cloned())
                .collect(),
        }
    }
    /// The row that is chosen now.
    pub fn chosen(&self, kind: usize) -> usize {
        match kind {
            AUDIO => 0,
            SUBTITLES => self.subtitle,
            _ => self.quality,
        }
    }
    /// Episode indices (into detail.episodes) of the shown season.
    pub fn season_episodes(&self) -> Vec<usize> {
        let Some(d) = &self.detail else {
            return vec![];
        };
        let Some(&season) = d.seasons.get(self.season) else {
            return vec![];
        };
        (0..d.episodes.len())
            .filter(|&i| d.episodes[i].season == season)
            .collect()
    }
    pub fn similar(&self) -> Vec<&StackItem> {
        self.pool
            .iter()
            .filter(|i| i.id > 0 && i.id != self.id)
            .take(16)
            .collect()
    }
    /// Label of the main button: "Play", or "Continue · S1 E5" for series.
    pub fn play_label(&self) -> String {
        let Some(d) = self.detail.as_ref().filter(|d| d.serial) else {
            return "Play".into();
        };
        match d.next.and_then(|i| d.episodes.get(i)) {
            Some(e) if e.progress > 0.02 || d.episodes.iter().any(|e| e.watched) => {
                format!("Continue · S{} E{}", e.season, e.number)
            }
            Some(e) => format!("Play · S{} E{}", e.season, e.number),
            None => "Play".into(),
        }
    }
    fn request(&self, episode: Option<(i64, i64)>) -> PlayRequest {
        let d = self.detail.as_ref();
        PlayRequest {
            id: self.id,
            episode,
            quality: (self.quality > 0)
                .then(|| d.and_then(|d| d.qualities.get(self.quality - 1).cloned()))
                .flatten(),
            subtitle: (self.subtitle > 0)
                .then(|| d.and_then(|d| d.subtitles.get(self.subtitle - 1).map(|s| s.0.clone())))
                .flatten(),
        }
    }
    /// Stills of the shown season that were not asked for yet.
    fn missing_thumbs(&mut self) -> Option<Effect> {
        if self.requested.contains(&self.season) {
            return None;
        }
        self.requested.push(self.season);
        let list: Vec<(usize, String)> = self
            .season_episodes()
            .into_iter()
            .filter_map(|i| {
                let e = &self.detail.as_ref()?.episodes[i];
                (e.thumb.is_none() && !e.thumb_url.is_empty()).then(|| (i, e.thumb_url.clone()))
            })
            .collect();
        (!list.is_empty()).then_some(Effect::MediaThumbs(self.id, list))
    }
}

/// Keep `index` within a window of `visible` items: returns the new first item.
fn keep_visible(first: f32, pitch: f32, index: usize, visible: usize, count: usize) -> f32 {
    let mut first = (first / pitch).round() as usize;
    if index < first {
        first = index;
    } else if index >= first + visible {
        first = index + 1 - visible;
    }
    first.min(count.saturating_sub(visible)) as f32 * pitch
}

impl State {
    /// OK on a stack card: open its title card and ask for the details.
    pub fn open_movie(
        &mut self,
        item: StackItem,
        pool: Vec<StackItem>,
        restore: Target,
    ) -> Vec<Effect> {
        let now = Instant::now();
        let id = item.id;
        self.movie = Some(MovieCard {
            id,
            title: item.title,
            poster: item.poster,
            detail: None,
            season: 0,
            quality: 0,
            subtitle: 0,
            panel: None,
            opened: now,
            shown: None,
            detail_at: None,
            backdrop_at: None,
            backdrop_under: None,
            pool,
            restore,
            requested: vec![],
            episodes_x: (0., 0.),
            similar_x: (0., 0.),
            lift: (0., 0.),
            options_y: (0., 0.),
            last_tick: now,
        });
        self.movie_closing = None;
        self.change_focus(Target::MoviePlay);
        self.dirty = true;
        // Prefetched on focus: open complete, then refresh watch marks.
        if let Some(detail) = self.movie_cache.get(&id).cloned() {
            self.movie_details(detail);
        }
        vec![Effect::MediaDetails(id)]
    }

    /// Details from media source: remembered for the next open, then shown.
    pub fn media_detail(&mut self, mut detail: Detail) {
        if self
            .movie_prefetch_busy
            .is_some_and(|(id, _)| id == detail.id)
            && detail.backdrop.is_some()
        {
            self.movie_prefetch_busy = None;
        }
        if let Some(old) = self.movie_cache.get(&detail.id) {
            if detail.poster.is_none() {
                detail.poster = old.poster.clone();
            }
            if detail.backdrop.is_none() {
                detail.backdrop = old.backdrop.clone();
            }
        }
        self.movie_cache.insert(detail.id, detail.clone());
        self.movie_details(detail);
    }

    /// A media source stack card that has held focus for a moment and whose
    /// details were never asked for: fetch them before OK is pressed.
    pub fn movie_prefetch(&mut self) -> Option<i64> {
        let focus = self.focus.clone();
        let Target::StackCard(row, index) = focus else {
            self.movie_prefetch.0 = None;
            return None;
        };
        match &self.movie_prefetch.0 {
            Some((target, _)) if *target == focus => {}
            _ => {
                self.movie_prefetch.0 = Some((focus, Instant::now()));
                return None;
            }
        }
        if self
            .movie_prefetch
            .0
            .as_ref()
            .is_some_and(|(_, since)| since.elapsed().as_secs_f32() < PREFETCH_DWELL)
        {
            return None;
        }
        if self
            .movie_prefetch_busy
            .is_some_and(|(_, since)| since.elapsed().as_secs_f32() < PREFETCH_BUSY)
        {
            return None;
        }
        let stack = crate::stacks::row_stack(self, row)?;
        if self.preferences.stacks[stack].source == "moonlight" {
            return None;
        }
        // The focused card first, then its neighbours on screen, one per
        // frame: by the time OK lands on one, its banner is usually on disk.
        let items = self.stack_items.get(stack)?;
        let id = [
            index,
            index + 1,
            index + 2,
            index.wrapping_sub(1),
            index + 3,
            index + 4,
        ]
        .into_iter()
        .filter_map(|i| items.get(i))
        .map(|item| item.id)
        .find(|id| {
            *id > 0 && !self.movie_cache.contains_key(id) && !self.movie_prefetch.1.contains(id)
        })?;
        self.movie_prefetch.1.insert(id);
        self.movie_prefetch_busy = Some((id, Instant::now()));
        Some(id)
    }

    /// Details arrive twice: text first, then the pictures.
    pub fn movie_details(&mut self, detail: Detail) {
        let Some(card) = self.movie.as_mut().filter(|m| m.id == detail.id) else {
            return;
        };
        let mut detail = detail;
        let now = Instant::now();
        card.shown.get_or_insert(now);
        if card.detail.is_none() {
            card.detail_at = Some(now);
        }
        if detail.backdrop.is_some() && card.backdrop_at.is_none() {
            card.backdrop_at = Some(now);
        }
        if let Some(old) = &card.detail {
            // A refresh sends text first: keep the pictures already shown.
            if detail.poster.is_none() {
                detail.poster = old.poster.clone();
            }
            if detail.backdrop.is_none() {
                detail.backdrop = old.backdrop.clone();
            } else if old.backdrop.is_some() && old.backdrop != detail.backdrop {
                card.backdrop_under = old.backdrop.clone();
            }
            // Keep stills already fetched for other seasons.
            for (new, old) in detail.episodes.iter_mut().zip(&old.episodes) {
                if new.thumb.is_none() {
                    new.thumb = old.thumb.clone();
                }
            }
        } else {
            card.title = detail.title.clone();
            card.season = detail
                .next
                .and_then(|i| detail.episodes.get(i))
                .and_then(|e| detail.seasons.iter().position(|&s| s == e.season))
                .unwrap_or(0);
            // details() downloads the stills of this season itself.
            card.requested.push(card.season);
            if let Some(next) = detail.next {
                let episodes: Vec<usize> = (0..detail.episodes.len())
                    .filter(|&i| detail.episodes[i].season == detail.episodes[next].season)
                    .collect();
                if let Some(pos) = episodes.iter().position(|&i| i == next) {
                    let goal =
                        keep_visible(0., EPISODE_PITCH, pos, EPISODES_VISIBLE, episodes.len());
                    card.episodes_x = (goal, goal);
                }
            }
        }
        card.detail = Some(detail);
        self.dirty = true;
    }

    pub fn movie_thumbs(&mut self, id: i64, done: Vec<(usize, String)>) {
        let Some(detail) = self
            .movie
            .as_mut()
            .filter(|m| m.id == id)
            .and_then(|m| m.detail.as_mut())
        else {
            return;
        };
        for (index, path) in done {
            if let Some(e) = detail.episodes.get_mut(index) {
                e.thumb = Some(path);
            }
        }
        self.dirty = true;
    }

    fn close_movie(&mut self) {
        if let Some(card) = self.movie.take() {
            self.focus = card.restore.clone();
            self.focus_tweens.clear();
            self.movie_closing = Some((Box::new(card), Instant::now()));
        }
        self.dirty = true;
    }

    fn close_movie_panel(&mut self) {
        let Some(card) = self.movie.as_mut() else {
            return;
        };
        if let Some((kind, _)) = card.panel.take() {
            self.change_focus(Target::MoviePill(kind));
        }
    }

    /// Back: the side panel first, then the card.
    pub fn movie_back(&mut self) {
        if self.movie.as_ref().is_some_and(|m| m.panel.is_some()) {
            self.close_movie_panel();
        } else {
            self.close_movie();
        }
    }

    pub fn movie_direction(&mut self, direction: Direction) -> Vec<Effect> {
        let Some(card) = self.movie.as_ref() else {
            return vec![];
        };
        let focus = self.focus.clone();
        let pills = card.pills();
        let serial = card.serial();
        let trailer = card.detail.as_ref().is_some_and(|d| d.trailer.is_some());
        let similar = card.similar().len();
        let episodes = card.season_episodes();
        let seasons = card.detail.as_ref().map_or(0, |d| d.seasons.len());
        let season = card.season;
        let lift = |s: &mut Self, up: bool| {
            if let Some(m) = s.movie.as_mut() {
                m.lift.1 = if up { SIMILAR_LIFT } else { 0. };
            }
        };
        // Side panel: a vertical list; Left or Back closes it.
        if let Some((kind, _)) = card.panel {
            let count = card.options(kind).len();
            let next = match (focus.clone(), direction) {
                (_, Direction::Left) => {
                    self.close_movie_panel();
                    return vec![];
                }
                (Target::MovieOption(0), Direction::Up) => Some(Target::MoviePanelBack),
                (Target::MovieOption(i), Direction::Up) => Some(Target::MovieOption(i - 1)),
                (Target::MovieOption(i), Direction::Down) if i + 1 < count => {
                    Some(Target::MovieOption(i + 1))
                }
                (Target::MoviePanelBack, Direction::Down) if count > 0 => {
                    Some(Target::MovieOption(card.chosen(kind).min(count - 1)))
                }
                _ => None,
            };
            if let Some(target) = next {
                if let Target::MovieOption(i) = target
                    && let Some(m) = self.movie.as_mut()
                {
                    m.options_y.1 =
                        keep_visible(m.options_y.1, OPTION_PITCH, i, OPTIONS_VISIBLE, count);
                }
                self.change_focus(target);
            }
            return vec![];
        }
        // The top row, the same on movies and series: Play, Trailer, pickers.
        let top: Vec<Target> = std::iter::once(Target::MoviePlay)
            .chain(trailer.then_some(Target::MovieTrailer))
            .chain(pills.iter().map(|&k| Target::MoviePill(k)))
            .collect();
        let in_row = |row: &[Target]| row.iter().position(|t| *t == focus);
        let step = |row: &[Target], s: &mut Self| -> bool {
            let Some(i) = in_row(row) else {
                return false;
            };
            let next = match direction {
                Direction::Left => i.checked_sub(1),
                Direction::Right => (i + 1 < row.len()).then_some(i + 1),
                _ => None,
            };
            if let Some(n) = next {
                s.change_focus(row[n].clone());
            } else if matches!(direction, Direction::Right) && row == top.as_slice() {
                s.change_focus(Target::MovieClose);
            }
            true
        };
        let mut effects = vec![];
        match (&focus, direction) {
            (Target::MovieClose, Direction::Down | Direction::Left) => {
                self.change_focus(Target::MoviePlay);
            }
            (Target::MovieClose, _) => {}
            (_, Direction::Left | Direction::Right) if in_row(&top).is_some() => {
                step(&top, self);
            }
            (_, Direction::Up) if in_row(&top).is_some() => self.change_focus(Target::MovieClose),
            // Movie: pills, then Play and Trailer, then More like this.
            (Target::MoviePlay | Target::MovieTrailer, Direction::Left | Direction::Right) => {
                step(&buttons(trailer), self);
            }
            (Target::MoviePlay | Target::MovieTrailer, Direction::Up) => {
                self.change_focus(top.first().cloned().unwrap_or(Target::MovieClose));
            }
            // Series: main row, season tabs, episodes.
            (_, Direction::Down) if serial && in_row(&top).is_some() && seasons > 0 => {
                self.change_focus(Target::MovieSeason(season));
            }
            (Target::MoviePill(_), Direction::Down) => self.change_focus(Target::MoviePlay),
            (Target::MoviePlay | Target::MovieTrailer, Direction::Down)
                if !serial && similar > 0 =>
            {
                let first = self.movie.as_ref().map_or(0., |m| m.similar_x.1);
                let index = ((first / SIMILAR_PITCH).round() as usize).min(similar - 1);
                lift(self, true);
                self.change_focus(Target::MovieSimilar(index));
            }
            (Target::MovieSimilar(i), Direction::Left | Direction::Right) => {
                let next = match direction {
                    Direction::Left => i.checked_sub(1),
                    _ => (i + 1 < similar).then_some(i + 1),
                };
                if let Some(n) = next {
                    if let Some(m) = self.movie.as_mut() {
                        m.similar_x.1 =
                            keep_visible(m.similar_x.1, SIMILAR_PITCH, n, SIMILAR_VISIBLE, similar);
                    }
                    self.change_focus(Target::MovieSimilar(n));
                }
            }
            (Target::MovieSimilar(_), Direction::Up) => {
                lift(self, false);
                self.change_focus(Target::MoviePlay);
            }
            (Target::MovieSeason(_), Direction::Up) => self.change_focus(Target::MoviePlay),
            (Target::MovieSeason(i), Direction::Left | Direction::Right) => {
                let next = match direction {
                    Direction::Left => i.checked_sub(1),
                    _ => (i + 1 < seasons).then_some(i + 1),
                };
                if let Some(n) = next {
                    // Moving onto a tab shows its season, like the top navigation.
                    if let Some(m) = self.movie.as_mut() {
                        m.season = n;
                        m.episodes_x.1 = 0.;
                        effects.extend(m.missing_thumbs());
                    }
                    self.change_focus(Target::MovieSeason(n));
                }
            }
            (Target::MovieSeason(_), Direction::Down) if !episodes.is_empty() => {
                let first = self.movie.as_ref().map_or(0., |m| m.episodes_x.1);
                let pos = ((first / EPISODE_PITCH).round() as usize).min(episodes.len() - 1);
                let next = self
                    .movie
                    .as_ref()
                    .and_then(|m| m.detail.as_ref()?.next)
                    .and_then(|n| episodes.iter().position(|&e| e == n))
                    .filter(|&p| p >= pos && p < pos + EPISODES_VISIBLE)
                    .unwrap_or(pos);
                self.change_focus(Target::MovieEpisode(episodes[next]));
            }
            (Target::MovieEpisode(_), Direction::Up) => {
                self.change_focus(Target::MovieSeason(season));
            }
            (Target::MovieEpisode(e), Direction::Left | Direction::Right) => {
                let Some(pos) = episodes.iter().position(|x| x == e) else {
                    return vec![];
                };
                let next = match direction {
                    Direction::Left => pos.checked_sub(1),
                    _ => (pos + 1 < episodes.len()).then_some(pos + 1),
                };
                if let Some(n) = next {
                    if let Some(m) = self.movie.as_mut() {
                        m.episodes_x.1 = keep_visible(
                            m.episodes_x.1,
                            EPISODE_PITCH,
                            n,
                            EPISODES_VISIBLE,
                            episodes.len(),
                        );
                    }
                    self.change_focus(Target::MovieEpisode(episodes[n]));
                }
            }
            _ => {}
        }
        effects
    }

    pub fn movie_activate(&mut self) -> Vec<Effect> {
        let Some(card) = self.movie.as_ref() else {
            return vec![];
        };
        match self.focus.clone() {
            Target::MovieClose => self.close_movie(),
            Target::MoviePlay => {
                let episode = card
                    .detail
                    .as_ref()
                    .and_then(|d| d.next.and_then(|i| d.episodes.get(i)))
                    .map(|e| (e.season, e.number));
                return vec![Effect::MediaPlay(card.request(episode))];
            }
            Target::MovieEpisode(i) => {
                let episode = card
                    .detail
                    .as_ref()
                    .and_then(|d| d.episodes.get(i))
                    .map(|e| (e.season, e.number));
                return vec![Effect::MediaPlay(card.request(episode))];
            }
            Target::MovieTrailer => {
                if let Some(url) = card.detail.as_ref().and_then(|d| d.trailer.clone()) {
                    return vec![Effect::PlayUrl(url, format!("{} · Trailer", card.title))];
                }
            }
            Target::MoviePill(kind) => {
                let count = card.options(kind).len();
                let chosen = card.chosen(kind).min(count.saturating_sub(1));
                if let Some(m) = self.movie.as_mut() {
                    m.panel = Some((kind, Instant::now()));
                    let goal = keep_visible(0., OPTION_PITCH, chosen, OPTIONS_VISIBLE, count);
                    m.options_y = (goal, goal);
                }
                self.change_focus(Target::MovieOption(chosen));
            }
            Target::MovieOption(i) => {
                let kind = card.panel.map(|p| p.0);
                if let Some(m) = self.movie.as_mut() {
                    match kind {
                        Some(SUBTITLES) => m.subtitle = i,
                        Some(QUALITY) => m.quality = i,
                        _ => {}
                    }
                }
                self.close_movie_panel();
            }
            Target::MoviePanelBack => self.close_movie_panel(),
            Target::MovieSeason(i) => {
                if let Some(m) = self.movie.as_mut()
                    && m.season != i
                {
                    m.season = i;
                    m.episodes_x.1 = 0.;
                    return m.missing_thumbs().into_iter().collect();
                }
            }
            Target::MovieSimilar(i) => {
                let Some(item) = card.similar().get(i).map(|i| (*i).clone()) else {
                    return vec![];
                };
                let pool = card.pool.clone();
                let restore = card.restore.clone();
                return self.open_movie(item, pool, restore);
            }
            _ => {}
        }
        vec![]
    }

    pub fn movie_tick(&mut self) {
        if let Some((_, closed)) = &self.movie_closing
            && closed.elapsed().as_secs_f32() >= CLOSE_TIME
        {
            self.movie_closing = None;
            self.dirty = true;
        }
        let reduced = self.preferences.reduced_motion;
        let Some(m) = self.movie.as_mut() else {
            return;
        };
        let dt = m.last_tick.elapsed().as_secs_f32().min(0.05);
        m.last_tick = Instant::now();
        let mut moved = false;
        if m.shown.is_none() && m.opened.elapsed().as_secs_f32() >= REVEAL_WAIT {
            m.shown = Some(Instant::now());
            moved = true;
        }
        for pair in [
            &mut m.episodes_x,
            &mut m.similar_x,
            &mut m.lift,
            &mut m.options_y,
        ] {
            if pair.0 != pair.1 {
                pair.0 = motion::chase(pair.0, pair.1, dt, reduced);
                moved = true;
            }
        }
        // A frame is drawn only when dirty: keep drawing through the entrance.
        if moved || self.movie_animating() {
            self.dirty = true;
        }
    }

    pub fn movie_animating(&self) -> bool {
        if self.preferences.reduced_motion {
            return false;
        }
        self.movie_closing.is_some()
            || self.movie.as_ref().is_some_and(|m| {
                m.opened.elapsed().as_secs_f32() < OPEN_TIME
                    || m.shown
                        .is_none_or(|t| t.elapsed().as_secs_f32() < REVEAL_TIME)
                    || m.detail_at
                        .is_some_and(|t| t.elapsed().as_secs_f32() < REVEAL_TIME)
                    || m.backdrop_at
                        .is_some_and(|t| t.elapsed().as_secs_f32() < BACKDROP_TIME)
                    || m.panel
                        .is_some_and(|(_, t)| t.elapsed().as_secs_f32() < PANEL_TIME)
                    || [m.episodes_x, m.similar_x, m.lift, m.options_y]
                        .iter()
                        .any(|p| p.0 != p.1)
            })
    }
}

/// What a wheel turn or a pointer drag on the card moves.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Scroll {
    /// The page itself (up and down).
    Page,
    /// The "More like this" row (sideways).
    Similar,
    /// The list of the open side panel.
    Panel,
}

impl MovieCard {
    fn axis(&mut self, scroll: Scroll) -> (&mut (f32, f32), f32, f32) {
        match scroll {
            Scroll::Page => {
                let max = if self.serial() || self.similar().is_empty() {
                    0.
                } else {
                    SIMILAR_LIFT
                };
                (&mut self.lift, max, 120.)
            }
            Scroll::Similar => {
                let count = self.similar().len();
                let max = count.saturating_sub(SIMILAR_VISIBLE) as f32 * SIMILAR_PITCH;
                (&mut self.similar_x, max, SIMILAR_PITCH)
            }
            Scroll::Panel => {
                let count = self.panel.map_or(0, |(k, _)| self.options(k).len());
                let max = count.saturating_sub(OPTIONS_VISIBLE) as f32 * OPTION_PITCH;
                (&mut self.options_y, max, OPTION_PITCH)
            }
        }
    }
}

impl State {
    /// Wheel on the card: whole cards or rows, a page step for the page.
    pub fn movie_wheel(&mut self, scroll: Scroll, steps: f32) {
        let Some(m) = self.movie.as_mut() else {
            return;
        };
        let (pair, max, step) = m.axis(scroll);
        pair.1 = (pair.1 + steps * step).clamp(0., max);
        self.dirty = true;
    }

    /// Pointer drag on the card: follows the hand, stretches past the ends.
    pub fn movie_drag(&mut self, scroll: Scroll, movement: f32) {
        let reduced = self.preferences.reduced_motion;
        let Some(m) = self.movie.as_mut() else {
            return;
        };
        let (pair, max, _) = m.axis(scroll);
        let next = if reduced {
            (pair.0 + movement).clamp(0., max)
        } else {
            super::rubber(pair.0, movement, max)
        };
        *pair = (next, next);
        self.dirty = true;
    }

    /// Drag released: coast with the throw and settle on a whole card or row.
    pub fn movie_release(&mut self, scroll: Scroll, velocity: f32) {
        let throw = if self.preferences.reduced_motion {
            0.
        } else {
            velocity * 0.22
        };
        let Some(m) = self.movie.as_mut() else {
            return;
        };
        let (pair, max, step) = m.axis(scroll);
        let goal = pair.0 + throw;
        pair.1 = if scroll == Scroll::Page {
            goal.clamp(0., max)
        } else {
            ((goal / step).round() * step).clamp(0., max)
        };
        self.dirty = true;
    }
}

fn buttons(trailer: bool) -> Vec<Target> {
    if trailer {
        vec![Target::MoviePlay, Target::MovieTrailer]
    } else {
        vec![Target::MoviePlay]
    }
}

#[cfg(debug_assertions)]
impl State {
    /// Desktop QA only: a card filled with sample text and bundled wallpapers.
    pub fn movie_preview(&mut self, kind: &str, assets: &std::path::Path) {
        use crate::media::Episode;
        let image = |name: &str| {
            Some(
                assets
                    .join("wallpapers")
                    .join(name)
                    .to_string_lossy()
                    .into_owned(),
            )
        };
        let serial = kind.starts_with("series");
        let pool: Vec<StackItem> = [
            "forest.jpg",
            "dunes.jpg",
            "midnight.jpg",
            "prism.jpg",
            "northern-lights.jpg",
        ]
        .iter()
        .cycle()
        .take(9)
        .enumerate()
        .map(|(i, f)| StackItem {
            title: format!("Sample {i}"),
            poster: image(f),
            id: i as i64 + 2,
        })
        .collect();
        let item = StackItem {
            title: String::new(),
            poster: image("prism.jpg"),
            id: 1,
        };
        self.open_movie(item, pool, Target::StackCard(0, 0));
        let episodes: Vec<Episode> = (1..=3)
            .flat_map(|season| {
                (1..=8).map(move |number| Episode {
                    season,
                    number,
                    title: format!("Episode title {number}"),
                    minutes: 48 + number,
                    progress: if season == 1 && number == 5 { 0.38 } else { 0. },
                    watched: season == 1 && number < 5,
                    thumb_url: String::new(),
                    thumb: None,
                })
            })
            .collect();
        let mut detail = Detail {
            id: 1,
            serial,
            title: if serial {
                "The Extraordinary Adventures of a Very Long Series Title".into()
            } else {
                "OK! Madam".into()
            },
            year: 2020,
            minutes: 106,
            genres: "Comedy, Action".into(),
            countries: "South Korea".into(),
            plot: "A couple who run a small bakery finally take their dream trip to Hawaii. Then agents hijack the plane, and the quiet wife turns out to be the only one on board who knows how to fight. Sample text for layout checks only.".into(),
            director: "Sample Director".into(),
            cast: "Sample Actor, Sample Actress, Another Name".into(),
            imdb: Some(6.5),
            kinopoisk: Some(6.8),
            tags: vec!["4K".into(), "5.1".into()],
            poster: image("prism.jpg"),
            backdrop: image("northern-lights.jpg"),
            audio: vec!["Russian · HDRezka".into(), "English · Original".into()],
            subtitles: (0..12).map(|i| (format!("Language {i}"), String::new())).collect(),
            qualities: vec!["2160p".into(), "1080p".into(), "720p".into()],
            trailer: Some("http://example.invalid/trailer".into()),
            seasons: if serial { vec![1, 2, 3] } else { vec![] },
            episodes: if serial { episodes } else { vec![] },
            next: if serial { Some(4) } else { None },
        };
        for (i, e) in detail.episodes.iter_mut().enumerate() {
            e.thumb = image(["forest.jpg", "dunes.jpg", "midnight.jpg"][i % 3]);
        }
        self.movie_details(detail);
        if let Some(m) = self.movie.as_mut() {
            m.opened = Instant::now() - std::time::Duration::from_secs(2);
            m.shown = Some(m.opened);
            m.detail_at = m.shown;
            m.backdrop_at = Some(m.opened);
            // Desktop QA of the entrance: freeze it this many seconds in.
            if let Some(at) = std::env::var("LUMA_MOVIE_PREVIEW_AT")
                .ok()
                .and_then(|v| v.parse::<f32>().ok())
            {
                m.shown = Some(Instant::now() - std::time::Duration::from_secs_f32(at));
                m.detail_at = m.shown;
                m.backdrop_at = m.shown;
            }
        }
        if kind.ends_with(":panel") {
            self.change_focus(Target::MoviePill(SUBTITLES));
            self.movie_activate();
            if let Some(m) = self.movie.as_mut()
                && let Some(p) = m.panel.as_mut()
            {
                p.1 = Instant::now() - std::time::Duration::from_secs(2);
            }
        }
        self.focus_tweens.clear();
    }
}
