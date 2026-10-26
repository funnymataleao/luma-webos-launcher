//! Library page (Home row › View Library, mockup 25.10): every card of one
//! stack source in the Apps grid, with tabs, search and media pages loaded
//! while scrolling. Only titles and poster paths are kept for loaded pages;
//! posters are downloaded for the rows on screen and two rows around them,
//! and the renderer's texture budget drops the ones that scrolled away.
use crate::media::{self, LibraryPage};

use crate::state::Target;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock, mpsc};
use std::time::{Duration, Instant};

pub const COLUMNS: usize = 6;
pub const CARD_W: f32 = 270.;
pub const CARD_H: f32 = 405.;
/// The Apps grid pitch, with Home's poster size.
pub const PITCH_X: f32 = 296.4;
pub const PITCH_Y: f32 = CARD_H + 38.4;
pub const GRID_LEFT: f32 = 84.;
/// Top of the first card row; the header above it stays put.
pub const GRID_TOP: f32 = 212.;
/// Lowest point of a focused card (before its 6 % zoom and ring), so the
/// zoomed card keeps a margin above the screen edge.
pub const GRID_BOTTOM: f32 = 1032.;
/// The next page loads when focus or the view is this close to the end.
const AHEAD_ROWS: usize = 2;
/// Rows above and below the screen whose posters are fetched too.
pub const POSTER_ROWS: usize = 2;
/// A tab reached with the remote opens after focus rests this long on it.
pub const TAB_SETTLE: Duration = Duration::from_millis(350);
pub const OPEN_TIME: f32 = 0.32;
pub const CLOSE_TIME: f32 = 0.24;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct LibItem {
    pub id: i64,
    pub title: String,
    pub poster_url: String,
    /// Downloaded poster file.
    pub poster: Option<String>,
}

#[derive(Clone, Debug)]
pub struct Library {
    pub games: bool,
    /// Stack source: "media" (media), "moonlight", "jellyfin" or "plex".
    pub source: String,
    /// Album lists: square cards.
    pub square: bool,
    /// "Movies", "Series", "Games"…
    pub title: String,
    /// (collection id, label)
    pub tabs: Vec<(String, String)>,
    pub tab: usize,
    /// A tab focused with the remote, opened once focus rests on it.
    pub pending_tab: Option<(usize, Instant)>,
    pub items: Vec<LibItem>,
    pub total: usize,
    /// Pages the list has (as far as known) and pages loaded.
    pub pages: u32,
    pub loaded: u32,
    pub loading: bool,
    /// When a failed page may be asked for again.
    pub retry_at: Option<Instant>,
    /// Search in effect, and the text being typed.
    pub query: String,
    pub draft: String,
    /// Replies of an older tab or search carry an older generation.
    pub generation: u64,
    pub scroll: f32,
    pub scroll_goal: f32,
    /// Last card focused in the grid: Down from the header returns to it.
    pub grid_focus: usize,
    pub opened: Instant,
    /// Grid content changed (tab, search): the cards fade in.
    pub changed: Instant,
    /// Home card focused when the page closes.
    pub restore: Target,
    /// Posters last asked for: (first row, last row, missing count).
    pub wanted: (usize, usize, usize),
    /// Last scroll glide step.
    pub stepped: Instant,
}

impl Library {
    pub fn new(
        games: bool,
        title: String,
        tabs: Vec<(String, String)>,
        tab: usize,
        restore: Target,
    ) -> Self {
        let now = Instant::now();
        Self {
            games,
            source: if games { "moonlight" } else { "jellyfin" }.to_owned(),
            square: false,
            title,
            tabs,
            tab,
            pending_tab: None,
            items: vec![],
            total: 0,
            pages: 1,
            loaded: 0,
            loading: false,
            retry_at: None,
            query: String::new(),
            draft: String::new(),
            generation: next_generation(),
            scroll: 0.,
            scroll_goal: 0.,
            grid_focus: 0,
            opened: now,
            changed: now,
            restore,
            wanted: (0, 0, usize::MAX),
            stepped: now,
        }
    }

    pub fn tab_id(&self) -> &str {
        self.tabs.get(self.tab).map_or("all", |t| t.0.as_str())
    }

    /// Start over for another tab or search.
    pub fn reset(&mut self) {
        self.generation = next_generation();
        self.items.clear();
        self.total = 0;
        self.pages = 1;
        self.loaded = 0;
        self.loading = false;
        self.retry_at = None;
        self.scroll = 0.;
        self.scroll_goal = 0.;
        self.grid_focus = 0;
        self.changed = Instant::now();
        self.wanted = (0, 0, usize::MAX);
    }

    pub fn rows(&self) -> usize {
        self.items.len().div_ceil(COLUMNS)
    }

    /// Card height: posters, or square album covers.
    pub fn card_h(&self) -> f32 {
        if self.square { CARD_W } else { CARD_H }
    }

    pub fn pitch_y(&self) -> f32 {
        self.card_h() + PITCH_Y - CARD_H
    }

    /// Screen y of a card row's top.
    pub fn row_y(&self, row: usize) -> f32 {
        GRID_TOP + row as f32 * self.pitch_y() - self.scroll
    }

    pub fn scroll_max(&self) -> f32 {
        // One more row of room while a page is loading (its placeholders).
        let rows = self.rows() + usize::from(self.loading);
        (rows as f32 * self.pitch_y() - 38.4 + GRID_TOP + 48. - 1080.).max(0.)
    }

    /// Scroll so the card at `index` is fully on screen.
    pub fn reveal(&mut self, index: usize) {
        let row = index / COLUMNS;
        let (pitch, card) = (self.pitch_y(), self.card_h());
        let top = GRID_TOP + row as f32 * pitch - self.scroll_goal;
        if top < GRID_TOP {
            self.scroll_goal = row as f32 * pitch;
        } else if top + card > GRID_BOTTOM {
            self.scroll_goal = row as f32 * pitch + card - (GRID_BOTTOM - GRID_TOP);
        }
        self.scroll_goal = self.scroll_goal.clamp(0., self.scroll_max().max(0.));
    }

    /// Card rows that are on screen at the current scroll.
    pub fn visible_rows(&self) -> (usize, usize) {
        let first = ((self.scroll - self.card_h()) / self.pitch_y())
            .ceil()
            .max(0.) as usize;
        let last = ((self.scroll + 1080. - GRID_TOP) / self.pitch_y())
            .floor()
            .max(0.) as usize;
        (first, last)
    }

    /// The page to fetch now, if any.
    pub fn request(&mut self, link: &PageLink) -> Option<Command> {
        if self.games || self.loading || self.loaded >= self.pages.max(1) {
            return None;
        }
        if self.retry_at.is_some_and(|t| Instant::now() < t) {
            return None;
        }
        let (_, last_row) = self.visible_rows();
        let reach = ((last_row + 1) * COLUMNS).max(self.grid_focus + 1);
        if self.loaded > 0 && reach + AHEAD_ROWS * COLUMNS < self.items.len() {
            return None;
        }
        self.loading = true;
        Some(Command::Page {
            generation: self.generation,
            link: link.clone(),
            tab: self.tab_id().to_owned(),
            query: self.query.clone(),
            page: self.loaded + 1,
        })
    }

    pub fn apply_page(&mut self, page: u32, result: Option<LibraryPage>, items: Vec<LibItem>) {
        self.loading = false;
        let Some(result) = result else {
            self.retry_at = Some(Instant::now() + Duration::from_secs(10));
            return;
        };
        self.retry_at = None;
        self.loaded = page;
        // An empty page ends the list whatever the count said.
        self.pages = if items.is_empty() {
            page
        } else {
            result.pages.max(page)
        };
        self.total = result.total.max(self.items.len() + items.len());
        let seen: std::collections::HashSet<i64> = self.items.iter().map(|i| i.id).collect();
        self.items
            .extend(items.into_iter().filter(|i| !seen.contains(&i.id)));
        if self.loaded >= self.pages {
            self.total = self.items.len();
        }
    }

    pub fn failed(&self) -> bool {
        self.retry_at.is_some()
    }

    /// Posters still missing around the screen, nearest rows first.
    pub fn missing_posters(&self) -> Vec<(i64, String)> {
        let (first, last) = self.visible_rows();
        let from = first.saturating_sub(POSTER_ROWS) * COLUMNS;
        let to = ((last + 1 + POSTER_ROWS) * COLUMNS).min(self.items.len());
        let on_screen = (first * COLUMNS).min(to)..((last + 1) * COLUMNS).min(to);
        let mut list: Vec<(i64, String)> = Vec::new();
        let mut later: Vec<(i64, String)> = Vec::new();
        for (index, item) in self.items.iter().enumerate().take(to).skip(from) {
            if item.poster.is_some() || item.poster_url.is_empty() {
                continue;
            }
            let entry = (item.id, item.poster_url.clone());
            if on_screen.contains(&index) {
                list.push(entry);
            } else {
                later.push(entry);
            }
        }
        list.extend(later);
        list
    }

    /// Progress of the page's open fade.
    pub fn open_progress(&self) -> f32 {
        crate::motion::progress(self.opened.elapsed().as_secs_f32(), OPEN_TIME)
    }

    pub fn animating(&self) -> bool {
        (self.scroll - self.scroll_goal).abs() > 0.5
            || self.opened.elapsed().as_secs_f32() < OPEN_TIME
            || self.changed.elapsed().as_secs_f32() < crate::motion::CARD_CASCADE_END
            || self.pending_tab.is_some()
    }

    /// One frame of the scroll glide.
    pub fn step(&mut self, dt: f32) {
        let k = 1. - (-dt * 14.).exp();
        self.scroll += (self.scroll_goal - self.scroll) * k;
        if (self.scroll - self.scroll_goal).abs() <= 0.5 {
            self.scroll = self.scroll_goal;
        }
    }
}

fn next_generation() -> u64 {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

/// Where a Library page comes from.
#[derive(Clone)]
pub enum PageLink {
    Jellyfin(crate::jellyfin::JellyfinLink),
    Plex(crate::plex::PlexLink),
}

pub enum Command {
    Page {
        generation: u64,
        link: PageLink,
        tab: String,
        query: String,
        page: u32,
    },
}

pub enum Reply {
    Page {
        generation: u64,
        page: u32,
        result: Option<LibraryPage>,
        items: Vec<LibItem>,
    },
    Poster(i64, String),
}

struct Service {
    commands: Mutex<mpsc::Sender<Command>>,
    replies: Mutex<mpsc::Receiver<Reply>>,
    wanted: Arc<Mutex<Vec<(i64, String)>>>,
}

/// The poster folder Home stacks use, and the Library's own one inside it.
fn folders() -> (PathBuf, PathBuf) {
    let cache = if std::path::Path::new("/media/developer/com.denisefremov.luma").exists() {
        PathBuf::from("/media/developer/com.denisefremov.luma/posters")
    } else {
        std::env::temp_dir().join("luma-posters")
    };
    let own = cache.join("library");
    (cache, own)
}

/// Library posters kept on disk; past this the folder starts over.
const DISK_POSTERS: usize = 1500;

fn service() -> &'static Service {
    static SERVICE: OnceLock<Service> = OnceLock::new();
    SERVICE.get_or_init(|| {
        let (home, own) = folders();
        if std::fs::read_dir(&own).is_ok_and(|d| d.count() > DISK_POSTERS) {
            let _ = std::fs::remove_dir_all(&own);
        }
        let _ = std::fs::create_dir_all(&own);
        let (commands, inbox) = mpsc::channel::<Command>();
        let (reply, replies) = mpsc::channel();
        let pages = reply.clone();
        let cached = move |id: i64| {
            [
                home.join(format!("{id}.jpg")),
                own.join(format!("{id}.jpg")),
            ]
            .into_iter()
            .find(|p| p.exists())
            .map(|p| p.to_string_lossy().into_owned())
        };
        std::thread::spawn(move || {
            while let Ok(first) = inbox.recv() {
                // Tabs passed on the way: only the newest request counts.
                let command = inbox.try_iter().last().unwrap_or(first);
                let Command::Page {
                    generation,
                    link,
                    tab,
                    query,
                    page,
                } = command;
                let result = match link {
                    PageLink::Jellyfin(link) => {
                        crate::jellyfin::library_page(&link, &tab, &query, page)
                    }
                    PageLink::Plex(link) => crate::plex::library_page(&link, &tab, &query, page),
                };
                let items = result
                    .as_ref()
                    .map(|r| {
                        r.items
                            .iter()
                            .map(|(id, title, url)| LibItem {
                                id: *id,
                                title: title.clone(),
                                poster_url: url.clone(),
                                poster: cached(*id),
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                let _ = pages.send(Reply::Page {
                    generation,
                    page,
                    result,
                    items,
                });
            }
        });
        let wanted: Arc<Mutex<Vec<(i64, String)>>> = Arc::default();
        let queue = wanted.clone();
        let (_, own) = folders();
        std::thread::spawn(move || {
            loop {
                let next = {
                    let mut list = queue.lock().unwrap();
                    (!list.is_empty()).then(|| list.remove(0))
                };
                match next {
                    Some((id, url)) => {
                        if let Some(path) = media::library_poster(&own, id, &url) {
                            let _ = reply.send(Reply::Poster(id, path));
                        }
                    }
                    None => std::thread::sleep(Duration::from_millis(60)),
                }
            }
        });
        Service {
            commands: Mutex::new(commands),
            replies: Mutex::new(replies),
            wanted,
        }
    })
}

pub fn send(command: Command) {
    let _ = service().commands.lock().unwrap().send(command);
}

/// Replace the poster queue: the cards around the screen now.
pub fn want(list: Vec<(i64, String)>) {
    *service().wanted.lock().unwrap() = list;
}

pub fn poll() -> Vec<Reply> {
    service().replies.lock().unwrap().try_iter().collect()
}

use crate::state::{Direction, Effect, GameCard, State, TextField};

impl State {
    /// View Library on a Home row: the page with every card of that stack.
    pub fn open_library(&mut self, row: usize, index: usize) -> Vec<Effect> {
        let Some(stack) = crate::stacks::row_stack(self, row) else {
            return vec![];
        };
        let config = self.preferences.stacks[stack].clone();
        let games = config.source == "moonlight";
        let server = matches!(config.source.as_str(), "jellyfin" | "plex");
        let tabs: Vec<(String, String)> = if server {
            vec![(
                config.collection.clone(),
                crate::stacks::collection_label(&config),
            )]
        } else {
            crate::stacks::GAME_COLLECTIONS
                .iter()
                .map(|(id, label)| ((*id).to_owned(), (*label).to_owned()))
                .collect()
        };
        // Every Library opens on its leftmost tab.
        let tab = 0;
        let title = if config.source == "jellyfin" {
            "Jellyfin".to_owned()
        } else if config.source == "plex" {
            "Plex".to_owned()
        } else {
            "Games".to_owned()
        };
        let mut library = Library::new(games, title, tabs, tab, Target::StackCard(row, index));
        library.source = config.source.clone();
        library.square =
            config.source == "jellyfin" && crate::jellyfin::music_collection(&config.collection);
        if games {
            library.items = self.library_game_items(&library);
            library.loaded = 1;
        }
        self.library = Some(library);
        self.library_closing = None;
        self.focus_tweens.clear();
        self.focus = Target::LibCard(0);
        self.pointer = false;
        self.dirty = true;
        vec![]
    }

    pub fn close_library(&mut self) {
        if let Some(library) = self.library.take() {
            self.focus = library.restore.clone();
            self.focus_tweens.clear();
            want(vec![]);
            self.library_closing = Some((Box::new(library), Instant::now()));
        }
        self.dirty = true;
    }

    fn library_game_items(&self, library: &Library) -> Vec<LibItem> {
        let query = library.query.trim().to_lowercase();
        self.preferences
            .games
            .select(
                library.tab_id(),
                &self.moonlight_items,
                &self.moonlight_games,
            )
            .into_iter()
            .filter(|item| query.is_empty() || item.title.to_lowercase().contains(&query))
            .map(|item| LibItem {
                id: item.id,
                title: item.title,
                poster_url: String::new(),
                poster: item.poster,
            })
            .collect()
    }

    /// The Apollo list changed: the open games Library shows it at once.
    pub fn library_games_changed(&mut self) {
        let Some(library) = self.library.as_ref().filter(|l| l.games) else {
            return;
        };
        let items = self.library_game_items(library);
        if let Some(library) = self.library.as_mut() {
            library.items = items;
            library.total = library.items.len();
            library.grid_focus = library
                .grid_focus
                .min(library.items.len().saturating_sub(1));
        }
        self.dirty = true;
    }

    /// Back, tabs, search (and its clear button) from left to right.
    pub fn library_header(&self) -> Vec<Target> {
        let Some(library) = self.library.as_ref() else {
            return vec![];
        };
        let mut row = vec![Target::LibBack];
        row.extend((0..library.tabs.len()).map(Target::LibTab));
        row.push(Target::LibSearch);
        if !library.query.is_empty() {
            row.push(Target::LibClear);
        }
        row
    }

    fn library_select_tab(&mut self, tab: usize) {
        let games = {
            let Some(library) = self.library.as_mut() else {
                return;
            };
            library.pending_tab = None;
            if library.tab == tab && library.query.is_empty() {
                return;
            }
            library.tab = tab;
            library.query.clear();
            library.reset();
            library.games
        };
        if games {
            self.library_games_changed();
            if let Some(library) = self.library.as_mut() {
                library.loaded = 1;
            }
        }
        self.dirty = true;
    }

    /// Search typed on the keyboard takes effect.
    pub fn library_commit_search(&mut self) {
        let games = {
            let Some(library) = self.library.as_mut() else {
                return;
            };
            let typed = library.draft.trim().to_owned();
            if typed == library.query {
                return;
            }
            library.query = typed;
            library.reset();
            library.games
        };
        if games {
            self.library_games_changed();
            if let Some(library) = self.library.as_mut() {
                library.loaded = 1;
            }
        }
        if self.library.as_ref().is_some_and(|l| l.items.is_empty()) {
            self.focus = Target::LibSearch;
        }
        self.dirty = true;
    }

    /// Typing in the games Library filters as each letter comes.
    pub fn library_typed(&mut self) {
        if self.library.as_ref().is_some_and(|l| l.games) {
            if let Some(library) = self.library.as_mut() {
                library.query = library.draft.trim().to_owned();
                library.scroll = 0.;
                library.scroll_goal = 0.;
                library.grid_focus = 0;
            }
            self.library_games_changed();
        }
    }

    fn library_grid_entry(&self) -> Option<Target> {
        let library = self.library.as_ref()?;
        if library.items.is_empty() {
            return None;
        }
        // The remembered card, or the first one on screen if it scrolled away.
        let (first, last) = library.visible_rows();
        let row = library.grid_focus / COLUMNS;
        let index = if (first..=last).contains(&row) {
            library.grid_focus
        } else {
            first * COLUMNS
        };
        Some(Target::LibCard(index.min(library.items.len() - 1)))
    }

    pub fn library_direction(&mut self, direction: Direction) -> Vec<Effect> {
        let header = self.library_header();
        let Some(library) = self.library.as_ref() else {
            return vec![];
        };
        let len = library.items.len();
        let tab_target = if library.query.is_empty() {
            Target::LibTab(library.tab)
        } else {
            Target::LibSearch
        };
        let next = match (self.focus.clone(), direction) {
            // An empty list: any press goes to the tabs.
            (Target::LibCard(i), _) if i >= len => Some(tab_target),
            (Target::LibCard(i), direction) => {
                let column = i % COLUMNS;
                match direction {
                    Direction::Left => (column > 0).then(|| Target::LibCard(i - 1)),
                    Direction::Right => {
                        (column + 1 < COLUMNS && i + 1 < len).then(|| Target::LibCard(i + 1))
                    }
                    Direction::Up if i >= COLUMNS => Some(Target::LibCard(i - COLUMNS)),
                    Direction::Up => Some(tab_target),
                    Direction::Down if i + COLUMNS < len => Some(Target::LibCard(i + COLUMNS)),
                    // The last row may be short: land on its last card.
                    Direction::Down if (i / COLUMNS + 1) * COLUMNS < len => {
                        Some(Target::LibCard(len - 1))
                    }
                    Direction::Down => None,
                }
            }
            (focus, Direction::Down) if header.contains(&focus) => self.library_grid_entry(),
            (focus, Direction::Left | Direction::Right) if header.contains(&focus) => {
                let at = header.iter().position(|t| *t == focus).unwrap_or(0);
                match direction {
                    Direction::Left => at.checked_sub(1).map(|a| header[a].clone()),
                    _ => header.get(at + 1).cloned(),
                }
            }
            (focus, _) if header.contains(&focus) => None,
            // Focus lost (a list that emptied): back to the header.
            _ => Some(tab_target),
        };
        let Some(next) = next else {
            self.dirty = true;
            return vec![];
        };
        if let Target::LibCard(i) = next
            && let Some(library) = self.library.as_mut()
        {
            library.grid_focus = i;
            library.reveal(i);
        }
        self.change_focus(next.clone());
        // Like the Apps tabs: the remote opens the tab it moves onto, at once.
        if let Target::LibTab(tab) = next {
            self.library_select_tab(tab);
        }
        self.dirty = true;
        vec![]
    }

    /// OK or a click on a Library control; None for other targets.
    pub fn library_activate(&mut self) -> Option<Vec<Effect>> {
        match self.focus.clone() {
            Target::LibBack => self.close_library(),
            Target::LibTab(tab) => self.library_select_tab(tab),
            Target::LibSearch => {
                if let Some(library) = self.library.as_mut() {
                    library.draft = library.query.clone();
                }
                self.open_keyboard(TextField::Library);
            }
            Target::LibClear => {
                if let Some(library) = self.library.as_mut() {
                    library.draft.clear();
                }
                self.library_commit_search();
                self.focus = Target::LibSearch;
            }
            Target::LibCard(index) => return Some(self.library_open_card(index)),
            _ => return None,
        }
        self.dirty = true;
        Some(vec![])
    }

    fn library_open_card(&mut self, index: usize) -> Vec<Effect> {
        let Some(library) = self.library.as_ref() else {
            return vec![];
        };
        let Some(item) = library.items.get(index) else {
            return vec![];
        };
        let card = |i: &LibItem| crate::stacks::StackItem {
            title: i.title.clone(),
            poster: i.poster.clone(),
            id: i.id,
        };
        let item = card(item);
        if library.games {
            let hdr = self
                .moonlight_games
                .iter()
                .find(|g| g.id == item.id)
                .is_some_and(|g| g.hdr);
            self.game = Some(GameCard::new(
                &item,
                &self.preferences.moonlight.host_name,
                self.moonlight_online,
                hdr,
                Target::LibCard(index),
            ));
            self.focus = Target::GamePlay;
            return vec![Effect::GameInfo(item.id, item.title.clone())];
        }
        if crate::jellyfin::is_album(item.id) {
            return self.open_album(item, Target::LibCard(index));
        }
        // "More like this" picks from the cards around this one.
        let from = index.saturating_sub(12);
        let pool: Vec<crate::stacks::StackItem> =
            library.items.iter().skip(from).take(25).map(card).collect();
        self.open_movie(item, pool, Target::LibCard(index))
    }

    /// Back: from the grid to the tabs first (the grid keeps its place),
    /// then the page closes.
    pub fn library_back(&mut self) {
        let Some(library) = self.library.as_mut() else {
            return;
        };
        library.pending_tab = None;
        if matches!(self.focus, Target::LibCard(_)) {
            let target = if library.query.is_empty() {
                Target::LibTab(library.tab)
            } else {
                Target::LibSearch
            };
            self.change_focus(target);
        } else {
            self.close_library();
        }
        self.dirty = true;
    }

    /// The remote's wheel: the grid moves, focus stays where it is.
    pub fn library_wheel(&mut self, pixels: f32) {
        if let Some(library) = self.library.as_mut() {
            library.scroll_goal =
                (library.scroll_goal + pixels).clamp(0., library.scroll_max().max(0.));
            self.dirty = true;
        }
    }

    /// The pointer holds the grid and pulls it (rubber band past the ends).
    pub fn library_drag(&mut self, movement: f32) {
        if let Some(library) = self.library.as_mut() {
            let max = library.scroll_max().max(0.);
            library.scroll = crate::state::rubber(library.scroll, movement, max);
            library.scroll_goal = library.scroll;
            self.dirty = true;
        }
    }

    /// Let go: coast on with the drag's speed, or spring back past an end.
    pub fn library_release(&mut self, velocity: f32) {
        if let Some(library) = self.library.as_mut() {
            let max = library.scroll_max().max(0.);
            let coast = if self.preferences.reduced_motion {
                0.
            } else {
                velocity * 0.3
            };
            library.scroll_goal = (library.scroll + coast).clamp(0., max);
            self.dirty = true;
        }
    }

    pub fn library_tick(&mut self) {
        if self
            .library_closing
            .as_ref()
            .is_some_and(|(_, t)| t.elapsed().as_secs_f32() >= CLOSE_TIME)
        {
            self.library_closing = None;
            self.dirty = true;
        }
        if self.library_closing.is_some() {
            self.dirty = true;
        }
        let settle = self
            .library
            .as_ref()
            .and_then(|l| l.pending_tab)
            .filter(|(_, at)| at.elapsed() >= TAB_SETTLE)
            .map(|(tab, _)| tab);
        if let Some(tab) = settle
            && self.focus == Target::LibTab(tab)
        {
            self.library_select_tab(tab);
        }
        // An empty list (nothing played yet, no results) puts focus on the
        // tabs, never on a card that is not there.
        let header_target = self.library.as_ref().and_then(|l| {
            let settled = l.games || (!l.loading && l.loaded > 0) || l.failed();
            match self.focus {
                Target::LibCard(i) if settled && i >= l.items.len() => {
                    Some(if l.items.is_empty() {
                        if l.query.is_empty() {
                            Target::LibTab(l.tab)
                        } else {
                            Target::LibSearch
                        }
                    } else {
                        Target::LibCard(l.items.len() - 1)
                    })
                }
                _ => None,
            }
        });
        if let Some(target) = header_target {
            self.change_focus(target);
        }
        let Some(library) = self.library.as_mut() else {
            return;
        };
        let now = Instant::now();
        let dt = now.duration_since(library.stepped).as_secs_f32().min(0.1);
        library.stepped = now;
        library.step(dt);
        if library.animating() {
            self.dirty = true;
        }
    }

    /// Pages the open Library needs now.
    pub fn library_requests(&mut self) -> Vec<Command> {
        let Some(source) = self.library.as_ref().map(|l| l.source.clone()) else {
            return vec![];
        };
        let link = match source.as_str() {
            "jellyfin" => PageLink::Jellyfin(self.preferences.jellyfin.clone()),
            "plex" => PageLink::Plex(self.preferences.plex.clone()),
            _ => return vec![],
        };
        self.library
            .as_mut()
            .and_then(|l| l.request(&link))
            .into_iter()
            .collect()
    }

    /// A Library reply; true when tokens changed and preferences need saving.
    pub fn library_reply(&mut self, reply: Reply) -> bool {
        match reply {
            Reply::Page {
                generation,
                page,
                result,
                items,
            } => {
                if let Some(library) = self.library.as_mut().filter(|l| l.generation == generation)
                {
                    library.apply_page(page, result, items);
                    self.dirty = true;
                }
            }
            Reply::Poster(id, path) => {
                if let Some(library) = self.library.as_mut() {
                    for item in library.items.iter_mut().filter(|i| i.id == id) {
                        item.poster = Some(path.clone());
                    }
                    self.dirty = true;
                }
            }
        }
        false
    }

    /// Ask for the posters around the screen when that set changed.
    pub fn library_posters(&mut self) {
        let Some(library) = self.library.as_mut() else {
            return;
        };
        if library.games {
            return;
        }
        let (first, last) = library.visible_rows();
        let missing = library.missing_posters();
        let key = (first, last, missing.len());
        if key != library.wanted {
            library.wanted = key;
            want(missing);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn library(items: usize) -> Library {
        let mut l = Library::new(
            false,
            "Movies".into(),
            vec![("all".into(), "All".into())],
            0,
            Target::StackCard(0, 20),
        );
        l.items = (0..items)
            .map(|i| LibItem {
                id: i as i64,
                ..Default::default()
            })
            .collect();
        l
    }

    #[test]
    fn next_page_loads_only_near_the_end() {
        let link = PageLink::Jellyfin(crate::jellyfin::JellyfinLink::default());
        let mut l = library(0);
        assert!(l.request(&link).is_some(), "first page at once");
        assert!(l.request(&link).is_none(), "one request at a time");
        l.apply_page(
            1,
            Some(LibraryPage {
                items: vec![],
                pages: 100,
                total: 4800,
            }),
            (0..48)
                .map(|i| LibItem {
                    id: i,
                    ..Default::default()
                })
                .collect(),
        );
        assert_eq!((l.loaded, l.pages, l.items.len()), (1, 100, 48));
        // Top of the list: 48 cards are plenty.
        assert!(l.request(&link).is_none());
        // Focus two rows from the end: the next page is asked for.
        l.grid_focus = 36;
        assert!(matches!(
            l.request(&link),
            Some(Command::Page { page: 2, .. })
        ));
    }

    #[test]
    fn empty_page_ends_the_list_and_failures_wait() {
        let link = PageLink::Jellyfin(crate::jellyfin::JellyfinLink::default());
        let mut l = library(0);
        l.request(&link);
        l.apply_page(1, None, vec![]);
        assert!(l.failed());
        assert!(l.request(&link).is_none(), "no retry storm");
        let mut l = library(0);
        l.request(&link);
        l.apply_page(1, Some(LibraryPage::default()), vec![]);
        assert_eq!(l.pages, 1);
        assert!(l.request(&link).is_none());
    }

    #[test]
    fn posters_are_wanted_around_the_screen_only() {
        let mut l = library(600);
        for item in &mut l.items {
            item.poster_url = "https://x".into();
        }
        l.scroll = 50. * l.pitch_y();
        let wanted = l.missing_posters();
        // Two screen rows plus two above and two below.
        assert!(wanted.len() <= (3 + 2 * POSTER_ROWS) * COLUMNS);
        // The cards on screen come first.
        assert_eq!(wanted[0].0, 50 * COLUMNS as i64);
    }

    #[test]
    fn reveal_keeps_the_focused_row_inside_the_grid() {
        let mut l = library(600);
        l.reveal(13 * COLUMNS);
        let top = GRID_TOP + 13. * l.pitch_y() - l.scroll_goal;
        assert!(top >= GRID_TOP - 0.5 && top + CARD_H <= GRID_BOTTOM + 0.5);
        l.reveal(0);
        assert_eq!(l.scroll_goal, 0.);
    }
}
