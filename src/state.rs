use crate::i18n::Language;
use crate::model::{
    App, BACKGROUNDS, CATEGORIES, INPUT_NAME_LIMIT, InputMode, Inventory, Preferences,
    clean_input_name,
};
use crate::motion;
use crate::renderer::Draw;
use crate::weather::{self, Unit};
use crate::weather_phrases::{self, GreetingStyle};
use serde::Serialize;
use std::cell::Cell;
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

#[path = "album.rs"]
pub mod album;
#[path = "mini.rs"]
pub mod mini;
pub use album::AlbumCard;
#[path = "game.rs"]
pub mod game;
pub use game::GameCard;
#[path = "movie.rs"]
pub mod movie;
pub use movie::MovieCard;

/// Cross-fade from a clip's first-frame still to the live hardware plane.
/// The still matches the first frame, so the fade only hides the plane's
/// first-frame latency and stays short enough not to ghost over motion.
pub const VIDEO_HANDOFF: Duration = Duration::from_millis(350);
pub const WEATHER_SECTION: usize = 2;
pub const INPUTS_SECTION: usize = 3;
pub const SMARTHOME_SECTION: usize = 4;
/// Smart Home rows scrolled back in stop below the clock and date.
/// How long a scene pill shows its "done" check after a press.
/// How long the full-screen camera waits for a first picture before it says offline.
pub const CAMERA_WAIT: f32 = 8.;
pub const SCENE_FIRED: f32 = 2.;
pub const SMART_REVEAL_TOP: f32 = 186.;
pub const SYSTEM_SECTION: usize = 5;
pub const APPEARANCE_SECTION: usize = 6;
pub const SECTION_COUNT: usize = 8;
/// Settings › Stacks (rows of media on Home).
pub const STACKS_SECTION: usize = 7;
/// Root order: Appearance and Stacks follow Wallpaper; other IDs keep their mapping.
pub const SECTION_ORDER: [usize; SECTION_COUNT] = [0, 1, 6, 7, 2, 3, 4, 5];
pub const APPS_COLUMNS: usize = 6;
pub const HOME_STRIP_LEFT: f32 = 84.;
pub const HOME_STRIP_RIGHT: f32 = 1836.;
pub const HOME_VIEWPORT_LEFT: f32 = 0.;
pub const HOME_VIEWPORT_RIGHT: f32 = 1920.;
pub const HOME_TILE_WIDTH: f32 = 270.;
pub const HOME_TILE_PITCH: f32 = 296.4;
/// Home rows revealed from above keep their card top here (title above it).
pub const HOME_REVEAL_TOP: f32 = 272.;
const HOME_BOTTOM: f32 = 1080.;
const CONTENT_BOTTOM: f32 = 994.4;
const FOCUS_BOTTOM_INSET: f32 = 12.;

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub enum Page {
    Home,
    Apps,
    SmartHome,
    Settings,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize)]
pub enum Target {
    Nav(usize),
    TvSettings,
    App(String),
    Input(String),
    Browse,
    Filter(usize),
    Search,
    ClearSearch,
    Sidebar(usize),
    Setting(usize),
    GreetingName,
    GreetingNameClear,
    GreetingStyle,
    AppNamesHome,
    AppNamesApps,
    ClockFormat,
    ClockTimeFormat,
    Background(usize),
    WallpaperTab(usize),
    WallpaperMenu(usize),
    VideoFilter(usize),
    VideoToggle,
    VideoShuffle,
    VideoGroup(usize),
    VideoSource(usize),
    AccentChoice(usize),
    AccentOption(usize),
    HeadingFontOpen,
    TextFontOpen,
    HeadingSize,
    TextSize,
    PhraseTitleSize,
    PhraseSubtitleSize,
    UiSounds,
    Screensaver,
    UiResolutionOpen,
    UiResolutionPick(usize),
    HeadingFontPick(usize),
    TextFontPick(usize),
    VideoPick(usize),
    VideoNext,
    /// Home caption of the current wallpaper video; Left/Right switch clips.
    VideoTitle,
    /// Home stack card: (stack row, card index).
    StackCard(usize, usize),
    /// Settings › Stacks list row; index == stacks.len() is "Add Stack".
    StackRow(usize),
    StackSource(usize),
    StackOpt(usize),
    StackPick(usize),
    /// Link page rows: 0 server address, 1 Connect.
    MoonlightHost(usize),
    MoonlightRetry,
    MoonlightCancelButton,
    /// Jellyfin sign-in rows: servers found, then "Enter address with your phone".
    JellyfinServer(usize),
    JellyfinNewCode,
    JellyfinCancel,
    PlexNewCode,
    PlexCancel,
    GameClose,
    GamePlay,
    GameWake,
    GameQuit,
    GameFavorite,
    GameTrailer,
    /// Screenshot thumbnail on the game card.
    GameShot(usize),
    /// Album card: close, Play, Shuffle, a song row.
    AlbumClose,
    AlbumPlay,
    AlbumShuffle,
    AlbumSong(usize),
    /// Home mini-player (background music): cover, then the buttons.
    MiniCover,
    MiniShuffle,
    MiniPrev,
    MiniPlay,
    MiniNext,
    MiniRepeat,
    /// Library page (View Library): Back, a tab, search, its clear button, a card.
    LibBack,
    LibTab(usize),
    LibSearch,
    LibClear,
    LibCard(usize),
    VideoSelectAll(usize),
    SettingsBack,
    InputMode(String),
    InputRename,
    InputNameReset,
    Choice(usize),
    CloseDialog,
    Retry,
    DismissNotice,
    Option(usize),
    Key(usize),
    /// A Home widget block, by its index in `preferences.widgets.blocks`.
    Widget(usize),
    /// An item of the widget text line.
    WidgetText(usize),
    /// A tile inside a block (HDMI L inputs, Smart Home L devices).
    WidgetTile(usize, usize),
    WidgetRemove(usize),
    WidgetTextRemove(usize),
    WidgetAdd,
    WidgetTab(usize),
    WidgetCoin(usize),
    WidgetClose,
    WidgetRow(usize),
    WidgetSize(usize),
    WeatherToggle,
    WeatherLocation,
    WeatherTemperature,
    WeatherQuery,
    WeatherSearch,
    WeatherClear,
    WeatherResult(usize),
    SmartConnect,
    SmartShowTab,
    SmartAddress,
    SmartServer(usize),
    SmartRetry,
    /// Connect screen: switch the QR between account sign-in and token paste.
    SmartFallback,
    SmartSignOut,
    SmartSection(usize),
    SmartPick(usize, String),
    /// Smart Home picker shortcuts: 0 recommended, 1 all, 2 none.
    SmartBulk(usize, u8),
    SmartEntity(String),
    SmartCamera(String),
    SmartScene(String),
    SmartClose,
    /// The Hide/Show button of a camera's long-press menu.
    SmartCameraHide,
    SmartBrightness(i8),
    /// Title card (movie popup).
    MovieClose,
    MoviePlay,
    MovieTrailer,
    /// Audio, subtitles or quality pill.
    MoviePill(usize),
    MovieSeason(usize),
    /// Index into the title's episodes.
    MovieEpisode(usize),
    MovieSimilar(usize),
    /// Row of the open side panel.
    MovieOption(usize),
    MoviePanelBack,
    PlayerToggle,
    PlayerPill(usize),
    PlayerOption(usize),
    PlayerAudioOption(usize),
    PlayerSubtitleOption(usize),
    PlayerRestart,
    PlayerSeek,
    PlayerChapter(usize),
    PlayerPreviousEpisode,
    PlayerNextEpisode,
    PlayerClose,
    PlayerPanelClose,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum SettingsLevel {
    Root,
    Section,
    WallpaperType,
    WallpaperPhotos,
    WallpaperVideos,
    WallpaperCategory(usize),
    HeadingFonts,
    TextFonts,
    UiResolution,
    /// Add Stack: choose a source.
    StackAdd,
    /// media X (media) device-code link.
    MoonlightHosts,
    MoonlightPin,
    /// Jellyfin server choice and Quick Connect code.
    JellyfinLink,
    /// plex.tv PIN sign-in.
    PlexLink,
    /// One stack's options.
    StackConfig(usize),
    StackCollection(usize),
    /// Content type of a stack (movies, series, …).
    StackShape(usize),
    /// Settings › Widgets and its pages.
    Widgets(crate::widgets::SettingsPage),
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}
impl Rect {
    pub const fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h }
    }
    pub fn contains(self, x: f32, y: f32) -> bool {
        x >= self.x && y >= self.y && x < self.x + self.w && y < self.y + self.h
    }
    pub fn center(self) -> (f32, f32) {
        (self.x + self.w / 2., self.y + self.h / 2.)
    }
    pub fn inset(self, n: f32) -> Self {
        Self::new(self.x + n, self.y + n, self.w - 2. * n, self.h - 2. * n)
    }
}

#[derive(Clone, Debug)]
pub struct Control {
    pub target: Target,
    pub rect: Rect,
    pub scrollable: bool,
}

#[derive(Clone, Debug)]
pub enum Overlay {
    Options { id: String, restore: Target },
    Keyboard,
    SystemKeyboard,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub enum Subpage {
    AppNames,
    Greeting,
    GreetingStyle,
    Clock,
    ClockFormat,
    TimeFormat,
    Input(String),
    WeatherLocation,
    Temperature,
    Language,
    SmartConnect,
    SmartSelect(usize),
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub enum TextField {
    Apps,
    Location,
    Name,
    InputName,
    HomeAddress,
    MoonlightAddress,
    /// Library page search.
    Library,
}

#[derive(Clone, Copy)]
pub enum Direction {
    Up,
    Down,
    Left,
    Right,
}

#[derive(Debug, PartialEq)]
pub enum Effect {
    Save,
    /// Read the app inventory again, keeping decoded icons of unchanged apps.
    Refresh,
    /// Refresh apps: read the inventory and every app icon again.
    Reload,
    Launch(String),
    Quit,
    Weather(weather::Command),
    HomeAssistant(crate::smart_home::Command),
    /// Start media device linking (Settings › Stacks › media X).
    /// Look for Jellyfin servers on the network (and the saved one).
    JellyfinDiscover,
    /// Start Quick Connect on this server.
    JellyfinConnect(String),
    /// Stop Quick Connect and the phone address form.
    JellyfinCancel,
    /// Serve the phone form for a server address.
    JellyfinAddress,
    /// Fetch the Jellyfin libraries offered as row contents.
    JellyfinViews,
    /// Get a plex.tv PIN and wait for approval.
    PlexLink,
    PlexCancel,
    /// Fetch the Plex libraries offered as row contents.
    PlexViews,
    MoonlightProbe(String),
    MoonlightDiscover(String),
    MoonlightPair(crate::moonlight::Host),
    MoonlightCancel,
    MoonlightWake,
    MoonlightPlay(i64),
    /// Close the running game on the PC (Apollo /cancel).
    MoonlightQuit,
    /// Rebuild every Games stack from the last Apollo list (no network).
    MoonlightRefilter,
    /// Look the game up on Steam for its card (id, title).
    GameInfo(i64, String),
    /// Songs and cover of a Jellyfin album card.
    AlbumDetails(i64),
    /// Play an album from this song, in order or shuffled.
    AlbumPlay(i64, usize, bool),
    /// Play the Steam trailer of the open game card in Luma's player.
    GameTrailer,
    /// Fetch cards for every configured stack.
    StacksRefresh,
    /// Fetch the account's bookmark folders and kino.pub collections.
    /// Play a media item (or one episode, quality, subtitles) in the TV player.
    MediaPlay(crate::media::PlayRequest),
    Player(crate::player::Command),
    /// Fetch everything the title card shows.
    MediaDetails(i64),
    /// Download episode stills: (item, [(episode index, url)]).
    MediaThumbs(i64, Vec<(usize, String)>),
    /// Play a plain stream URL (trailers) with this title.
    PlayUrl(String, String),
    /// Read root features, applying a change first when one is given.
    Root(Option<crate::platform::RootSet>),
}

/// Drag offset with resistance past either end (0..max), the iOS rubber band.
pub fn rubber(current: f32, movement: f32, max: f32) -> f32 {
    let next = current + movement;
    let outside = |v: f32| v < 0. || v > max;
    let step = if outside(current) || outside(next) {
        movement * 0.35
    } else {
        movement
    };
    (current + step).clamp(-120., max + 120.)
}

/// What an edge bounce moves: the Home page, the Favorites strip or a stack row.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BounceAxis {
    Page,
    Favorites,
    Stack(usize),
}
const BOUNCE_TIME: f32 = 0.7;

pub const KEYS: [&str; 40] = [
    "A", "B", "C", "D", "E", "F", "G", "H", "I", "J", "K", "L", "M", "N", "O", "P", "Q", "R", "S",
    "T", "U", "V", "W", "X", "Y", "Z", "0", "1", "2", "3", "4", "5", "6", "7", "8", "9", "Space",
    "Delete", "Clear", "Done",
];

pub struct FocusTween {
    pub from: f32,
    pub to: f32,
    pub start: Instant,
    pub duration: f32,
}
impl FocusTween {
    fn value(&self) -> f32 {
        self.from
            + (self.to - self.from)
                * motion::progress(self.start.elapsed().as_secs_f32(), self.duration)
    }
    fn running(&self) -> bool {
        self.start.elapsed().as_secs_f32() < self.duration
    }
}

pub struct State {
    /// Root features, `None` while unknown or when the TV has no root.
    pub root: Option<crate::platform::RootStatus>,
    /// Fetched cards per configured stack (same order as preferences.stacks).
    pub stack_items: Vec<Vec<crate::stacks::StackItem>>,
    /// Device code shown on the link page: (user code, site).
    /// Bookmark folders and collections of the linked account: (id, label).
    /// Last link or fetch problem, shown on the link page.
    /// Jellyfin servers found on the network or entered on the phone.
    pub jellyfin_servers: Vec<crate::jellyfin::Server>,
    /// Address Quick Connect runs against.
    pub jellyfin_selected: Option<String>,
    pub jellyfin_code: Option<String>,
    pub jellyfin_status: Option<String>,
    pub jellyfin_error: bool,
    /// Left-column QR: Quick Connect page of the server, or the phone address form.
    pub jellyfin_qr: Vec<Vec<bool>>,
    pub jellyfin_qr_caption: (String, String),
    /// The phone address form is open.
    pub jellyfin_phone: bool,
    /// Libraries of the signed-in user: ("view:<id>", name).
    pub jellyfin_views: Vec<(String, String)>,
    pub plex_code: Option<String>,
    pub plex_status: Option<String>,
    pub plex_error: bool,
    /// Plex sign-in page with the PIN filled in.
    pub plex_qr: Vec<Vec<bool>>,
    /// Libraries of the Plex server: ("section:<key>", title).
    pub plex_views: Vec<(String, String)>,
    /// Typed media X site before it is saved.
    pub moonlight_hosts: Vec<crate::moonlight::Host>,
    pub moonlight_retry_host: Option<crate::moonlight::Host>,
    pub moonlight_pair_error: bool,
    pub moonlight_discovery_start: Option<Instant>,
    pub moonlight_pin: Option<String>,
    pub moonlight_status: Option<String>,
    pub moonlight_ip_draft: String,
    pub moonlight_online: bool,
    pub moonlight_games: Vec<crate::moonlight::Game>,
    /// The whole Apollo list; Games stacks pick from it (`moonlight_refilter`).
    pub moonlight_items: Vec<crate::stacks::StackItem>,
    /// Steam store details per Apollo app id, filled in the background.
    pub game_info: std::collections::HashMap<i64, crate::steam::GameInfo>,
    pub moonlight_pending: Option<i64>,
    /// Luma started a Moonlight stream; on return ask Apollo whether it still runs.
    pub moonlight_launched: bool,
    pub game: Option<GameCard>,
    /// The game card fading out after Close/Back, as on the movie card.
    pub game_closing: Option<(Box<GameCard>, Instant)>,
    pub album: Option<AlbumCard>,
    pub album_closing: Option<(Box<AlbumCard>, Instant)>,
    /// Background music and its queue.
    pub music: crate::music::Engine,
    /// The Library page opened from a row's View Library card.
    pub library: Option<crate::library::Library>,
    pub library_closing: Option<(Box<crate::library::Library>, Instant)>,
    pub page: Page,
    pub focus: Target,
    pub inventory: Inventory,
    pub preferences: Preferences,
    pub accent_transition: Option<(u32, u32, Instant)>,
    pub query: String,
    pub filter: usize,
    pub section: usize,
    pub subpage: Option<Subpage>,
    pub settings_level: SettingsLevel,
    pub rendered_settings_level: SettingsLevel,
    pub rendered_preview_focus: Target,
    pub settings_forward: bool,
    pub scroll: f32,
    pub home_scroll: f32,
    pub scroll_goal: f32,
    pub home_scroll_goal: f32,
    /// Horizontal offset of the focused stack row (row, current, goal).
    pub stack_scroll: (usize, f32, f32),
    /// A row the focus just left glides back to its first card: row, offset, start.
    pub stack_return: Option<(usize, f32, Instant)>,
    /// Rubber-band kick at a scroll edge: axis, signed size, start.
    pub bounce: Option<(BounceAxis, f32, Instant)>,
    pub scroll_last: Instant,
    pub focus_tweens: HashMap<Target, FocusTween>,
    pub rendered_page: Page,
    pub rendered_section: usize,
    pub rendered_subpage: Option<Subpage>,
    pub previous_page_layer: Option<(Vec<Draw>, Instant, f32)>,
    pub previous_panel_layer: Option<(Vec<Draw>, Instant, bool)>,
    pub previous_sidebar_layer: Option<(Vec<Draw>, Instant)>,
    /// Preview identities for the crossfade (0: static preview).
    pub last_sidebar_key: u64,
    pub previous_sidebar_key: u64,
    /// Settings shows an animated illustration that needs every frame.
    pub settings_art_live: bool,
    pub previous_overlay_layer: Option<(Vec<Draw>, Instant)>,
    pub last_body: Vec<Draw>,
    pub last_panel: Vec<Draw>,
    pub last_sidebar: Vec<Draw>,
    pub last_overlay: Vec<Draw>,
    pub first_home: bool,
    pub overlay: Option<Overlay>,
    pub moving: Option<String>,
    pub loading: bool,
    /// Set by Refresh apps: the next inventory also reloads every app icon.
    pub reload_artwork: bool,
    pub launching: bool,
    /// media details already fetched (by a card or by focus prefetch), so
    /// a title card opens complete instead of waiting on the network.
    pub movie_cache: HashMap<i64, crate::media::Detail>,
    /// Clean wallpaper mode (OK on the video caption): whether the interface
    /// is going away (true) or coming back, and when that move starts.
    pub ambient: Option<(bool, Instant)>,
    /// Last real remote, pointer or keyboard input, for the screensaver.
    pub last_input: Instant,
    /// Stack card under focus and since when; ids already prefetched.
    pub movie_prefetch: (Option<(Target, Instant)>, std::collections::HashSet<i64>),
    /// The prefetch still downloading, so they go one at a time and never
    /// queue up in front of a title that was actually opened.
    pub movie_prefetch_busy: Option<(i64, Instant)>,
    /// media item playing in the web player: its card refreshes watched
    /// marks and progress when Luma comes back to the foreground.
    pub web_player_item: Option<i64>,
    pub notice: Option<(String, Instant)>,
    pub error: Option<String>,
    pub dirty: bool,
    pub pointer: bool,
    pub pressed: Option<Target>,
    pub focus_started: Instant,
    pub page_started: Instant,
    pub kinetic_velocity: f32,
    pub kinetic_home: bool,
    pub kinetic_max: f32,
    pub kinetic_last: Instant,
    pub bitcoin_usd: Option<String>,
    pub video_active: bool,
    pub video_frame_ready: bool,
    /// Start of the short still-to-live handoff after the hardware plane has a frame.
    pub video_transition_started: Option<Instant>,
    pub video_title: Option<String>,
    pub wallpaper_gallery_video: bool,
    pub video_filter: usize,
    pub video_filter_changed_at: Instant,
    pub video_filter_items: Vec<Vec<usize>>,
    pub video_group_items: Vec<Vec<(String, String, Vec<usize>)>>,
    pub video_selection_version: Cell<u64>,
    pub video_selected_count_cache: Cell<usize>,
    pub photo_background_backup: String,
    pub previous_background: Option<(String, Instant)>,
    pub panel_started: Option<Instant>,
    pub weather: weather::Weather,
    pub rude_weather: weather_phrases::Engine,
    pub keyboard_field: TextField,
    pub greeting_name_draft: String,
    pub input_name: String,
    pub ha_online: bool,
    pub ha_entities: Vec<crate::smart_home::Entity>,
    pub ha_servers: Vec<crate::smart_home::DiscoveredServer>,
    pub ha_snapshots: HashMap<String, String>,
    /// Cameras whose last snapshot request failed.
    pub ha_snapshot_failed: HashSet<String>,
    /// When the full-screen camera opened, to stop waiting after a while.
    pub ha_camera_opened: Instant,
    pub ha_address_draft: String,
    pub ha_qr: Vec<Vec<bool>>,
    pub ha_fallback_qr: Vec<Vec<bool>>,
    pub ha_auth_status: String,
    pub ha_discovery_since: Instant,
    pub ha_detail: Option<String>,
    /// The camera whose long-press menu (Hide/Show camera) is open.
    pub ha_camera_menu: Option<String>,
    /// The scene just run from the tab and when, for a short confirmation.
    pub ha_scene_fired: Option<(String, Instant)>,
    /// Open title card, and one fading out after closing.
    pub movie: Option<MovieCard>,
    pub playback: Option<crate::player::Session>,
    pub movie_closing: Option<(Box<MovieCard>, Instant)>,
    pub ha_camera: Option<String>,
    pub ha_signout_armed: bool,
    pub ha_token_qr: bool,
    pub widget_data: crate::widgets::Data,
    /// Full-screen page opened from a widget.
    pub widget_detail: Option<crate::widgets::Detail>,
    pub widget_detail_started: Option<Instant>,
    /// Home widget edit mode (hold OK on a widget).
    pub widget_edit: Option<crate::widgets::Edit>,
    /// Settings › Widgets › Add: chosen type and size.
    pub widget_add: (crate::widgets::Kind, crate::widgets::Size),
    /// Where focus returns when a page opened from a widget closes.
    pub widget_return: Option<Target>,
}

impl State {
    pub fn new(mut preferences: Preferences) -> Self {
        preferences.accent.normalized();
        if preferences.smart_home.selection_version < 1 {
            // 2.2.113 treated an empty selection as "every entity"; start over
            // from the recommended set instead of a hundred maintenance switches.
            let home = &mut preferences.smart_home;
            home.selected_entities.clear();
            home.selected_cameras.clear();
            home.selected_scenes.clear();
            home.selection_version = 1;
        }
        let video_gallery = preferences.background == "video";
        let photo_backup = if video_gallery {
            "yosemite".into()
        } else {
            preferences.background.clone()
        };
        let video_filter_items = (0..crate::video_catalog::FILTERS.len())
            .map(crate::video_catalog::display_filter)
            .collect();
        let video_group_items = (0..crate::video_catalog::FILTERS.len())
            .map(crate::video_catalog::groups)
            .collect();
        let video_selection_version =
            crate::video_catalog::selection_version(&preferences.video_excluded);
        let video_selected_count_cache =
            crate::video_catalog::selected_count(&preferences.video_excluded);
        Self {
            page: Page::Home,
            focus: Target::Nav(0),
            inventory: Inventory::default(),
            preferences,
            accent_transition: None,
            stack_items: vec![],
            jellyfin_servers: vec![],
            jellyfin_selected: None,
            jellyfin_code: None,
            jellyfin_status: None,
            jellyfin_error: false,
            jellyfin_qr: vec![],
            jellyfin_qr_caption: (String::new(), String::new()),
            jellyfin_phone: false,
            jellyfin_views: vec![],
            plex_code: None,
            plex_status: None,
            plex_error: false,
            plex_qr: vec![],
            plex_views: vec![],
            moonlight_hosts: Vec::new(),
            moonlight_retry_host: None,
            moonlight_pair_error: false,
            moonlight_discovery_start: None,
            moonlight_pin: None,
            moonlight_status: None,
            moonlight_ip_draft: String::new(),
            moonlight_online: false,
            moonlight_games: Vec::new(),
            moonlight_items: Vec::new(),
            game_info: Default::default(),
            moonlight_pending: None,
            moonlight_launched: false,
            game: None,
            game_closing: None,
            album: None,
            album_closing: None,
            music: Default::default(),
            library: None,
            library_closing: None,
            query: String::new(),
            filter: 0,
            section: 0,
            subpage: None,
            settings_level: SettingsLevel::Root,
            rendered_settings_level: SettingsLevel::Root,
            rendered_preview_focus: Target::Nav(0),
            settings_forward: true,
            scroll: 0.,
            home_scroll: 0.,
            scroll_goal: 0.,
            home_scroll_goal: 0.,
            stack_scroll: (0, 0., 0.),
            stack_return: None,
            bounce: None,
            scroll_last: Instant::now(),
            focus_tweens: HashMap::new(),
            rendered_page: Page::Home,
            rendered_section: 0,
            rendered_subpage: None,
            previous_page_layer: None,
            previous_panel_layer: None,
            previous_sidebar_layer: None,
            last_sidebar_key: 0,
            previous_sidebar_key: 0,
            settings_art_live: false,
            previous_overlay_layer: None,
            last_body: Vec::new(),
            last_panel: Vec::new(),
            last_sidebar: Vec::new(),
            last_overlay: Vec::new(),
            first_home: true,
            overlay: None,
            moving: None,
            loading: false,
            reload_artwork: false,
            launching: false,
            movie_cache: HashMap::new(),
            ambient: None,
            last_input: Instant::now(),
            movie_prefetch_busy: None,
            movie_prefetch: (None, std::collections::HashSet::new()),
            web_player_item: None,
            notice: None,
            error: None,
            dirty: true,
            pointer: false,
            pressed: None,
            focus_started: Instant::now(),
            page_started: Instant::now(),
            kinetic_velocity: 0.,
            kinetic_home: false,
            kinetic_max: 0.,
            kinetic_last: Instant::now(),
            bitcoin_usd: None,
            video_active: false,
            video_frame_ready: false,
            video_transition_started: None,
            video_title: None,
            wallpaper_gallery_video: video_gallery,
            video_filter: 0,
            video_filter_changed_at: Instant::now(),
            video_filter_items,
            video_group_items,
            video_selection_version: Cell::new(video_selection_version),
            video_selected_count_cache: Cell::new(video_selected_count_cache),
            photo_background_backup: photo_backup,
            previous_background: None,
            panel_started: None,
            weather: weather::Weather::default(),
            rude_weather: weather_phrases::Engine::default(),
            keyboard_field: TextField::Apps,
            greeting_name_draft: String::new(),
            input_name: String::new(),
            ha_online: false,
            ha_entities: Vec::new(),
            ha_servers: Vec::new(),
            ha_snapshots: HashMap::new(),
            ha_snapshot_failed: HashSet::new(),
            ha_camera_opened: Instant::now(),
            ha_address_draft: String::new(),
            ha_qr: Vec::new(),
            ha_fallback_qr: Vec::new(),
            ha_auth_status: String::new(),
            ha_discovery_since: Instant::now(),
            ha_detail: None,
            ha_camera_menu: None,
            ha_scene_fired: None,
            movie: None,
            playback: None,
            root: None,
            movie_closing: None,
            ha_camera: None,
            ha_signout_armed: false,
            ha_token_qr: false,
            widget_data: Default::default(),
            widget_detail: None,
            widget_detail_started: None,
            widget_edit: None,
            widget_add: (crate::widgets::Kind::Weather, crate::widgets::Size::S),
            widget_return: None,
        }
    }

    pub fn video_selected_count(&self) -> usize {
        let version = crate::video_catalog::selection_version(&self.preferences.video_excluded);
        if self.video_selection_version.get() != version {
            self.video_selection_version.set(version);
            self.video_selected_count_cache
                .set(crate::video_catalog::selected_count(
                    &self.preferences.video_excluded,
                ));
        }
        self.video_selected_count_cache.get()
    }

    pub fn video_items_for_filter(&self, filter: usize) -> &[usize] {
        self.video_filter_items
            .get(filter)
            .map_or(&[], Vec::as_slice)
    }

    pub fn video_groups_for_filter(&self, filter: usize) -> &[(String, String, Vec<usize>)] {
        self.video_group_items
            .get(filter)
            .map_or(&[], Vec::as_slice)
    }

    /// Entities a Settings picker lists, grouped by room (rooms A–Z, unassigned last).
    /// Entities Home Assistant itself hides or marks diagnostic/config never appear.
    pub fn ha_listed(&self, section: usize) -> Vec<&crate::smart_home::Entity> {
        let mut entities: Vec<_> = self
            .ha_entities
            .iter()
            .filter(|e| !e.hidden && Self::ha_in_section(e, section))
            .collect();
        entities.sort_by(|a, b| {
            (a.area.is_empty(), &a.area, &a.name).cmp(&(b.area.is_empty(), &b.area, &b.name))
        });
        entities
    }
    fn ha_in_section(e: &crate::smart_home::Entity, section: usize) -> bool {
        match section {
            0 => !matches!(e.domain.as_str(), "camera" | "scene" | "script"),
            1 => e.domain == "camera",
            _ => matches!(e.domain.as_str(), "scene" | "script"),
        }
    }
    fn ha_selection(&self, section: usize) -> &Vec<String> {
        match section {
            0 => &self.preferences.smart_home.selected_entities,
            1 => &self.preferences.smart_home.selected_cameras,
            _ => &self.preferences.smart_home.selected_scenes,
        }
    }
    /// An empty selection means the recommended set (`Entity::useful`).
    pub fn ha_selected(&self, section: usize, e: &crate::smart_home::Entity) -> bool {
        let selection = self.ha_selection(section);
        if selection.is_empty() {
            e.useful()
        } else {
            selection.contains(&e.id)
        }
    }
    pub fn ha_selection_is_default(&self, section: usize) -> bool {
        self.ha_selection(section).is_empty()
    }
    /// What the Smart Home tab shows, in picker order.
    pub fn ha_camera_hidden(&self, id: &str) -> bool {
        self.preferences
            .smart_home
            .hidden_cameras
            .iter()
            .any(|camera| camera == id)
    }
    pub fn ha_tab(&self, section: usize) -> Vec<&crate::smart_home::Entity> {
        let shown: Vec<_> = self
            .ha_listed(section)
            .into_iter()
            .filter(|e| self.ha_selected(section, e))
            .collect();
        // A camera's second stream is the same picture: show the main one only.
        let ids: std::collections::HashSet<&str> = shown.iter().map(|e| e.id.as_str()).collect();
        shown
            .iter()
            .copied()
            .filter(|e| {
                crate::smart_home::stream_twin(&e.id)
                    .is_none_or(|twin| !ids.contains(twin.as_str()))
            })
            .collect()
    }
    pub fn ha_selection_entries(&self, section: usize) -> Vec<String> {
        self.ha_listed(section)
            .into_iter()
            .map(|e| e.id.clone())
            .collect()
    }
    pub fn accept_ha(&mut self, update: crate::smart_home::Update) {
        match update {
            crate::smart_home::Update::States(entities) => self.ha_entities = entities,
            crate::smart_home::Update::Connected => self.ha_online = true,
            crate::smart_home::Update::QrReady {
                authorize,
                fallback,
            } => {
                self.ha_qr = authorize
                    .as_deref()
                    .map(crate::smart_home::qr_matrix)
                    .unwrap_or_default();
                self.ha_fallback_qr = crate::smart_home::qr_matrix(&fallback);
                self.ha_auth_status = if self.ha_qr.is_empty() {
                    "Choose a server or use the phone setup QR".into()
                } else {
                    "Waiting for sign-in…".into()
                };
            }
            crate::smart_home::Update::QrExpired => {
                self.ha_auth_status = "QR expired. Select Show new QR.".into();
                self.ha_qr.clear();
                self.ha_fallback_qr.clear();
            }
            crate::smart_home::Update::PairingError(reason) => {
                self.ha_auth_status = reason;
            }
            crate::smart_home::Update::Authorized { server } => {
                self.preferences.smart_home.server_url = server;
                self.preferences.smart_home.show_tab = true;
                self.ha_auth_status = "Connected".into();
                self.ha_qr.clear();
                self.ha_fallback_qr.clear();
                self.subpage = None;
                self.focus = Target::SmartShowTab;
                self.ha_online = true;
            }
            crate::smart_home::Update::Discovered(server) => {
                if !self.ha_servers.iter().any(|item| item.url == server.url) {
                    self.ha_servers.push(server);
                }
            }
            crate::smart_home::Update::Snapshot { id, path } => {
                self.ha_snapshot_failed.remove(&id);
                // A hidden camera is blurred on its tile, sharp full screen; a
                // frame fetched before that switched is dropped.
                let blur = self.ha_camera_hidden(&id) && self.ha_camera.as_ref() != Some(&id);
                if path.contains("-blur-") == blur {
                    self.ha_snapshots.insert(id, path);
                }
            }
            crate::smart_home::Update::SnapshotFailed { id } => {
                self.ha_snapshot_failed.insert(id);
            }

            crate::smart_home::Update::Offline => self.ha_online = false,
            crate::smart_home::Update::Error(reason) => self.notify(reason),
            crate::smart_home::Update::SignedOut => {
                self.ha_online = false;
                self.ha_entities.clear();
            }
        }
        self.dirty = true;
    }

    /// Signed in while the link page is open: show the first new stack.
    pub fn server_linked(&mut self, i: usize) {
        self.settings_level = SettingsLevel::Section;
        self.settings_push(SettingsLevel::StackConfig(i), Target::StackOpt(0));
    }

    pub fn jellyfin_reset(&mut self) {
        self.jellyfin_servers.clear();
        self.jellyfin_selected = None;
        self.jellyfin_code = None;
        self.jellyfin_status = None;
        self.jellyfin_error = false;
        self.jellyfin_qr.clear();
        self.jellyfin_qr_caption = (String::new(), String::new());
        self.jellyfin_phone = false;
    }

    /// Start Quick Connect on a server; the QR opens its Quick Connect page.
    pub fn jellyfin_connect(&mut self, address: String) -> Vec<Effect> {
        self.jellyfin_selected = Some(address.clone());
        self.jellyfin_code = None;
        self.jellyfin_error = false;
        self.jellyfin_phone = false;
        self.jellyfin_qr = crate::phone_link::qr_matrix(&format!("{address}/web/#/quickconnect"));
        self.jellyfin_qr_caption = (
            "Scan with your phone".into(),
            "Opens Quick Connect on your Jellyfin server".into(),
        );
        self.jellyfin_status = Some("Getting a code…".into());
        self.dirty = true;
        vec![Effect::JellyfinConnect(address)]
    }

    pub fn notify(&mut self, text: impl Into<String>) {
        self.notice = Some((text.into(), Instant::now()));
        self.dirty = true;
    }

    pub fn focus_level(&self, target: &Target) -> f32 {
        if self.preferences.reduced_motion {
            return if &self.focus == target { 1. } else { 0. };
        }
        self.focus_tweens.get(target).map_or(
            if &self.focus == target { 1. } else { 0. },
            FocusTween::value,
        )
    }
    /// Focus-in spring for card zoom (overshoots ~20%, settles at 1) and the
    /// progress of a one-off diagonal light sweep.
    pub fn focus_pop(&self, target: &Target) -> (f32, Option<f32>) {
        let level = self.focus_level(target);
        if self.preferences.reduced_motion || &self.focus != target {
            return (level, None);
        }
        let elapsed = self.focus_started.elapsed().as_secs_f32();
        let k = (elapsed / motion::FOCUS_POP).min(1.);
        let spring = 1. - (-5. * k).exp() * (10. * k).cos();
        let sweep = (elapsed - 0.04) / motion::FOCUS_SHEEN;
        (spring.max(0.), (sweep > 0. && sweep < 1.).then_some(sweep))
    }
    pub fn change_focus(&mut self, target: Target) {
        if self.focus == target {
            return;
        }
        if target != Target::SmartSignOut {
            self.ha_signout_armed = false;
        }
        // Apps category tabs follow the remote like the top navigation: moving
        // onto a tab selects it, no OK needed. Hover alone does not.
        if let Target::Filter(index) = target
            && !self.pointer
            && self.page == Page::Apps
            && self.filter != index
        {
            self.filter = index;
            self.scroll = 0.;
        }

        if self.preferences.reduced_motion {
            self.focus_tweens.clear();
            self.focus = target;
            self.dirty = true;
            return;
        }
        let old = self.focus.clone();
        let old_value = self.focus_level(&old);
        let new_value = self.focus_level(&target);
        let now = Instant::now();
        self.focus_tweens.insert(
            old,
            FocusTween {
                from: old_value,
                to: 0.,
                start: now,
                duration: motion::FOCUS_OUT,
            },
        );
        self.focus_tweens.insert(
            target.clone(),
            FocusTween {
                from: new_value,
                to: 1.,
                start: now,
                duration: motion::FOCUS_IN,
            },
        );
        self.focus = target;
        self.focus_started = now;
        self.dirty = true;
    }

    pub fn start_kinetic(&mut self, velocity: f32, home: bool, max: f32) {
        self.scroll_goal = self.scroll;
        self.home_scroll_goal = self.home_scroll;
        self.kinetic_velocity = velocity.clamp(-3600., 3600.);
        self.kinetic_home = home;
        self.kinetic_max = max.max(0.);
        self.kinetic_last = Instant::now();
        self.dirty = true;
    }
    pub fn stop_kinetic(&mut self) {
        self.kinetic_velocity = 0.;
    }
    /// Close the game card: focus goes back to its Home card, which has not
    /// moved, and the card fades out over it.
    pub fn close_game(&mut self) {
        if let Some(g) = self.game.take() {
            self.focus = g.restore.clone();
            self.game_closing = Some((Box::new(g), Instant::now()));
            self.dirty = true;
        }
    }
    pub fn game_tick(&mut self) {
        if self
            .game_closing
            .as_ref()
            .is_some_and(|(_, t)| t.elapsed().as_secs_f32() >= movie::CLOSE_TIME)
        {
            self.game_closing = None;
            self.dirty = true;
        }
        if self.game_closing.is_some() && !self.preferences.reduced_motion {
            self.dirty = true;
        }
        let Some(g) = self.game.as_mut() else {
            return;
        };
        let info = self.game_info.get(&g.id);
        let now = Instant::now();
        if g.shown.is_none()
            && (info.is_some() || g.opened.elapsed().as_secs_f32() >= game::GAME_WAIT)
        {
            g.shown = Some(now);
            self.dirty = true;
        }
        if g.backdrop_at.is_none() && info.is_some_and(|i| i.steam_id > 0 && i.hero.is_some()) {
            g.backdrop_at = Some(now);
            self.dirty = true;
        }
        // A frame is drawn only when dirty: keep drawing through the entrance.
        if self.game_animating() {
            self.dirty = true;
        }
    }
    pub fn game_animating(&self) -> bool {
        if self.preferences.reduced_motion {
            return false;
        }
        self.game.as_ref().is_some_and(|g| {
            g.opened.elapsed().as_secs_f32() < movie::OPEN_TIME
                || g.shown
                    .is_none_or(|t| t.elapsed().as_secs_f32() < movie::REVEAL_TIME)
                || g.backdrop_at
                    .is_some_and(|t| t.elapsed().as_secs_f32() < movie::BACKDROP_TIME)
        })
    }
    pub fn tick(&mut self) {
        self.screensaver_tick();
        if self.ambient_animating() {
            self.dirty = true;
        }
        self.movie_tick();
        self.game_tick();
        self.album_tick();
        self.library_tick();
        if self.settings_level == SettingsLevel::MoonlightHosts
            && self.moonlight_hosts.is_empty()
            && self
                .moonlight_discovery_start
                .is_some_and(|at| at.elapsed() > Duration::from_secs(8))
            && self.moonlight_status.as_deref() == Some("Searching for PCs…")
        {
            self.moonlight_status = Some(
                "No PCs found. Check Sunshine and make sure your PC is on the same network.".into(),
            );
            self.dirty = true;
        }
        if !self.preferences.reduced_motion && self.kinetic_velocity.abs() >= 3. {
            let dt = self.kinetic_last.elapsed().as_secs_f32().min(0.05);
            self.kinetic_last = Instant::now();
            if dt > 0. {
                let offset = self.kinetic_velocity * dt;
                let position = if self.kinetic_home {
                    &mut self.home_scroll
                } else {
                    &mut self.scroll
                };
                let candidate = *position + offset;
                if candidate < 0. || candidate > self.kinetic_max {
                    *position = candidate.clamp(0., self.kinetic_max);
                    self.kinetic_velocity *= -0.17;
                } else {
                    *position = candidate;
                }
                self.kinetic_velocity *= 0.92_f32.powf(dt * 60.);
                self.dirty = true;
            }
        } else {
            if self.kinetic_home {
                self.home_scroll_goal =
                    Self::snap_home_scroll(self.home_scroll, self.home_scroll_max());
                self.kinetic_home = false;
            }
            self.kinetic_velocity = 0.;
        }
        let dt_scroll = self.scroll_last.elapsed().as_secs_f32().min(0.05);
        self.scroll_last = Instant::now();
        let dt_settings = if self.page == Page::Settings {
            dt_scroll * (0.180 / 0.220)
        } else {
            dt_scroll
        };
        if self.kinetic_velocity.abs() < 3. {
            let next = motion::chase(
                self.scroll,
                self.scroll_goal,
                dt_settings,
                self.preferences.reduced_motion,
            );
            let home = motion::chase(
                self.home_scroll,
                self.home_scroll_goal,
                dt_scroll,
                self.preferences.reduced_motion,
            );
            let row = motion::chase(
                self.stack_scroll.1,
                self.stack_scroll.2,
                dt_scroll,
                self.preferences.reduced_motion,
            );
            if row != self.stack_scroll.1 {
                self.stack_scroll.1 = row;
                self.dirty = true;
            }
            if let Some((_, _, start)) = self.stack_return {
                self.dirty = true;
                if start.elapsed().as_secs_f32() > STACK_RETURN_TIME {
                    self.stack_return = None;
                }
            }
            if next != self.scroll || home != self.home_scroll {
                self.dirty = true;
            }
            self.scroll = next;
            self.home_scroll = home;
        }
        self.focus_tweens.retain(|_, a| a.running());
        if self.page_started.elapsed() < Duration::from_secs_f32(self.page_entrance_duration())
            && !self.preferences.reduced_motion
        {
            self.dirty = true;
        }
        if !self.focus_tweens.is_empty() && !self.preferences.reduced_motion {
            self.dirty = true;
        }
        if let Some(start) = self.panel_started {
            self.dirty = true;
            let duration = if self.overlay.is_some() {
                motion::PANEL_IN
            } else if self.page == Page::Settings {
                motion::ROW_CASCADE_END
            } else {
                motion::PANEL_IN
            };
            if self.preferences.reduced_motion || start.elapsed().as_secs_f32() >= duration {
                self.panel_started = None;
            }
        }
        if !self.preferences.reduced_motion
            && self.notice.as_ref().is_some_and(|(_, start)| {
                let age = start.elapsed().as_secs_f32();
                age < motion::PANEL_IN || (5. ..5. + motion::PANEL_OUT).contains(&age)
            })
        {
            self.dirty = true;
        }
        if let Some((_, start)) = &self.previous_background {
            self.dirty = true;
            if self.preferences.reduced_motion || start.elapsed() >= Duration::from_millis(400) {
                self.previous_background = None;
            }
        }
        if let Some(start) = self.video_transition_started {
            self.dirty = true;
            if self.preferences.reduced_motion || start.elapsed() >= VIDEO_HANDOFF {
                self.video_transition_started = None;
            }
        }
        for expired in [
            self.previous_page_layer
                .as_ref()
                .is_some_and(|(_, s, _)| s.elapsed().as_secs_f32() >= motion::PAGE_OUT),
            self.previous_panel_layer
                .as_ref()
                .is_some_and(|(_, s, _)| s.elapsed().as_secs_f32() >= motion::PANEL_OUT),
            self.previous_sidebar_layer
                .as_ref()
                .is_some_and(|(_, s)| s.elapsed().as_secs_f32() >= motion::PANEL_OUT),
            self.previous_overlay_layer
                .as_ref()
                .is_some_and(|(_, s)| s.elapsed().as_secs_f32() >= motion::PANEL_OUT),
        ] {
            if expired {
                self.dirty = true;
            }
        }
        if self
            .previous_page_layer
            .as_ref()
            .is_some_and(|(_, s, _)| s.elapsed().as_secs_f32() >= motion::PAGE_OUT)
        {
            self.previous_page_layer = None;
        }
        if self
            .previous_panel_layer
            .as_ref()
            .is_some_and(|(_, s, _)| s.elapsed().as_secs_f32() >= motion::PANEL_OUT)
        {
            self.previous_panel_layer = None;
        }
        if self
            .previous_sidebar_layer
            .as_ref()
            .is_some_and(|(_, s)| s.elapsed().as_secs_f32() >= motion::PANEL_OUT)
        {
            self.previous_sidebar_layer = None;
        }
        if self
            .previous_overlay_layer
            .as_ref()
            .is_some_and(|(_, s)| s.elapsed().as_secs_f32() >= motion::PANEL_OUT)
        {
            self.previous_overlay_layer = None;
        }
        if self.notice.as_ref().is_some_and(|(_, start)| {
            start.elapsed() > Duration::from_secs(5) + Duration::from_secs_f32(motion::PANEL_OUT)
        }) {
            self.notice = None;
            self.dirty = true;
        }
    }

    pub fn show_smart_tab(&self) -> bool {
        self.preferences.smart_home.show_tab
            && !self.preferences.smart_home.server_url.trim().is_empty()
    }

    pub fn settings_nav_index(&self) -> usize {
        if self.show_smart_tab() { 3 } else { 2 }
    }

    pub fn page_index(&self) -> usize {
        match self.page {
            Page::Home => 0,
            Page::Apps => 1,
            Page::SmartHome => 2,
            Page::Settings => self.settings_nav_index(),
        }
    }

    pub fn open_settings(&mut self) {
        self.open(self.settings_nav_index());
    }

    pub fn page_entrance_duration(&self) -> f32 {
        if self.first_home && self.page == Page::Home {
            motion::FIRST_HOME_END
        } else if self.page == Page::Settings {
            motion::ROW_CASCADE_END
        } else {
            motion::CARD_CASCADE_END
        }
    }

    pub fn accent_colors(&self) -> (u32, u32) {
        let (a, b) = self.preferences.accent.colors();
        let Some((old_a, old_b, started)) = self.accent_transition else {
            return (a, b);
        };
        let t = crate::motion::progress(started.elapsed().as_secs_f32(), 0.200);
        let mix = |old: u32, new: u32| {
            let channels = (0..3).map(|n| {
                let shift = n * 8;
                let x = ((old >> shift) & 0xff) as f32;
                let y = ((new >> shift) & 0xff) as f32;
                (((x + (y - x) * t).round() as u32) & 0xff) << shift
            });
            channels.fold(0u32, |acc, next| acc | next)
        };
        (mix(old_a, a), mix(old_b, b))
    }
    pub fn animating(&self) -> bool {
        self.accent_transition
            .as_ref()
            .is_some_and(|(_, _, start)| start.elapsed().as_secs_f32() < 0.2)
            || self.previous_background.is_some()
            || self.panel_started.is_some()
            || (self
                .ha_camera
                .as_ref()
                .is_some_and(|id| !self.ha_snapshots.contains_key(id))
                && self.ha_camera_opened.elapsed().as_secs_f32() < CAMERA_WAIT + 0.5)
            || self
                .ha_scene_fired
                .as_ref()
                .is_some_and(|(_, at)| at.elapsed().as_secs_f32() < SCENE_FIRED)
            || self.previous_page_layer.is_some()
            || self.previous_panel_layer.is_some()
            || self.previous_sidebar_layer.is_some()
            || self.previous_overlay_layer.is_some()
            || (self.scroll - self.scroll_goal).abs() > 0.35
            || (self.home_scroll - self.home_scroll_goal).abs() > 0.35
            || (self.stack_scroll.1 - self.stack_scroll.2).abs() > 0.35
            || self.stack_return.is_some()
            || self.movie_animating()
            || self.game_animating()
            || self.album_animating()
            || self.album_closing.is_some()
            || self.library.as_ref().is_some_and(|l| l.animating())
            || self.library_closing.is_some()
            || self
                .bounce
                .is_some_and(|(_, _, start)| start.elapsed().as_secs_f32() < BOUNCE_TIME)
            || (!self.preferences.reduced_motion && !self.focus_tweens.is_empty())
            || self.kinetic_velocity.abs() >= 3.
            || (!self.preferences.reduced_motion
                && self.page_started.elapsed()
                    < Duration::from_secs_f32(self.page_entrance_duration()))
            || (!self.preferences.reduced_motion
                && self.focus_started.elapsed()
                    < Duration::from_secs_f32(motion::FOCUS_SHEEN + 0.1))
            || (!self.preferences.reduced_motion
                && self.notice.as_ref().is_some_and(|(_, start)| {
                    let age = start.elapsed().as_secs_f32();
                    age < motion::PANEL_IN || (5. ..5. + motion::PANEL_OUT).contains(&age)
                }))
    }

    pub fn open(&mut self, index: usize) {
        self.weather.cancel_search();
        if index == self.page_index() {
            return;
        }
        self.first_home = false;
        self.page = match index {
            1 => Page::Apps,
            2 if self.show_smart_tab() => Page::SmartHome,
            2 | 3 => Page::Settings,
            _ => Page::Home,
        };
        self.change_focus(Target::Nav(self.page_index()));
        self.page_started = Instant::now();
        self.focus_started = Instant::now();
        self.stop_kinetic();
        self.scroll = 0.;
        self.scroll_goal = 0.;
        if self.page == Page::Home {
            self.scroll_favorites(0.);
            self.home_scroll_goal =
                Self::snap_home_scroll(self.home_scroll, self.home_scroll_max());
        }
        self.query.clear();
        self.filter = 0;
        self.overlay = None;
        self.moving = None;
        self.subpage = None;
        self.settings_level = SettingsLevel::Root;
        if self.page == Page::Settings {
            // Settings always opens on General; the row lights up only on Down.
            self.section = 0;
        }
        self.panel_started = None;
        self.dirty = true;
    }

    pub fn apps(&self) -> Vec<&App> {
        let ids = self.preferences.favorites(&self.inventory.apps);
        if self.page == Page::Home {
            return ids
                .iter()
                .filter(|id| !self.preferences.hidden.contains(id))
                .filter_map(|id| self.inventory.apps.iter().find(|app| &app.id == id))
                .collect();
        }
        let query = self.query.to_lowercase();
        let mut apps: Vec<_> = self
            .inventory
            .apps
            .iter()
            .filter(|app| {
                self.preferences.hidden.contains(&app.id) == (self.filter == 5)
                    && (self.filter == 0
                        || self.filter == 5
                        || app.category == CATEGORIES[self.filter - 1])
                    && app.title.to_lowercase().contains(&query)
            })
            .collect();
        apps.sort_by(|a, b| {
            let rank = |app: &App| {
                ids.iter()
                    .position(|id| id == &app.id)
                    .unwrap_or(usize::MAX)
            };
            rank(a)
                .cmp(&rank(b))
                .then_with(|| a.title.to_lowercase().cmp(&b.title.to_lowercase()))
        });
        apps
    }

    pub fn home_scroll_max(&self) -> f32 {
        let count = self
            .preferences
            .favorites(&self.inventory.apps)
            .iter()
            .filter(|id| {
                !self.preferences.hidden.contains(id)
                    && self.inventory.apps.iter().any(|app| &app.id == *id)
            })
            .count();
        let content = if count == 0 {
            0.
        } else {
            (count - 1) as f32 * HOME_TILE_PITCH + HOME_TILE_WIDTH
        };
        let max = (content - (HOME_STRIP_RIGHT - HOME_STRIP_LEFT)).max(0.);
        (max / HOME_TILE_PITCH).round() * HOME_TILE_PITCH
    }

    fn snap_home_scroll(value: f32, max: f32) -> f32 {
        let slots = (max / HOME_TILE_PITCH).round();
        (value / HOME_TILE_PITCH).round().clamp(0., slots) * HOME_TILE_PITCH
    }

    pub fn bounce(&mut self, axis: BounceAxis, amount: f32) {
        if self.preferences.reduced_motion {
            return;
        }
        // Repeated wheel ticks at the edge keep one bounce going.
        if self
            .bounce
            .is_some_and(|(a, _, start)| a == axis && start.elapsed().as_secs_f32() < 0.18)
        {
            return;
        }
        self.bounce = Some((axis, amount, Instant::now()));
        self.dirty = true;
    }

    /// Offset of a running bounce: out past the edge and back with a soft overshoot.
    pub fn bounce_at(&self, axis: BounceAxis) -> f32 {
        match self.bounce {
            Some((a, amount, start)) if a == axis => {
                let t = start.elapsed().as_secs_f32();
                if t >= BOUNCE_TIME {
                    0.
                } else {
                    amount * (-7. * t).exp() * (13. * t).sin()
                }
            }
            _ => 0.,
        }
    }

    fn stack_row_max(&self, row: usize) -> f32 {
        let pitch = crate::stacks::row_pitch(self, row);
        let cards = crate::stacks::row_len(self, row) as f32;
        let overflow = cards * pitch - 26.4 + HOME_STRIP_LEFT - HOME_STRIP_RIGHT;
        ((overflow / pitch).ceil() * pitch).max(0.)
    }

    /// Pointer drag along a stack row: follows the hand, stretches past the ends.
    pub fn drag_stack_row(&mut self, row: usize, movement: f32) {
        let max = self.stack_row_max(row);
        let current = if self.stack_scroll.0 == row {
            self.stack_scroll.1
        } else {
            0.
        };
        let next = self.rubber(current, movement, max);
        self.stack_scroll = (row, next, next);
        self.dirty = true;
    }

    /// Edge auto-scroll: move a stack row without overscroll. Returns whether
    /// it moved.
    pub fn nudge_stack_row(&mut self, row: usize, movement: f32) -> bool {
        let max = self.stack_row_max(row);
        let current = if self.stack_scroll.0 == row {
            self.stack_scroll.1
        } else {
            0.
        };
        let next = (current + movement).clamp(0., max);
        if next == current {
            return false;
        }
        self.stack_scroll = (row, next, next);
        self.dirty = true;
        true
    }

    /// Pointer drag past an end stretches with resistance; with animations
    /// off it simply stops at the edge.
    pub fn rubber(&self, current: f32, movement: f32, max: f32) -> f32 {
        if self.preferences.reduced_motion {
            (current + movement).clamp(0., max)
        } else {
            rubber(current, movement, max)
        }
    }

    /// Drag released: coast a little with the throw, settle on a whole card.
    pub fn release_stack_row(&mut self, row: usize, velocity: f32) {
        let pitch = crate::stacks::row_pitch(self, row);
        let max = self.stack_row_max(row);
        let current = if self.stack_scroll.0 == row {
            self.stack_scroll.1
        } else {
            0.
        };
        let throw = if self.preferences.reduced_motion {
            0.
        } else {
            velocity * 0.22
        };
        let goal = (((current + throw) / pitch).round() * pitch).clamp(0., max);
        self.stack_scroll = (row, current, goal);
        self.dirty = true;
    }

    /// Wheel over a stack row: slide it by whole cards, bounce at the ends.
    pub fn scroll_stack_row(&mut self, row: usize, steps: f32) {
        let pitch = crate::stacks::row_pitch(self, row);
        let max = self.stack_row_max(row);
        let (current_row, current, goal) = self.stack_scroll;
        let (current, goal) = if current_row == row {
            (current, goal)
        } else {
            (0., 0.)
        };
        let wanted = goal + steps * pitch;
        if wanted < 0. && goal <= 0.5 {
            self.bounce(BounceAxis::Stack(row), -70.);
        } else if wanted > max && goal >= max - 0.5 {
            self.bounce(BounceAxis::Stack(row), 70.);
        }
        self.stack_scroll = (row, current, wanted.clamp(0., max));
        self.dirty = true;
    }

    /// Edge scroll ended: settle Favorites on a whole card inside the row.
    pub fn release_favorites(&mut self) {
        self.home_scroll_goal = Self::snap_home_scroll(self.home_scroll, self.home_scroll_max());
        self.dirty = true;
    }

    pub fn scroll_favorites(&mut self, delta: f32) {
        let max = self.home_scroll_max();
        if delta < 0. && self.home_scroll_goal <= 0.5 {
            self.bounce(BounceAxis::Favorites, -70.);
        } else if delta > 0. && self.home_scroll_goal >= max - 0.5 {
            self.bounce(BounceAxis::Favorites, 70.);
        }
        self.home_scroll_goal = self.home_scroll_goal.clamp(0., max);
        let next = (self.home_scroll + delta).clamp(0., max);
        if delta != 0. {
            self.home_scroll_goal = Self::snap_home_scroll(next, max);
        }
        if next != self.home_scroll {
            self.home_scroll = next;
            self.dirty = true;
        }
    }

    pub fn set_inventory(&mut self, next: Inventory, warning: Option<String>) {
        self.inventory.apps = next.apps;
        // Launch focus is the first Favorites tile, not the navigation.
        let first_load = self.loading;
        if let Some(warning) = warning {
            self.notify(warning);
        } else {
            self.inventory.inputs = next.inputs;
        }
        self.loading = false;
        self.error = None;
        self.dirty = true;
        self.reconcile_focus();
        if first_load && self.page == Page::Home && self.focus == Target::Nav(0) {
            self.focus = self.first_content();
        }
    }

    pub fn reconcile_focus(&mut self) {
        if self.page == Page::Home {
            self.scroll_favorites(0.);
        }
        if let Target::App(id) = &self.focus
            && !self.apps().iter().any(|app| &app.id == id)
        {
            self.focus = self.first_content();
        }
        if let Target::Input(id) = &self.focus
            && !self
                .inventory
                .inputs
                .iter()
                .any(|input| &input.id == id && self.preferences.input_visible(input))
        {
            self.focus = self.first_content();
        }
    }

    pub fn first_content(&self) -> Target {
        match self.page {
            Page::Home => self
                .apps()
                .get(((self.home_scroll - 0.01).max(0.) / HOME_TILE_PITCH).ceil() as usize)
                .map(|app| Target::App(app.id.clone()))
                .unwrap_or(if self.error.is_some() {
                    Target::Retry
                } else {
                    Target::Browse
                }),
            // Reading order of the tab: scenes row, cameras, devices.
            Page::SmartHome => self
                .ha_tab(2)
                .first()
                .map(|e| Target::SmartScene(e.id.clone()))
                .or_else(|| {
                    self.ha_tab(1)
                        .first()
                        .map(|e| Target::SmartCamera(e.id.clone()))
                })
                .or_else(|| {
                    self.ha_tab(0)
                        .first()
                        .map(|e| Target::SmartEntity(e.id.clone()))
                })
                .unwrap_or(Target::SmartConnect),
            // Down from the navigation lands on the category tabs first.
            Page::Apps => Target::Filter(self.filter),
            Page::Settings => match self.settings_level {
                SettingsLevel::Root => Target::Sidebar(0),
                SettingsLevel::Section => self.section_entry(),
                SettingsLevel::Widgets(_) => Target::WidgetRow(0),
                SettingsLevel::HeadingFonts => Target::HeadingFontPick(
                    crate::typography::HeadingFont::ALL
                        .iter()
                        .position(|&f| f == self.preferences.heading_font)
                        .unwrap_or(0),
                ),
                SettingsLevel::UiResolution => Target::UiResolutionPick(
                    crate::model::UiResolution::ALL
                        .iter()
                        .position(|&r| r == self.preferences.ui_resolution)
                        .unwrap_or(0),
                ),
                SettingsLevel::TextFonts => Target::TextFontPick(
                    crate::typography::TextFont::ALL
                        .iter()
                        .position(|&f| f == self.preferences.text_font)
                        .unwrap_or(0),
                ),
                SettingsLevel::WallpaperType => {
                    Target::WallpaperTab(usize::from(self.preferences.background == "video"))
                }
                SettingsLevel::WallpaperPhotos => Target::Background(0),
                SettingsLevel::StackAdd => Target::StackSource(0),
                SettingsLevel::MoonlightHosts => Target::MoonlightHost(0),
                SettingsLevel::MoonlightPin => Target::MoonlightCancelButton,
                SettingsLevel::JellyfinLink => Target::JellyfinServer(0),
                SettingsLevel::PlexLink => Target::PlexCancel,
                SettingsLevel::StackConfig(_) => Target::StackOpt(0),
                SettingsLevel::StackCollection(i) => Target::StackPick(
                    self.preferences
                        .stacks
                        .get(i)
                        .and_then(|s| {
                            crate::stacks::collection_choices(self, i)
                                .iter()
                                .position(|c| c.0 == s.collection)
                        })
                        .unwrap_or(0),
                ),
                SettingsLevel::StackShape(i) => Target::StackPick(
                    self.preferences
                        .stacks
                        .get(i)
                        .map_or(0, |s| s.shape.index()),
                ),
                SettingsLevel::WallpaperVideos => Target::VideoSource(1),
                SettingsLevel::WallpaperCategory(group) => crate::video_catalog::groups(0)
                    .get(group)
                    .and_then(|item| item.2.first())
                    .copied()
                    .map(Target::VideoPick)
                    .unwrap_or(Target::VideoGroup(group)),
            },
        }
    }

    pub fn focus(&mut self, target: Target, controls: &[Control]) {
        if self.focus != target {
            self.focus_started = Instant::now();
        }
        self.stop_kinetic();
        self.pointer = false;
        self.change_focus(target);
        self.reveal_focus(controls);
        if let Target::Sidebar(section) = self.focus {
            if self.subpage == Some(Subpage::WeatherLocation) {
                self.weather.cancel_search();
            }
            self.section = section;
            self.subpage = None;
        }
        self.dirty = true;
    }

    /// Bottom edge shared by the content clip and pointer hit-testing.
    pub fn main_bottom(&self) -> f32 {
        // Home and Apps run to the physical bottom edge: no footer, no mask.
        if matches!(self.page, Page::Home | Page::Apps | Page::SmartHome) && self.moving.is_none() {
            HOME_BOTTOM
        } else {
            CONTENT_BOTTOM
        }
    }

    /// Leave breathing room below the focused card after remote or touch scrolling.
    pub fn focus_bottom(&self) -> f32 {
        // Home keeps a calm margin under the lowest focused row.
        let inset = if matches!(self.page, Page::Home | Page::Apps | Page::SmartHome) {
            64.
        } else {
            FOCUS_BOTTOM_INSET
        };
        self.main_bottom() - inset
    }

    /// Remote focus is revealed after layout changes, including hide/reorder.
    /// Pointer paths deliberately do not call this method.
    pub fn reveal_focus(&mut self, controls: &[Control]) -> bool {
        let (previous, previous_home) = (self.scroll_goal, self.home_scroll_goal);
        // Home stands still under the title card while it opens, shows and
        // closes; its rows keep their scroll for the card to return to.
        if self.movie.is_some()
            || self.movie_closing.is_some()
            || self.game.is_some()
            || self.game_closing.is_some()
            || self.album.is_some()
            || self.album_closing.is_some()
            || self.library.is_some()
        {
            return false;
        }
        if let Some(control) = controls.iter().find(|c| c.target == self.focus) {
            if self.page == Page::Home
                && let Target::App(id) = &self.focus
                && let Some(index) = self.apps().iter().position(|app| &app.id == id)
            {
                let current = self.home_scroll_goal.clamp(0., self.home_scroll_max());
                let card_left = HOME_STRIP_LEFT + index as f32 * HOME_TILE_PITCH - current;
                let card_right = card_left + HOME_TILE_WIDTH;
                let target = if card_left < HOME_STRIP_LEFT {
                    // Keep the leftmost visible card aligned to the strip origin.
                    (index as f32 * HOME_TILE_PITCH).min(self.home_scroll_max())
                } else if card_right > HOME_STRIP_RIGHT {
                    // Advance only as far as needed, then snap to a full-card step.
                    let required =
                        (index as f32 * HOME_TILE_PITCH + HOME_STRIP_LEFT + HOME_TILE_WIDTH
                            - HOME_STRIP_RIGHT)
                            .max(0.);
                    (required / HOME_TILE_PITCH).ceil() * HOME_TILE_PITCH
                } else {
                    current
                };
                self.home_scroll_goal = target.min(self.home_scroll_max());
            } else if self.page == Page::Home && !matches!(self.focus, Target::App(_)) {
                // Leaving Favorites slides the row back to its first card.
                self.home_scroll_goal = 0.;
            }
            if matches!(
                self.focus,
                Target::Nav(_)
                    | Target::Filter(_)
                    | Target::Search
                    | Target::ClearSearch
                    | Target::Sidebar(_)
            ) {
                self.scroll_goal = 0.;
            } else if control.scrollable {
                let top = if self.page == Page::Settings
                    && (self.section == APPEARANCE_SECTION
                        || matches!(
                            self.settings_level,
                            SettingsLevel::HeadingFonts
                                | SettingsLevel::TextFonts
                                | SettingsLevel::UiResolution
                                | SettingsLevel::StackCollection(_)
                                | SettingsLevel::StackShape(_)
                        )
                        || self.section == STACKS_SECTION
                        || self.section == SMARTHOME_SECTION)
                {
                    208.
                } else if self.page == Page::Apps {
                    164.8
                } else if self.page == Page::SmartHome {
                    SMART_REVEAL_TOP
                } else if self.page == Page::Home {
                    // A row scrolled back in stops with its title below the
                    // clock and clear of the top fade.
                    HOME_REVEAL_TOP
                } else {
                    120.8
                };
                if control.rect.y < top {
                    self.scroll_goal = (self.scroll + control.rect.y - top).max(0.);
                }
                let bottom = if self.page == Page::Settings
                    && (self.section == APPEARANCE_SECTION
                        || matches!(
                            self.settings_level,
                            SettingsLevel::HeadingFonts
                                | SettingsLevel::TextFonts
                                | SettingsLevel::UiResolution
                                | SettingsLevel::StackCollection(_)
                                | SettingsLevel::StackShape(_)
                        )
                        || self.section == STACKS_SECTION
                        || self.section == SMARTHOME_SECTION)
                {
                    936.
                } else {
                    self.focus_bottom()
                };
                if control.rect.y + control.rect.h > bottom {
                    self.scroll_goal =
                        (self.scroll + control.rect.y + control.rect.h - bottom).max(0.);
                }
            }
        }
        if self.page == Page::Home {
            if let Target::StackCard(row, index) = self.focus {
                let pitch = crate::stacks::row_pitch(self, row);
                let (current_row, _, goal) = self.stack_scroll;
                let goal = if current_row == row { goal } else { 0. };
                let left = index as f32 * pitch - goal;
                let goal = if left < 0. {
                    index as f32 * pitch
                } else if HOME_STRIP_LEFT + left + pitch - 26.4 > HOME_STRIP_RIGHT {
                    // Keep the focused card fully inside, one full card step at a time.
                    ((index as f32 * pitch + HOME_STRIP_LEFT + pitch - 26.4 - HOME_STRIP_RIGHT)
                        / pitch)
                        .ceil()
                        * pitch
                } else {
                    goal
                };
                if current_row != row {
                    self.glide_stack_row_home();
                    self.stack_scroll = (row, 0., goal);
                } else {
                    self.stack_scroll.2 = goal;
                }
            } else {
                // Leaving a stack row slides it back to its first card.
                self.glide_stack_row_home();
            }
            if matches!(self.focus, Target::App(_) | Target::VideoTitle) {
                // Back in Favorites (or above), Home returns to its resting layout.
                self.scroll_goal = 0.;
            }
            self.scroll_favorites(0.);
            self.home_scroll_goal =
                Self::snap_home_scroll(self.home_scroll_goal, self.home_scroll_max());
        }
        if self.preferences.reduced_motion {
            self.scroll = self.scroll_goal;
            self.home_scroll = self.home_scroll_goal;
        }
        let changed = self.scroll_goal != previous || self.home_scroll_goal != previous_home;
        if changed {
            self.dirty = true;
        }
        changed
    }

    /// Hands the scrolled row over to the delayed glide-back and resets the live scroll.
    fn glide_stack_row_home(&mut self) {
        let (row, current, _) = self.stack_scroll;
        // A very long glide back passes dozens of cards in half a second:
        // start it at most a few screens away.
        let current = current.min(STACK_RETURN_MAX);
        if current.abs() > 0.5 && !self.preferences.reduced_motion {
            self.stack_return = Some((row, current, Instant::now()));
        }
        self.stack_scroll = (row, 0., 0.);
    }

    /// Offset of a row that is gliding back: a short pause, an ease-out to the
    /// first card, then a small springy overshoot that settles.
    pub fn stack_return_shift(&self, row: usize) -> f32 {
        let Some((r, offset, start)) = self.stack_return else {
            return 0.;
        };
        if r != row {
            return 0.;
        }
        stack_return_curve(offset, start.elapsed().as_secs_f32())
    }

    /// Hover and pointer-down must not move the control before pointer-up.
    pub fn pointer_focus(&mut self, target: Target) {
        if !self.pointer {
            self.dirty = true;
        }
        self.pointer = true;
        if let Some(playback) = self.playback.as_mut() {
            playback.reveal();
            match target {
                Target::PlayerPill(i) => playback.selected_pill = i,
                Target::PlayerOption(i) => playback.selected_row = i,
                Target::PlayerAudioOption(i) => {
                    playback.panel = Some(crate::player::Panel::Audio);
                    playback.selected_row = i;
                }
                Target::PlayerSubtitleOption(i) => {
                    playback.panel = Some(crate::player::Panel::Subtitles);
                    playback.selected_row = i;
                }
                _ => {}
            }
        }
        if self.focus == target {
            return;
        }
        if let (Target::LibCard(i), Some(library)) = (&target, self.library.as_mut()) {
            library.grid_focus = *i;
        }
        if let Target::Sidebar(section) = target {
            if self.subpage == Some(Subpage::WeatherLocation) {
                self.weather.cancel_search();
            }
            self.section = section;
            self.subpage = None;
        }
        self.scroll_goal = self.scroll;
        self.home_scroll_goal = self.home_scroll;
        self.change_focus(target);
        self.dirty = true;
    }

    pub fn hidden_count(&self) -> usize {
        self.inventory
            .apps
            .iter()
            .filter(|app| self.preferences.hidden.contains(&app.id))
            .count()
    }

    fn section_entry(&self) -> Target {
        match self.section {
            1 => Target::WallpaperMenu(0),
            WEATHER_SECTION => Target::WeatherToggle,
            SMARTHOME_SECTION => {
                if self.preferences.smart_home.server_url.is_empty() {
                    Target::SmartConnect
                } else {
                    Target::SmartShowTab
                }
            }
            SYSTEM_SECTION => Target::Setting(3),
            APPEARANCE_SECTION => Target::AccentChoice(7),
            STACKS_SECTION => Target::StackRow(0),
            INPUTS_SECTION => self
                .inventory
                .inputs
                .first()
                .map(|input| Target::InputMode(input.id.clone()))
                .unwrap_or(Target::Setting(3)),
            _ => Target::Setting(0),
        }
    }

    pub(crate) fn settings_push(&mut self, level: SettingsLevel, target: Target) {
        self.previous_panel_layer = None;
        self.previous_sidebar_layer = None;
        self.settings_forward = true;
        self.settings_level = level;
        self.change_focus(target);
        self.scroll = 0.;
        self.scroll_goal = 0.;
        self.panel_started = Some(Instant::now());
        self.dirty = true;
    }

    pub(crate) fn settings_pop(&mut self, level: SettingsLevel, target: Target) {
        self.previous_panel_layer = None;
        self.previous_sidebar_layer = None;
        self.settings_forward = false;
        self.settings_level = level;
        self.change_focus(target);
        self.scroll = 0.;
        self.scroll_goal = 0.;
        self.panel_started = Some(Instant::now());
        self.dirty = true;
    }

    fn close_subpage(&mut self) {
        if self.subpage == Some(Subpage::WeatherLocation) {
            self.weather.cancel_search();
        }
        self.panel_started = (!self.preferences.reduced_motion).then(Instant::now);
        self.settings_forward = false;
        self.pointer = false;
        self.focus = match self.subpage.take() {
            Some(Subpage::AppNames) => Target::Setting(0),
            Some(Subpage::Greeting) => Target::Setting(6),
            Some(Subpage::GreetingStyle) => {
                self.subpage = Some(Subpage::Greeting);
                Target::GreetingStyle
            }
            Some(Subpage::Clock) => Target::Setting(1),
            Some(Subpage::ClockFormat) => {
                self.subpage = Some(Subpage::Clock);
                Target::ClockFormat
            }
            Some(Subpage::TimeFormat) => {
                self.subpage = Some(Subpage::Clock);
                Target::ClockTimeFormat
            }
            Some(Subpage::Input(id)) => Target::InputMode(id),
            Some(Subpage::WeatherLocation) => Target::WeatherLocation,
            Some(Subpage::Temperature) => Target::WeatherTemperature,
            Some(Subpage::Language) => Target::Setting(4),
            Some(Subpage::SmartConnect) => Target::SmartConnect,
            Some(Subpage::SmartSelect(n)) => Target::SmartSection(n),
            None => Target::Sidebar(self.section),
        };
        // Reset the goal too, or the eased scroll glides the parent page up
        // to where the long subpage was.
        self.scroll = 0.;
        self.scroll_goal = 0.;
        self.dirty = true;
    }

    pub fn direction(&mut self, direction: Direction, controls: &[Control]) -> Vec<Effect> {
        if self.ambient_on() {
            // Left and Right flip the clip, as on the caption; the rest wakes.
            if matches!(direction, Direction::Left | Direction::Right)
                && self.preferences.background == "video"
            {
                self.focus = Target::VideoTitle;
            } else {
                self.leave_ambient();
                return vec![];
            }
        }
        self.pointer = false;
        self.dirty = true;
        if matches!(self.overlay, Some(Overlay::SystemKeyboard)) {
            // The TV keyboard owns its arrows until Done/Back dismisses it.
            return vec![];
        }
        if let Some(effects) = self.mini_direction(direction, controls) {
            return effects;
        }
        if self.playback.is_none()
            && self.movie.is_none()
            && self.game.is_none()
            && self.album.is_none()
            && self.overlay.is_none()
            && self.ha_camera.is_none()
            && let Some(effects) = self.widget_direction(direction, controls)
        {
            return effects;
        }
        if let Some(playback) = self.playback.as_mut() {
            // Three rows: close button, the timeline (seek), the bottom buttons.
            let row = playback.bottom_row();
            if playback.panel.is_none()
                && let Some(at) = row.iter().position(|t| *t == self.focus)
            {
                match direction {
                    Direction::Left => self.focus = row[at.saturating_sub(1)].clone(),
                    Direction::Right => self.focus = row[(at + 1).min(row.len() - 1)].clone(),
                    Direction::Up => self.focus = Target::PlayerToggle,
                    Direction::Down => {}
                }
                if let Target::PlayerPill(i) = self.focus {
                    playback.selected_pill = i;
                }
                playback.reveal();
                return vec![];
            }
            if playback.panel.is_none() && matches!(self.focus, Target::PlayerClose) {
                if matches!(direction, Direction::Down) {
                    self.focus = Target::PlayerToggle;
                }
                playback.reveal();
                return vec![];
            }
            if playback.panel.is_none() && matches!(direction, Direction::Up) {
                self.focus = Target::PlayerClose;
                playback.reveal();
                return vec![];
            }
            if playback.panel.is_none() && matches!(direction, Direction::Down) {
                let pill = Target::PlayerPill(playback.selected_pill);
                self.focus = if row.contains(&pill) {
                    pill
                } else {
                    row.first().cloned().unwrap_or(Target::PlayerToggle)
                };
                playback.reveal();
                return vec![];
            }
            let commands = playback.navigate(direction);
            if let Some(panel) = playback.panel {
                self.focus = match panel {
                    crate::player::Panel::Audio => Target::PlayerAudioOption(playback.selected_row),
                    crate::player::Panel::Subtitles => {
                        Target::PlayerSubtitleOption(playback.selected_row)
                    }
                    crate::player::Panel::Quality => Target::PlayerOption(playback.selected_row),
                    crate::player::Panel::Chapters => Target::PlayerChapter(playback.selected_row),
                };
            } else if matches!(
                self.focus,
                Target::PlayerChapter(_)
                    | Target::PlayerAudioOption(_)
                    | Target::PlayerSubtitleOption(_)
                    | Target::PlayerOption(_)
                    | Target::PlayerPanelClose
            ) {
                self.focus = Target::PlayerPill(playback.selected_pill);
            }
            return commands.into_iter().map(Effect::Player).collect();
        }
        if self.movie.is_some() {
            return self.movie_direction(direction);
        }
        if self.album.is_some() {
            return self.album_direction(direction);
        }
        if self.game.is_some() {
            let online = self.game.as_ref().is_some_and(|g| g.online);
            let running = self.game.as_ref().is_some_and(|g| g.running);
            let trailer = self.game.as_ref().is_some_and(|g| {
                self.game_info
                    .get(&g.id)
                    .is_some_and(|i| i.steam_id > 0 && !i.trailer.is_empty())
            });
            // Screenshots on the card: as many as the layout fitted.
            let shots: Vec<Target> = controls
                .iter()
                .filter(|c| matches!(c.target, Target::GameShot(_)))
                .map(|c| c.target.clone())
                .collect();
            let full = self.game.as_ref().map_or(0, |g| {
                self.game_info
                    .get(&g.id)
                    .map_or(0, |i| i.screenshots_full.len())
            });
            if let Some(g) = self.game.as_mut()
                && let Some(n) = g.shot
            {
                // Viewer open: left and right page through, up/down do nothing.
                if full > 0 {
                    g.shot = Some(match direction {
                        Direction::Left => (n + full - 1) % full,
                        Direction::Right => (n + 1) % full,
                        _ => n,
                    });
                    self.dirty = true;
                }
                return vec![];
            }
            // Rows: close (top), buttons, screenshots. Left/right move inside a
            // row, up/down between rows.
            let mut targets = vec![Target::GamePlay];
            if running {
                targets.push(Target::GameQuit);
            }
            if !online {
                targets.push(Target::GameWake);
            }
            if trailer {
                targets.push(Target::GameTrailer);
            }
            targets.push(Target::GameFavorite);
            let next = match (&self.focus, direction) {
                (Target::GameClose, Direction::Down | Direction::Left) => Some(Target::GamePlay),
                (Target::GameClose, _) => None,
                (Target::GameShot(_), Direction::Up) => Some(Target::GamePlay),
                (Target::GameShot(n), Direction::Left) => n.checked_sub(1).map(Target::GameShot),
                (Target::GameShot(n), Direction::Right) => shots
                    .iter()
                    .find(|t| **t == Target::GameShot(n + 1))
                    .cloned(),
                (Target::GameShot(_), Direction::Down) => None,
                (focus, direction) => {
                    let current = targets.iter().position(|t| t == focus).unwrap_or(0);
                    match direction {
                        Direction::Left => current.checked_sub(1).map(|i| targets[i].clone()),
                        Direction::Right => targets.get(current + 1).cloned(),
                        Direction::Up => Some(Target::GameClose),
                        Direction::Down => shots.first().cloned(),
                    }
                }
            };
            if let Some(next) = next {
                self.focus(next, controls);
            }
            return vec![];
        }
        if self.library.is_some() && self.overlay.is_none() {
            return self.library_direction(direction);
        }
        if let Some(id) = self.moving.clone() {
            let mut ids = self.preferences.favorites(&self.inventory.apps);
            let slots: Vec<_> = ids
                .iter()
                .enumerate()
                .filter_map(|(index, id)| {
                    (!self.preferences.hidden.contains(id)
                        && self.inventory.apps.iter().any(|app| &app.id == id))
                    .then_some(index)
                })
                .collect();
            if let Some(index) = slots.iter().position(|slot| ids[*slot] == id) {
                let next = match direction {
                    Direction::Left => index.saturating_sub(1),
                    Direction::Right => (index + 1).min(slots.len() - 1),
                    _ => index,
                };
                if next != index {
                    ids.swap(slots[index], slots[next]);
                    self.preferences.favorites = Some(ids);
                    self.dirty = true;
                    return vec![Effect::Save];
                }
            }
            return vec![];
        }
        if self.overlay.is_none() {
            if self.page == Page::Settings
                && self.settings_level != SettingsLevel::Root
                && matches!(direction, Direction::Up)
                && matches!(
                    self.focus,
                    Target::WallpaperMenu(0)
                        | Target::WallpaperTab(0)
                        | Target::WallpaperTab(1)
                        | Target::VideoGroup(0)
                        | Target::VideoPick(_)
                        | Target::Setting(0)
                        | Target::WeatherToggle
                        | Target::SmartConnect
                        | Target::SmartShowTab
                )
            {
                let first = match self.settings_level {
                    SettingsLevel::WallpaperCategory(group) => crate::video_catalog::groups(0)
                        .get(group)
                        .and_then(|g| g.2.first())
                        .copied()
                        .map(|v| self.focus == Target::VideoPick(v))
                        .unwrap_or(false),
                    _ => true,
                };
                if first {
                    self.focus(Target::SettingsBack, controls);
                    return vec![];
                }
            }
            if self.page == Page::Settings && self.subpage.is_none() {
                // Appearance owns Left/Right to move among swatches. Only the
                // dedicated Back control and the physical BACK key navigate
                // away from this screen. Other legacy sections retain Left.
                if self.settings_level == SettingsLevel::Section
                    && self.section != APPEARANCE_SECTION
                    && matches!(direction, Direction::Left)
                    && !matches!(self.focus, Target::Nav(_))
                {
                    // The first content row navigates to the visible Back
                    // arrow, rather than silently popping an entire level.
                    let first = self.first_content();
                    if self.focus == first {
                        self.focus(Target::SettingsBack, controls);
                        return vec![];
                    }
                    return self.back();
                }
                if let Target::VideoGroup(group) = self.focus
                    && self.settings_level == SettingsLevel::WallpaperVideos
                    && matches!(direction, Direction::Right)
                {
                    let first = crate::video_catalog::groups(0)
                        .get(group)
                        .and_then(|g| g.2.first())
                        .copied();
                    if let Some(first) = first {
                        self.settings_push(
                            SettingsLevel::WallpaperCategory(group),
                            Target::VideoPick(first),
                        );
                    }
                    return vec![];
                }
                if let SettingsLevel::WallpaperCategory(group) = self.settings_level
                    && let Target::VideoPick(index) = self.focus
                    && let Some(items) = crate::video_catalog::groups(0)
                        .get(group)
                        .map(|g| g.2.clone())
                    && let Some(pos) = items.iter().position(|item| *item == index)
                {
                    let next = match direction {
                        Direction::Left if pos % 4 > 0 => Some(pos - 1),
                        Direction::Right if pos % 4 < 3 && pos + 1 < items.len() => Some(pos + 1),
                        Direction::Up if pos >= 4 => Some(pos - 4),
                        Direction::Down if pos + 4 < items.len() => Some(pos + 4),
                        _ => None,
                    };
                    if let Some(next) = next {
                        self.focus(Target::VideoPick(items[next]), controls);
                    }
                    return vec![];
                }
            }
            if self.page == Page::Home && self.focus == Target::VideoTitle {
                match direction {
                    Direction::Left | Direction::Right => {
                        let excluded = &self.preferences.video_excluded;
                        let shuffle = self.preferences.video_shuffle;
                        let seed = self.preferences.video_shuffle_seed;
                        let current = self.preferences.video_index;
                        let next = if matches!(direction, Direction::Right) {
                            crate::video_catalog::next_index(current, excluded, shuffle, seed)
                        } else {
                            crate::video_catalog::prev_index(current, excluded, shuffle, seed)
                        };
                        if let Some(next) = next {
                            self.preferences.video_index = next;
                            return vec![Effect::Save];
                        }
                    }
                    Direction::Up => self.focus(Target::Nav(0), controls),
                    Direction::Down => {
                        if let Some(app) = self.apps().first() {
                            let id = app.id.clone();
                            self.focus(Target::App(id), controls);
                        }
                    }
                }
                return vec![];
            }
            if self.page == Page::Home
                && let Target::StackCard(row, index) = self.focus
            {
                // Whole rows, not just the cards on screen: the row scrolls.
                let preview = controls
                    .iter()
                    .filter(|c| matches!(c.target, Target::StackCard(..)))
                    .count();
                let in_row = |r: usize| {
                    let n = crate::stacks::row_len(self, r);
                    if n == 0 && preview > 0 {
                        controls
                            .iter()
                            .filter(|c| matches!(c.target, Target::StackCard(rr, _) if rr == r))
                            .count()
                    } else {
                        n
                    }
                };
                match direction {
                    Direction::Left if index > 0 => {
                        self.focus(Target::StackCard(row, index - 1), controls)
                    }
                    Direction::Right if index + 1 < in_row(row) => {
                        self.focus(Target::StackCard(row, index + 1), controls)
                    }
                    // Changing rows always lands on the first card of the new row.
                    Direction::Up if row > 0 => self.focus(Target::StackCard(row - 1, 0), controls),
                    Direction::Up => {
                        let apps = self.apps();
                        if let Some(app) = apps.get(index.min(apps.len().saturating_sub(1))) {
                            let id = app.id.clone();
                            self.focus(Target::App(id), controls);
                        }
                    }
                    Direction::Down if in_row(row + 1) > 0 => {
                        self.focus(Target::StackCard(row + 1, 0), controls)
                    }
                    _ => {}
                }
                return vec![];
            }
            if self.page == Page::Home
                && let Target::App(id) = &self.focus
            {
                let apps = self.apps();
                if let Some(index) = apps.iter().position(|app| &app.id == id) {
                    match direction {
                        Direction::Left | Direction::Right => {
                            let next = if matches!(direction, Direction::Left) {
                                (index + apps.len() - 1) % apps.len()
                            } else {
                                (index + 1) % apps.len()
                            };
                            let id = apps[next].id.clone();
                            self.focus(Target::App(id), controls);
                        }
                        Direction::Up => {
                            if controls.iter().any(|c| c.target == Target::VideoTitle) {
                                self.focus(Target::VideoTitle, controls)
                            } else {
                                self.focus(Target::Nav(0), controls)
                            }
                        }
                        Direction::Down => {
                            let visible = controls
                                .iter()
                                .filter(|c| matches!(c.target, Target::StackCard(0, _)))
                                .count();
                            if visible > 0 {
                                self.focus(Target::StackCard(0, index.min(visible - 1)), controls);
                                return vec![];
                            }
                            let x = controls
                                .iter()
                                .find(|control| control.target == self.focus)
                                .map_or(960., |control| control.rect.center().0);
                            let top = controls
                                .iter()
                                .filter(|control| matches!(control.target, Target::Input(_)))
                                .map(|control| control.rect.y)
                                .fold(f32::INFINITY, f32::min);
                            let next = controls
                                .iter()
                                .filter(|control| {
                                    matches!(control.target, Target::Input(_))
                                        && (control.rect.y - top).abs() < 1.
                                })
                                .min_by(|a, b| {
                                    (a.rect.center().0 - x)
                                        .abs()
                                        .total_cmp(&(b.rect.center().0 - x).abs())
                                })
                                .map(|control| control.target.clone());
                            if let Some(next) = next {
                                self.focus(next, controls);
                            }
                        }
                    }
                    return vec![];
                }
            }
            let setting_rows = match self.subpage {
                Some(Subpage::Greeting) => Some(vec![
                    Target::GreetingStyle,
                    Target::GreetingName,
                    Target::GreetingNameClear,
                ]),
                Some(Subpage::AppNames) => Some(vec![Target::AppNamesHome, Target::AppNamesApps]),
                Some(Subpage::Clock) => Some(vec![Target::ClockFormat, Target::ClockTimeFormat]),
                _ => None,
            };
            if let Some(rows) = setting_rows
                && let Some(index) = rows.iter().position(|target| *target == self.focus)
            {
                match direction {
                    Direction::Left => self.close_subpage(),
                    Direction::Up => self.focus(
                        if index == 0 {
                            Target::Nav(self.settings_nav_index())
                        } else {
                            rows[0].clone()
                        },
                        controls,
                    ),
                    Direction::Down if index + 1 < rows.len() => {
                        self.focus(rows[index + 1].clone(), controls)
                    }
                    Direction::Right
                        if matches!(
                            self.focus,
                            Target::GreetingStyle
                                | Target::GreetingName
                                | Target::ClockFormat
                                | Target::ClockTimeFormat
                        ) =>
                    {
                        return self.activate();
                    }
                    _ => {}
                }
                return vec![];
            }
            if let Target::Choice(index) = self.focus {
                let count = match self.subpage {
                    Some(
                        Subpage::ClockFormat
                        | Subpage::TimeFormat
                        | Subpage::Temperature
                        | Subpage::GreetingStyle,
                    ) => 2,
                    Some(Subpage::Language) => Language::ALL.len(),
                    _ => 3,
                };
                match direction {
                    Direction::Left => self.close_subpage(),
                    Direction::Up => self.focus(
                        if index == 0 {
                            Target::Nav(self.settings_nav_index())
                        } else {
                            Target::Choice(index - 1)
                        },
                        controls,
                    ),
                    Direction::Down
                        if index + 1 == count
                            && matches!(self.subpage, Some(Subpage::Input(_))) =>
                    {
                        self.focus(Target::InputRename, controls)
                    }
                    Direction::Down => self.focus(Target::Choice((index + 1) % count), controls),
                    Direction::Right => {}
                }
                return vec![];
            }
            if matches!(self.focus, Target::InputRename | Target::InputNameReset) {
                match direction {
                    Direction::Left => self.close_subpage(),
                    Direction::Up => self.focus(
                        if self.focus == Target::InputNameReset {
                            Target::InputRename
                        } else {
                            Target::Choice(2)
                        },
                        controls,
                    ),
                    Direction::Down
                        if self.focus == Target::InputRename
                            && controls.iter().any(|c| c.target == Target::InputNameReset) =>
                    {
                        self.focus(Target::InputNameReset, controls)
                    }
                    _ => {}
                }
                return vec![];
            }
            if let Target::Sidebar(index) = self.focus {
                const ORDER: [usize; SECTION_COUNT] = SECTION_ORDER;
                let pos = ORDER.iter().position(|&i| i == index).unwrap_or(0);
                match direction {
                    Direction::Right => return self.activate(),
                    Direction::Up => self.focus(
                        if pos == 0 {
                            Target::Nav(self.settings_nav_index())
                        } else {
                            Target::Sidebar(ORDER[pos - 1])
                        },
                        controls,
                    ),
                    Direction::Down => {
                        self.focus(Target::Sidebar(ORDER[(pos + 1) % SECTION_COUNT]), controls)
                    }
                    Direction::Left => {}
                }
                return vec![];
            }
            if self.page == Page::Settings
                && self.focus == Target::SettingsBack
                && matches!(direction, Direction::Right | Direction::Down)
            {
                self.focus(self.first_content(), controls);
                return vec![];
            }
            // Settings › Stacks: every page is one vertical list.
            if self.page == Page::Settings {
                let list = match self.focus {
                    Target::StackRow(i) => Some((i, self.preferences.stacks.len() + 1)),
                    Target::StackSource(i) => Some((i, crate::stacks::SOURCES.len())),
                    Target::StackOpt(i) => Some((
                        i,
                        if let SettingsLevel::StackConfig(n) = self.settings_level {
                            self.preferences
                                .stacks
                                .get(n)
                                .map_or(7, |s| crate::stacks::option_count(&s.source))
                        } else {
                            7
                        },
                    )),
                    Target::MoonlightHost(i) => Some((i, self.moonlight_hosts.len() + 2)),
                    Target::StackPick(i) => Some((
                        i,
                        match self.settings_level {
                            SettingsLevel::StackShape(_) => crate::stacks::CardShape::ALL.len(),
                            SettingsLevel::StackCollection(s) => {
                                crate::stacks::collection_choices(self, s).len()
                            }
                            _ => 0,
                        },
                    )),
                    _ => None,
                };
                if let Some((i, count)) = list {
                    let at = |n: usize| match self.focus {
                        Target::StackRow(_) => Target::StackRow(n),
                        Target::StackSource(_) => Target::StackSource(n),
                        Target::StackOpt(_) => Target::StackOpt(n),
                        Target::MoonlightHost(_) => Target::MoonlightHost(n),
                        _ => Target::StackPick(n),
                    };
                    let next = match direction {
                        Direction::Up if i == 0 => {
                            if self.settings_level == SettingsLevel::Section {
                                Target::Nav(self.settings_nav_index())
                            } else {
                                Target::SettingsBack
                            }
                        }
                        Direction::Up => at(i - 1),
                        Direction::Down => at((i + 1).min(count - 1)),
                        Direction::Left => Target::SettingsBack,
                        Direction::Right => return self.activate(),
                    };
                    self.focus(next, controls);
                    return vec![];
                }
            }
            // Dedicated UI density choice. Always return to parent cell on Back.
            if self.page == Page::Settings
                && self.settings_level == SettingsLevel::UiResolution
                && let Target::UiResolutionPick(i) = self.focus
            {
                let next = match direction {
                    Direction::Up if i == 0 => Target::SettingsBack,
                    Direction::Up => Target::UiResolutionPick(i - 1),
                    Direction::Down => Target::UiResolutionPick((i + 1).min(3)),
                    Direction::Left => Target::SettingsBack,
                    Direction::Right => return self.activate(),
                };
                self.focus(next, controls);
                return vec![];
            }
            // Font gallery: single-column tvOS cells with live sample and scroll.
            if self.page == Page::Settings && self.subpage.is_none() {
                let gallery = match self.focus {
                    Target::HeadingFontPick(i) => {
                        Some((i, crate::typography::HeadingFont::ALL.len(), true))
                    }
                    Target::TextFontPick(i) => {
                        Some((i, crate::typography::TextFont::ALL.len(), false))
                    }
                    _ => None,
                };
                if let Some((i, count, heading)) = gallery {
                    let next = match direction {
                        Direction::Up if i == 0 => Target::SettingsBack,
                        Direction::Up => {
                            if heading {
                                Target::HeadingFontPick(i - 1)
                            } else {
                                Target::TextFontPick(i - 1)
                            }
                        }
                        Direction::Down => {
                            if heading {
                                Target::HeadingFontPick((i + 1).min(count - 1))
                            } else {
                                Target::TextFontPick((i + 1).min(count - 1))
                            }
                        }
                        Direction::Left => Target::SettingsBack,
                        Direction::Right => return self.activate(),
                    };
                    self.focus(next, controls);
                    return vec![];
                }
                match self.focus {
                    Target::HeadingSize | Target::TextSize
                        if matches!(direction, Direction::Left | Direction::Right) =>
                    {
                        let delta = if matches!(direction, Direction::Right) {
                            5i16
                        } else {
                            -5i16
                        };
                        if self.focus == Target::HeadingSize {
                            self.preferences.heading_size_pct =
                                (self.preferences.heading_size_pct as i16 + delta).clamp(85, 170)
                                    as u8;
                        } else {
                            self.preferences.text_size_pct =
                                (self.preferences.text_size_pct as i16 + delta).clamp(90, 150)
                                    as u8;
                        }
                        self.dirty = true;
                        return vec![Effect::Save];
                    }
                    Target::PhraseTitleSize | Target::PhraseSubtitleSize
                        if matches!(direction, Direction::Left | Direction::Right) =>
                    {
                        let delta: i16 = if matches!(direction, Direction::Right) {
                            5
                        } else {
                            -5
                        };
                        let (min, max) = (
                            crate::model::PHRASE_PCT_MIN as i16,
                            crate::model::PHRASE_PCT_MAX as i16,
                        );
                        let value = if self.focus == Target::PhraseTitleSize {
                            &mut self.preferences.phrase_title_pct
                        } else {
                            &mut self.preferences.phrase_subtitle_pct
                        };
                        *value = (*value as i16 + delta).clamp(min, max) as u8;
                        self.dirty = true;
                        return vec![Effect::Save];
                    }
                    Target::UiResolutionOpen if matches!(direction, Direction::Right) => {
                        return self.activate();
                    }
                    Target::HeadingFontOpen | Target::TextFontOpen
                        if matches!(direction, Direction::Right) =>
                    {
                        return self.activate();
                    }
                    _ => {}
                }
                if matches!(
                    self.focus,
                    Target::HeadingFontOpen
                        | Target::TextFontOpen
                        | Target::HeadingSize
                        | Target::TextSize
                        | Target::PhraseTitleSize
                        | Target::PhraseSubtitleSize
                        | Target::UiSounds
                        | Target::Screensaver
                        | Target::UiResolutionOpen
                ) && self.section == APPEARANCE_SECTION
                    && self.settings_level == SettingsLevel::Section
                {
                    let next = match (&self.focus, direction) {
                        (Target::HeadingFontOpen, Direction::Up) => Target::UiResolutionOpen,
                        (Target::HeadingFontOpen, Direction::Down) => Target::TextFontOpen,
                        (Target::TextFontOpen, Direction::Up) => Target::HeadingFontOpen,
                        (Target::TextFontOpen, Direction::Down) => Target::HeadingSize,
                        (Target::HeadingSize, Direction::Up) => Target::TextFontOpen,
                        (Target::HeadingSize, Direction::Down) => Target::TextSize,
                        (Target::TextSize, Direction::Up) => Target::HeadingSize,
                        (Target::TextSize, Direction::Down) => Target::PhraseTitleSize,
                        (Target::PhraseTitleSize, Direction::Up) => Target::TextSize,
                        (Target::PhraseTitleSize, Direction::Down) => Target::PhraseSubtitleSize,
                        (Target::PhraseSubtitleSize, Direction::Up) => Target::PhraseTitleSize,
                        (Target::PhraseSubtitleSize, Direction::Down) => Target::UiSounds,
                        (Target::UiSounds, Direction::Up) => Target::PhraseSubtitleSize,
                        (Target::UiSounds, Direction::Down) => Target::Screensaver,
                        (Target::Screensaver, Direction::Up) => Target::UiSounds,
                        (Target::UiResolutionOpen, Direction::Up) => Target::AccentOption(2),
                        (Target::UiResolutionOpen, Direction::Down) => Target::HeadingFontOpen,
                        (_, Direction::Left) => Target::SettingsBack,
                        _ => self.focus.clone(),
                    };
                    self.focus(next, controls);
                    return vec![];
                }
            }
            if matches!(
                self.focus,
                Target::AccentChoice(_) | Target::AccentOption(_)
            ) && self.page == Page::Settings
                && self.section == APPEARANCE_SECTION
            {
                let next = match self.focus {
                    Target::AccentChoice(i) => match direction {
                        Direction::Left if i == 0 || i == 8 => Target::SettingsBack,
                        Direction::Left => Target::AccentChoice(i - 1),
                        Direction::Right => {
                            Target::AccentChoice(if i == 7 || i == 15 { i } else { i + 1 })
                        }
                        Direction::Down if i < 8 => Target::AccentChoice(i + 8),
                        Direction::Down => Target::AccentOption(0),
                        Direction::Up if i >= 8 => Target::AccentChoice(i - 8),
                        Direction::Up => Target::SettingsBack,
                    },
                    Target::AccentOption(i) => match direction {
                        Direction::Left if i == 0 => Target::SettingsBack,
                        Direction::Up if i == 0 => Target::AccentChoice(8),
                        Direction::Up => Target::AccentOption(i.saturating_sub(1)),
                        Direction::Down if i == 2 => Target::UiResolutionOpen,
                        Direction::Down => Target::AccentOption((i + 1).min(2)),
                        _ => Target::AccentOption(i),
                    },
                    Target::SettingsBack => Target::AccentChoice(0),
                    _ => self.focus.clone(),
                };
                self.focus(next, controls);
                return vec![];
            }
            if matches!(
                self.focus,
                Target::Setting(0 | 1 | 4 | 6)
                    | Target::InputMode(_)
                    | Target::WeatherLocation
                    | Target::WeatherTemperature
            ) && matches!(direction, Direction::Right)
            {
                return self.activate();
            }
            if matches!(self.focus, Target::Setting(_) | Target::WeatherToggle)
                && matches!(direction, Direction::Right)
            {
                return vec![];
            }
            if self.focus == Target::Setting(0) && matches!(direction, Direction::Up) {
                self.focus(Target::Nav(self.settings_nav_index()), controls);
                return vec![];
            }
            if self.focus == Target::WeatherToggle && matches!(direction, Direction::Up) {
                self.focus(Target::Nav(self.settings_nav_index()), controls);
                return vec![];
            }
            if matches!(
                self.focus,
                Target::WeatherQuery | Target::WeatherSearch | Target::WeatherClear
            ) {
                match direction {
                    Direction::Left => {
                        self.close_subpage();
                        return vec![];
                    }
                    Direction::Up => {
                        self.focus(
                            if self.preferences.weather_location.is_some()
                                && self.focus != Target::WeatherClear
                            {
                                Target::WeatherClear
                            } else {
                                Target::Nav(self.settings_nav_index())
                            },
                            controls,
                        );
                        return vec![];
                    }
                    Direction::Down => {
                        let target = if self.focus == Target::WeatherClear {
                            Some(Target::WeatherQuery)
                        } else if !self.weather.results.is_empty() {
                            Some(Target::WeatherResult(0))
                        } else if self.focus == Target::WeatherQuery {
                            Some(Target::WeatherSearch)
                        } else {
                            None
                        };
                        if let Some(target) = target {
                            self.focus(target, controls);
                        }
                        return vec![];
                    }
                    _ => {}
                }
            }
            if let Target::WeatherResult(index) = self.focus {
                match direction {
                    Direction::Left => self.close_subpage(),
                    Direction::Up => self.focus(
                        if index == 0 {
                            Target::WeatherQuery
                        } else {
                            Target::WeatherResult(index - 1)
                        },
                        controls,
                    ),
                    Direction::Down if index + 1 < self.weather.results.len() => {
                        self.focus(Target::WeatherResult(index + 1), controls)
                    }
                    _ => {}
                }
                return vec![];
            }
            if self.focus == Target::Search {
                match direction {
                    Direction::Left => {
                        self.focus(
                            Target::Filter(if self.hidden_count() > 0 { 5 } else { 4 }),
                            controls,
                        );
                        return vec![];
                    }
                    Direction::Up => {
                        self.focus(Target::Nav(1), controls);
                        return vec![];
                    }
                    _ => {}
                }
            }
            if self.ha_detail.is_some() || self.ha_camera.is_some() {
                if self.ha_detail.is_some() && matches!(direction, Direction::Up | Direction::Down)
                {
                    let delta = if matches!(direction, Direction::Up) {
                        10i8
                    } else {
                        -10i8
                    };
                    let id = self.ha_detail.as_deref().unwrap_or("").to_owned();
                    if let Some(entity) = self.ha_entities.iter_mut().find(|e| e.id == id)
                        && entity.domain == "light"
                        && self.ha_online
                    {
                        let value = (entity.brightness.unwrap_or(50) as i16 + delta as i16)
                            .clamp(1, 100) as u8;
                        entity.brightness = Some(value);
                        return vec![Effect::HomeAssistant(crate::smart_home::Command::Call {
                            entity_id: id.clone(),
                            service: "turn_on".into(),
                            data: serde_json::json!({
                                "entity_id": id, "brightness_pct": value
                            }),
                        })];
                    }
                }
                return vec![];
            }
            if let Target::Nav(index) = self.focus {
                let next = match direction {
                    Direction::Left if index > 0 => Some(index - 1),
                    Direction::Right if index < self.settings_nav_index() => Some(index + 1),
                    _ => None,
                };
                if let Some(next) = next {
                    self.open(next);
                    return vec![];
                }
                // The leftmost tab is a wall; only Down leaves the navigation.
                if matches!(direction, Direction::Left) {
                    return vec![];
                }
            }
            if self.page == Page::Apps
                && let Target::App(id) = &self.focus
                && matches!(direction, Direction::Up | Direction::Down)
            {
                let apps = self.apps();
                if let Some(index) = apps.iter().position(|app| &app.id == id) {
                    // A preceding row may be above the visible scroll region.
                    // Its grid index still defines the route; reveal_focus then
                    // scrolls it into view, without letting the fixed nav steal Up.
                    let next = match direction {
                        Direction::Up if index < APPS_COLUMNS => Some(Target::Filter(self.filter)),
                        Direction::Up => Some(Target::App(apps[index - APPS_COLUMNS].id.clone())),
                        Direction::Down
                            if (index / APPS_COLUMNS + 1) * APPS_COLUMNS < apps.len() =>
                        {
                            Some(Target::App(
                                apps[(index + APPS_COLUMNS).min(apps.len() - 1)].id.clone(),
                            ))
                        }
                        _ => None,
                    };
                    if let Some(next) = next {
                        self.focus(next, controls);
                    }
                    return vec![];
                }
            }
            if let Target::Background(index) = self.focus {
                match direction {
                    Direction::Right if index + 1 < BACKGROUNDS.len() => {
                        self.focus(Target::Background(index + 1), controls);
                        return vec![];
                    }
                    Direction::Left if index % 3 != 0 => {
                        self.focus(Target::Background(index - 1), controls);
                        return vec![];
                    }
                    _ => {}
                }
            }
            if matches!(self.focus, Target::Nav(_)) && matches!(direction, Direction::Down) {
                if self.page == Page::Settings {
                    self.subpage = None;
                    self.section = 0;
                }
                self.focus(self.first_content(), controls);
                return vec![];
            }
            if self.page == Page::Settings
                && matches!(self.subpage, Some(Subpage::SmartSelect(_)))
                && matches!(direction, Direction::Left)
            {
                self.close_subpage();
                return vec![];
            }
            // A left-edge press returns to the preceding level instead of
            // focusing controls from the hidden root screen.
            if self.page == Page::Settings
                && self.subpage.is_none()
                && self.settings_level != SettingsLevel::Root
                && matches!(direction, Direction::Left)
                && !matches!(self.focus, Target::Nav(_))
            {
                return self.back();
            }
        }
        let Some(current) = controls.iter().find(|c| c.target == self.focus) else {
            self.focus(self.first_content(), controls);
            return vec![];
        };
        let (x, y) = current.rect.center();
        let next = controls
            .iter()
            .filter(|candidate| candidate.target != self.focus)
            .filter(|candidate| {
                self.page != Page::Home
                    || !matches!(candidate.target, Target::App(_))
                    || (candidate.rect.x + candidate.rect.w > HOME_VIEWPORT_LEFT
                        && candidate.rect.x < HOME_VIEWPORT_RIGHT)
            })
            .filter_map(|candidate| {
                // Header controls never steal an Up route from the active tab.
                if self.overlay.is_none()
                    && matches!(direction, Direction::Up)
                    && candidate.rect.y < 150.
                    && candidate.target != Target::Nav(self.page_index())
                {
                    return None;
                }
                let (cx, cy) = candidate.rect.center();
                let (primary, cross, overlap) = match direction {
                    Direction::Up if self.page == Page::SmartHome && cy < y => (
                        (current.rect.y - (candidate.rect.y + candidate.rect.h)).max(2.),
                        (x - cx).abs(),
                        (current.rect.w + candidate.rect.w) / 2.,
                    ),
                    Direction::Up => (
                        y - cy,
                        (x - cx).abs(),
                        (current.rect.w + candidate.rect.w) / 2.,
                    ),
                    Direction::Down if self.page == Page::SmartHome && cy > y => (
                        (candidate.rect.y - (current.rect.y + current.rect.h)).max(2.),
                        (x - cx).abs(),
                        (current.rect.w + candidate.rect.w) / 2.,
                    ),
                    Direction::Down => (
                        cy - y,
                        (x - cx).abs(),
                        (current.rect.w + candidate.rect.w) / 2.,
                    ),
                    Direction::Left => (
                        x - cx,
                        (y - cy).abs(),
                        (current.rect.h + candidate.rect.h) / 2.,
                    ),
                    Direction::Right => (
                        cx - x,
                        (y - cy).abs(),
                        (current.rect.h + candidate.rect.h) / 2.,
                    ),
                };
                (primary > 1.).then_some((
                    candidate.target.clone(),
                    primary + cross * 2. + if cross >= overlap { 10000. } else { 0. },
                ))
            })
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(target, _)| target);
        if let Some(next) = next {
            self.focus(next, controls);
        }
        vec![]
    }

    pub fn options(&mut self) {
        if self.overlay.is_some() || self.moving.is_some() {
            return;
        }
        if let Target::App(id) = &self.focus {
            self.overlay = Some(Overlay::Options {
                id: id.clone(),
                restore: self.focus.clone(),
            });
            self.focus = Target::Option(0);
            if !self.preferences.reduced_motion {
                self.panel_started = Some(Instant::now());
            }
            self.dirty = true;
        }
    }

    pub fn hold(&mut self) -> Vec<Effect> {
        if self.widget_hold() {
            return vec![];
        }
        if self.page == Page::Settings
            && self.settings_level == SettingsLevel::WallpaperVideos
            && let Target::VideoGroup(group) = self.focus
            && let Some((_, _, items)) = crate::video_catalog::groups(0).get(group)
        {
            let all = items.iter().all(|&i| {
                !self
                    .preferences
                    .video_excluded
                    .contains(&crate::video_catalog::all()[i].id)
            });
            for &index in items {
                let id = &crate::video_catalog::all()[index].id;
                if all {
                    self.preferences.video_excluded.insert(id.clone());
                } else {
                    self.preferences.video_excluded.remove(id);
                }
            }
            self.dirty = true;
            return vec![Effect::Save];
        }
        if let Target::SmartCamera(id) = &self.focus {
            self.ha_camera_menu = Some(id.clone());
            self.focus = Target::SmartCameraHide;
            self.dirty = true;
            return vec![];
        }
        if let Target::SmartEntity(id) = &self.focus {
            self.ha_detail = Some(id.clone());
            self.focus = Target::SmartBrightness(0);
            self.dirty = true;
            return vec![];
        }
        if self.page == Page::Settings
            && self.section == 1
            && matches!(self.settings_level, SettingsLevel::WallpaperCategory(_))
            && let Target::VideoPick(index) = self.focus
            && index < crate::video_catalog::all().len()
        {
            let id = &crate::video_catalog::all()[index].id;
            self.preferences.video_excluded.remove(id);
            self.preferences.video_index = index;
            if self.preferences.background != "video" {
                self.photo_background_backup = self.preferences.background.clone();
            }
            self.preferences.background = "video".into();
            self.video_frame_ready = false;
            self.dirty = true;
            return vec![Effect::Save];
        }
        if self.page == Page::Home && self.overlay.is_none() {
            if let Target::App(id) = &self.focus {
                self.moving = Some(id.clone());
                self.notice = None;
                self.dirty = true;
            }
        } else {
            self.options();
        }
        vec![]
    }

    pub fn text(&mut self, text: &str) {
        if self.text_input_active() {
            if self.keyboard_field == TextField::HomeAddress {
                self.ha_address_draft
                    .extend(text.chars().filter(|c| !c.is_control()).take(200));
                self.dirty = true;
                return;
            }
            if self.keyboard_field == TextField::MoonlightAddress {
                self.moonlight_ip_draft.extend(
                    text.chars()
                        .filter(|c| c.is_ascii_digit() || *c == '.')
                        .take(15),
                );
                self.dirty = true;
                return;
            }
            if self.keyboard_field == TextField::Library {
                if let Some(library) = self.library.as_mut() {
                    let room = 80usize.saturating_sub(library.draft.chars().count());
                    library
                        .draft
                        .extend(text.chars().filter(|c| !c.is_control()).take(room));
                }
                self.library_typed();
                self.dirty = true;
                return;
            }
            if self.keyboard_field == TextField::Location {
                let mut query = self.weather.query.clone();
                query.extend(
                    text.chars()
                        .filter(|c| !c.is_control())
                        .take(120usize.saturating_sub(query.chars().count())),
                );
                self.weather.edit(query);
                self.dirty = true;
                return;
            }
            if self.keyboard_field == TextField::Name {
                let name = &mut self.greeting_name_draft;
                name.extend(
                    text.chars()
                        .filter(|c| !c.is_control())
                        .take(40usize.saturating_sub(name.chars().count())),
                );
                self.dirty = true;
                return;
            }
            if self.keyboard_field == TextField::InputName {
                let length = self.input_name.chars().count();
                self.input_name.extend(
                    text.chars()
                        .filter(|c| !c.is_control())
                        .take(INPUT_NAME_LIMIT.saturating_sub(length)),
                );
                self.dirty = true;
                return;
            }
            self.query.extend(
                text.chars()
                    .filter(|c| !c.is_control())
                    .take(120usize.saturating_sub(self.query.chars().count())),
            );
            self.scroll = 0.;
            self.dirty = true;
        }
    }

    pub fn delete(&mut self) {
        if self.text_input_active() {
            if self.keyboard_field == TextField::HomeAddress {
                self.ha_address_draft.pop();
            } else if self.keyboard_field == TextField::MoonlightAddress {
                self.moonlight_ip_draft.pop();
            } else if self.keyboard_field == TextField::Library {
                if let Some(library) = self.library.as_mut() {
                    library.draft.pop();
                }
                self.library_typed();
            } else if self.keyboard_field == TextField::Location {
                let mut query = self.weather.query.clone();
                query.pop();
                self.weather.edit(query);
            } else if self.keyboard_field == TextField::Name {
                self.greeting_name_draft.pop();
            } else if self.keyboard_field == TextField::InputName {
                self.input_name.pop();
            } else {
                self.query.pop();
            }
            self.dirty = true;
        }
    }

    pub fn text_input_active(&self) -> bool {
        matches!(
            self.overlay,
            Some(Overlay::Keyboard | Overlay::SystemKeyboard)
        )
    }

    pub fn finish_text_input(&mut self) -> Vec<Effect> {
        if self.text_input_active() {
            self.overlay = None;
            self.panel_started = None;
            self.focus = self.keyboard_return();
            self.pointer = false;
            self.dirty = true;
            return self.commit_text_input();
        }
        vec![]
    }

    fn commit_text_input(&mut self) -> Vec<Effect> {
        if self.keyboard_field == TextField::Library {
            self.library_commit_search();
            return vec![];
        }
        if self.keyboard_field == TextField::MoonlightAddress {
            let ip = std::mem::take(&mut self.moonlight_ip_draft);
            if !ip.is_empty() {
                self.moonlight_status = Some("Checking PC address…".into());
                return vec![Effect::MoonlightProbe(ip)];
            }
            return vec![];
        }
        if self.keyboard_field == TextField::HomeAddress {
            return vec![Effect::HomeAssistant(
                crate::smart_home::Command::BeginPairing {
                    server: self.ha_address_draft.clone(),
                },
            )];
        }
        if self.keyboard_field == TextField::Name {
            let name = std::mem::take(&mut self.greeting_name_draft);
            let name = name.trim();
            // Empty dismissal preserves the saved value; Clear name is explicit.
            if !name.is_empty() && self.preferences.greeting_name != name {
                self.preferences.greeting_name = name.into();
                return vec![Effect::Save];
            }
            return vec![];
        }
        self.commit_input_name()
    }

    /// Closing the keyboard by any route keeps a typed input name. An empty
    /// field leaves the saved name unchanged; Clear name removes it explicitly.
    fn commit_input_name(&mut self) -> Vec<Effect> {
        let name = std::mem::take(&mut self.input_name);
        if self.keyboard_field != TextField::InputName || clean_input_name(&name).is_empty() {
            return vec![];
        }
        let Some(Subpage::Input(id)) = self.subpage.clone() else {
            return vec![];
        };
        if self.preferences.set_input_name(&id, &name) {
            vec![Effect::Save]
        } else {
            vec![]
        }
    }

    pub fn delete_surrounding_text(&mut self, offset: i32, length: i32) {
        if !self.text_input_active() || length <= 0 {
            return;
        }
        // SDL-webOS forwards the native text model's UTF-8 byte offsets,
        // relative to the insertion cursor (at the end of our query).
        let mut query = self.editing_text().to_owned();
        let mut start =
            (query.len() as i64 + i64::from(offset)).clamp(0, query.len() as i64) as usize;
        let mut end = start.saturating_add(length as usize).min(query.len());
        while !query.is_char_boundary(start) {
            start -= 1;
        }
        while !query.is_char_boundary(end) {
            end += 1;
        }
        query.replace_range(start..end, "");
        if self.keyboard_field == TextField::Location {
            self.weather.edit(query);
        } else if self.keyboard_field == TextField::Name {
            self.greeting_name_draft = query;
        } else if self.keyboard_field == TextField::InputName {
            self.input_name = query;
        } else if self.keyboard_field == TextField::MoonlightAddress {
            self.moonlight_ip_draft = query;
        } else if self.keyboard_field == TextField::Library {
            if let Some(library) = self.library.as_mut() {
                library.draft = query;
            }
            self.library_typed();
        } else {
            self.query = query;
            self.scroll = 0.;
        }
        self.dirty = true;
    }

    pub fn editing_text(&self) -> &str {
        match self.keyboard_field {
            TextField::HomeAddress => &self.ha_address_draft,
            TextField::MoonlightAddress => &self.moonlight_ip_draft,
            TextField::Location => &self.weather.query,
            TextField::Name => &self.greeting_name_draft,
            TextField::InputName => &self.input_name,
            TextField::Apps => &self.query,
            TextField::Library => self.library.as_ref().map_or("", |l| l.draft.as_str()),
        }
    }
    pub fn open_keyboard(&mut self, field: TextField) {
        self.keyboard_field = field;
        if field == TextField::Name {
            // SDL-webOS cannot pre-fill the system keyboard's text model.
            self.greeting_name_draft.clear();
        }
        if field == TextField::Library
            && let Some(library) = self.library.as_mut()
        {
            // The TV keyboard starts empty, so a new search replaces the old one.
            library.draft.clear();
        }
        if field == TextField::InputName {
            // The TV keyboard starts empty, so a new name replaces the old one.
            self.input_name.clear();
        }
        if cfg!(all(
            feature = "webos",
            target_os = "linux",
            target_arch = "arm"
        )) {
            self.overlay = Some(Overlay::SystemKeyboard);
            self.focus = match field {
                TextField::HomeAddress => Target::SmartAddress,
                TextField::MoonlightAddress => Target::MoonlightHost(self.moonlight_hosts.len()),
                TextField::Location => Target::WeatherQuery,
                TextField::Name => Target::GreetingName,
                TextField::InputName => Target::InputRename,
                TextField::Apps => Target::Search,
                TextField::Library => Target::LibSearch,
            };
            self.panel_started = None;
            self.dirty = true;
            return;
        }
        self.overlay = Some(Overlay::Keyboard);
        self.focus = Target::Key(0);
        if !self.preferences.reduced_motion {
            self.panel_started = Some(Instant::now());
        }
        self.dirty = true;
    }
    fn open_location(&mut self) {
        self.weather.edit(String::new());
        self.subpage = Some(Subpage::WeatherLocation);
        self.focus = Target::WeatherQuery;
        self.scroll = 0.;
        if !self.preferences.reduced_motion {
            self.panel_started = Some(Instant::now());
        }
        self.dirty = true;
    }
    fn keyboard_return(&self) -> Target {
        match self.keyboard_field {
            TextField::HomeAddress => Target::SmartAddress,
            TextField::MoonlightAddress => Target::MoonlightHost(self.moonlight_hosts.len()),
            TextField::Location => Target::WeatherSearch,
            TextField::Name => Target::GreetingName,
            TextField::InputName => Target::InputRename,
            TextField::Apps => Target::Search,
            TextField::Library => Target::LibSearch,
        }
    }

    pub fn activate(&mut self) -> Vec<Effect> {
        if self.ambient_on() {
            self.leave_ambient();
            return vec![];
        }
        if self.page == Page::Home && self.focus == Target::VideoTitle && !self.ambient_on() {
            // The caption gives a short press, then the interface clears away.
            self.ambient = Some((true, Instant::now() + AMBIENT_PRESS));
            self.dirty = true;
            return vec![];
        }
        if let Some(effects) = self.mini_activate() {
            self.dirty = true;
            return effects;
        }
        if self.moving.take().is_some() {
            // Order was saved at every move. A fresh OK release finishes the
            // mode and keeps the selected favorite, rather than launching it.
            self.dirty = true;
            return vec![];
        }
        self.dirty = true;
        if let Some(playback) = self.playback.as_mut() {
            let previous_panel = playback.panel;
            let cmds = playback.activate(Some(&self.focus));
            if let Some(panel) = playback.panel {
                self.focus = match panel {
                    crate::player::Panel::Audio => Target::PlayerAudioOption(playback.selected_row),
                    crate::player::Panel::Subtitles => {
                        Target::PlayerSubtitleOption(playback.selected_row)
                    }
                    crate::player::Panel::Quality => Target::PlayerOption(playback.selected_row),
                    crate::player::Panel::Chapters => Target::PlayerChapter(playback.selected_row),
                };
            } else if previous_panel.is_some() && !cmds.contains(&crate::player::Command::Close) {
                self.focus = Target::PlayerPill(playback.selected_pill);
            }

            if cmds.contains(&crate::player::Command::Close) {
                if let Some(session) = self.playback.as_ref()
                    && let Some(id) = session.request.item_id
                {
                    self.preferences
                        .playback_progress
                        .insert(id.to_string(), session.position_ms);
                }
                self.playback = None;
                self.focus = Target::MoviePlay;
            }
            if cmds.contains(&crate::player::Command::Close) {
                let mut effects = vec![Effect::Save];
                effects.extend(cmds.into_iter().map(Effect::Player));
                return effects;
            }
            return cmds.into_iter().map(Effect::Player).collect();
        }
        if self.movie.is_some() {
            return self.movie_activate();
        }
        if self.album.is_some() {
            return self.album_activate();
        }
        if self.game.is_none()
            && self.overlay.is_none()
            && self.ha_camera.is_none()
            && let Some(effects) = self.widget_activate()
        {
            return effects;
        }
        if let Some(g) = self.game.as_mut()
            && g.shot.is_some()
        {
            g.shot = None;
            self.dirty = true;
            return vec![];
        }
        if let Some(Target::GameShot(n)) = self.game.as_ref().map(|_| self.focus.clone()) {
            if let Some(g) = self.game.as_mut() {
                g.shot = Some(n);
                self.dirty = true;
            }
            return vec![];
        }
        if let Some(game) = self.game.as_ref() {
            let id = game.id;
            match self.focus {
                Target::GameClose => {
                    self.close_game();
                    return vec![];
                }
                Target::GameWake => return vec![Effect::MoonlightWake],
                Target::GameQuit => return vec![Effect::MoonlightQuit],
                Target::GamePlay => {
                    self.preferences.games.record_play(id);
                    return vec![
                        Effect::Save,
                        Effect::MoonlightPlay(id),
                        Effect::MoonlightRefilter,
                    ];
                }
                Target::GameTrailer => return vec![Effect::GameTrailer],
                Target::GameFavorite => {
                    self.preferences.games.toggle_favorite(id);
                    return vec![Effect::Save, Effect::MoonlightRefilter];
                }
                _ => return vec![],
            }
        }
        if self.overlay.is_none()
            && self.game.is_none()
            && let Some(effects) = self.library_activate()
        {
            return effects;
        }
        match self.focus.clone() {
            Target::UiResolutionOpen => {
                let index = crate::model::UiResolution::ALL
                    .iter()
                    .position(|&r| r == self.preferences.ui_resolution)
                    .unwrap_or(0);
                self.settings_push(SettingsLevel::UiResolution, Target::UiResolutionPick(index));
                return vec![];
            }
            Target::StackRow(i) => {
                if i < self.preferences.stacks.len() {
                    self.settings_push(SettingsLevel::StackConfig(i), Target::StackOpt(0));
                } else {
                    self.settings_push(SettingsLevel::StackAdd, Target::StackSource(0));
                }
                return vec![];
            }
            Target::StackCard(row, index) => {
                let Some(stack) = crate::stacks::row_stack(self, row) else {
                    return vec![];
                };
                if index >= crate::stacks::row_shown(self, row)
                    && crate::stacks::has_library(&self.preferences.stacks[stack].source)
                {
                    return self.open_library(row, index);
                }
                let items = self.stack_items.get(stack).map_or(&[][..], Vec::as_slice);
                if self.preferences.stacks[stack].source == "moonlight" {
                    if let Some(item) = items.get(index).cloned() {
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
                            Target::StackCard(row, index),
                        ));
                        self.focus = Target::GamePlay;
                        return vec![Effect::GameInfo(item.id, item.title.clone())];
                    }
                    return vec![];
                }
                if let Some(item) = items.get(index).filter(|i| crate::jellyfin::is_album(i.id)) {
                    return self.open_album(item.clone(), Target::StackCard(row, index));
                }
                return match items.get(index) {
                    Some(item) if item.id > 0 => {
                        let (item, pool) = (item.clone(), items.to_vec());
                        self.open_movie(item, pool, Target::StackCard(row, index))
                    }
                    _ => vec![],
                };
            }
            Target::MoonlightHost(i) => {
                if let Some(host) = self.moonlight_hosts.get(i).cloned() {
                    self.moonlight_pin = None;
                    self.moonlight_retry_host = Some(host.clone());
                    self.moonlight_pair_error = false;
                    self.moonlight_status = Some("Waiting for Sunshine PIN approval…".into());
                    // Never focus Back when entering PIN: a repeated OK would
                    // immediately cancel pairing and return to host list.
                    self.settings_push(SettingsLevel::MoonlightPin, Target::MoonlightCancelButton);
                    return vec![Effect::MoonlightPair(host)];
                }
                if i == self.moonlight_hosts.len() {
                    self.open_keyboard(TextField::MoonlightAddress);
                    return vec![];
                }
                self.moonlight_hosts.clear();
                self.moonlight_discovery_start = Some(Instant::now());
                self.moonlight_status = Some("Searching for PCs…".into());
                self.focus = Target::MoonlightHost(0);
                return vec![Effect::MoonlightDiscover(
                    self.preferences.moonlight.address.clone(),
                )];
            }
            Target::MoonlightCancelButton => {
                // A second remote OK from the preceding host selection must not
                // cancel the newly opened pairing sheet during its entrance.
                if self
                    .panel_started
                    .is_some_and(|t| t.elapsed() < Duration::from_millis(1000))
                {
                    return vec![];
                }
                return self.back();
            }
            Target::MoonlightRetry => {
                if let Some(host) = self.moonlight_retry_host.clone() {
                    self.moonlight_pin = None;
                    self.moonlight_pair_error = false;
                    self.moonlight_status = Some("Waiting for Sunshine PIN approval…".into());
                    self.focus = Target::MoonlightCancelButton;
                    return vec![Effect::MoonlightPair(host)];
                }
                return vec![];
            }
            Target::JellyfinServer(i) => {
                if let Some(server) = self.jellyfin_servers.get(i) {
                    let address = server.address.clone();
                    return self.jellyfin_connect(address);
                }
                self.jellyfin_phone = true;
                self.jellyfin_code = None;
                self.jellyfin_error = false;
                self.jellyfin_status = Some("Scan the QR and type the server address".into());
                return vec![Effect::JellyfinCancel, Effect::JellyfinAddress];
            }
            Target::JellyfinNewCode => {
                if let Some(address) = self.jellyfin_selected.clone() {
                    return self.jellyfin_connect(address);
                }
                return vec![Effect::JellyfinDiscover];
            }
            Target::JellyfinCancel | Target::PlexCancel => return self.back(),
            Target::PlexNewCode => {
                self.plex_code = None;
                self.plex_error = false;
                self.plex_status = Some("Getting a code…".into());
                return vec![Effect::PlexCancel, Effect::PlexLink];
            }
            Target::StackSource(index) => {
                let Some(source) = crate::stacks::SOURCES.get(index) else {
                    return vec![];
                };
                if !source.2 {
                    self.notify(format!("{} stacks are coming soon", source.1));
                    return vec![];
                }
                if source.0 == "moonlight" && self.preferences.moonlight.paired {
                    let first = !self
                        .preferences
                        .stacks
                        .iter()
                        .any(|s| s.source == "moonlight");
                    let mut stack = crate::stacks::StackConfig {
                        source: "moonlight".into(),
                        collection: if first { "all" } else { "recent" }.into(),
                        ..Default::default()
                    };
                    stack.name = crate::stacks::row_title(&stack);
                    self.preferences.stacks.push(stack);
                    self.stack_items.push(vec![]);
                    let i = self.preferences.stacks.len() - 1;
                    self.settings_level = SettingsLevel::Section;
                    self.settings_push(SettingsLevel::StackConfig(i), Target::StackOpt(0));
                    return vec![Effect::Save, Effect::MoonlightRefilter];
                }
                if source.0 == "moonlight" {
                    self.moonlight_hosts.clear();
                    self.moonlight_discovery_start = Some(Instant::now());
                    self.moonlight_status = Some("Searching for PCs…".into());
                    self.settings_push(SettingsLevel::MoonlightHosts, Target::MoonlightHost(0));
                    return vec![Effect::MoonlightDiscover(
                        self.preferences.moonlight.address.clone(),
                    )];
                }
                if source.0 == "plex" && !self.preferences.plex.linked() {
                    self.plex_code = None;
                    self.plex_error = false;
                    self.plex_qr.clear();
                    self.plex_status = Some("Getting a code…".into());
                    self.settings_push(SettingsLevel::PlexLink, Target::PlexCancel);
                    return vec![Effect::PlexLink];
                }
                if source.0 == "plex" {
                    let mut stack = crate::stacks::StackConfig {
                        source: "plex".into(),
                        collection: "ondeck".into(),
                        ..Default::default()
                    };
                    stack.name = crate::stacks::row_title(&stack);
                    self.preferences.stacks.push(stack);
                    self.stack_items.push(vec![]);
                    let i = self.preferences.stacks.len() - 1;
                    self.settings_level = SettingsLevel::Section;
                    self.settings_push(SettingsLevel::StackConfig(i), Target::StackOpt(0));
                    return vec![Effect::Save, Effect::StacksRefresh, Effect::PlexViews];
                }
                if source.0 == "jellyfin" && !self.preferences.jellyfin.linked() {
                    self.jellyfin_reset();
                    self.jellyfin_status = Some("Searching for servers…".into());
                    self.settings_push(SettingsLevel::JellyfinLink, Target::JellyfinServer(0));
                    return vec![Effect::JellyfinDiscover];
                }
                if source.0 == "jellyfin" {
                    let mut stack = crate::stacks::StackConfig {
                        source: "jellyfin".into(),
                        collection: "resume".into(),
                        ..Default::default()
                    };
                    stack.name = crate::stacks::row_title(&stack);
                    self.preferences.stacks.push(stack);
                    self.stack_items.push(vec![]);
                    let i = self.preferences.stacks.len() - 1;
                    self.settings_level = SettingsLevel::Section;
                    self.settings_push(SettingsLevel::StackConfig(i), Target::StackOpt(0));
                    return vec![Effect::Save, Effect::StacksRefresh, Effect::JellyfinViews];
                }
                self.preferences.stacks.push(crate::stacks::StackConfig {
                    source: source.0.into(),
                    ..Default::default()
                });
                self.stack_items.push(vec![]);
                let i = self.preferences.stacks.len() - 1;
                self.settings_level = SettingsLevel::Section;
                self.settings_push(SettingsLevel::StackConfig(i), Target::StackOpt(0));
                return vec![Effect::Save, Effect::StacksRefresh];
            }
            Target::StackOpt(option) => {
                let SettingsLevel::StackConfig(i) = self.settings_level else {
                    return vec![];
                };
                let len = self.preferences.stacks.len();
                if i >= len {
                    return vec![];
                }
                if crate::stacks::compact(&self.preferences.stacks[i].source) {
                    let jellyfin = self.preferences.stacks[i].source == "jellyfin";
                    let plex = self.preferences.stacks[i].source == "plex";
                    match option {
                        0 => {
                            let pick = crate::stacks::collection_choices(self, i)
                                .iter()
                                .position(|c| c.0 == self.preferences.stacks[i].collection)
                                .unwrap_or(if jellyfin || plex { 0 } else { 2 });
                            self.settings_push(
                                SettingsLevel::StackCollection(i),
                                Target::StackPick(pick),
                            );
                            return if jellyfin {
                                vec![Effect::JellyfinViews]
                            } else if plex {
                                vec![Effect::PlexViews]
                            } else {
                                vec![]
                            };
                        }
                        1 => {
                            let pick = self.preferences.stacks[i].shape.index();
                            self.settings_push(
                                SettingsLevel::StackShape(i),
                                Target::StackPick(pick),
                            );
                            return vec![];
                        }
                        2 => self.preferences.stacks[i].show = !self.preferences.stacks[i].show,
                        3 if i > 0 => {
                            self.preferences.stacks.swap(i, i - 1);
                            self.stack_items.swap(i, i - 1);
                            self.settings_level = SettingsLevel::StackConfig(i - 1);
                        }
                        4 if i + 1 < len => {
                            self.preferences.stacks.swap(i, i + 1);
                            self.stack_items.swap(i, i + 1);
                            self.settings_level = SettingsLevel::StackConfig(i + 1);
                        }
                        5 if plex => {
                            self.preferences.plex = Default::default();
                            self.plex_views.clear();
                            for (j, s) in self.preferences.stacks.iter().enumerate() {
                                if s.source == "plex"
                                    && let Some(v) = self.stack_items.get_mut(j)
                                {
                                    v.clear();
                                }
                            }
                            self.notify("Signed out of Plex");
                        }
                        5 if jellyfin => {
                            self.preferences.jellyfin = Default::default();
                            self.jellyfin_views.clear();
                            for (j, s) in self.preferences.stacks.iter().enumerate() {
                                if s.source == "jellyfin"
                                    && let Some(v) = self.stack_items.get_mut(j)
                                {
                                    v.clear();
                                }
                            }
                            self.notify("Signed out of Jellyfin");
                        }
                        5 => {
                            self.preferences.moonlight = Default::default();
                            for (j, s) in self.preferences.stacks.iter().enumerate() {
                                if s.source == "moonlight"
                                    && let Some(v) = self.stack_items.get_mut(j)
                                {
                                    v.clear();
                                }
                            }
                            self.moonlight_games.clear();
                            self.moonlight_status = Some("Luma unpaired".into());
                        }
                        6 => {
                            self.preferences.stacks.remove(i);
                            if i < self.stack_items.len() {
                                self.stack_items.remove(i);
                            }
                            self.settings_pop(
                                SettingsLevel::Section,
                                Target::StackRow(i.min(self.preferences.stacks.len())),
                            );
                        }
                        _ => return vec![],
                    }
                    return vec![Effect::Save];
                }
                match option {
                    0 => {
                        let pick = crate::stacks::collection_choices(self, i)
                            .iter()
                            .position(|c| c.0 == self.preferences.stacks[i].collection)
                            .unwrap_or(0);
                        self.settings_push(
                            SettingsLevel::StackCollection(i),
                            Target::StackPick(pick),
                        );
                        return vec![];
                    }
                    2 => {
                        let pick = self.preferences.stacks[i].shape.index();
                        self.settings_push(SettingsLevel::StackShape(i), Target::StackPick(pick));
                        return vec![];
                    }
                    3 => self.preferences.stacks[i].show = !self.preferences.stacks[i].show,
                    4 if i > 0 => {
                        self.preferences.stacks.swap(i, i - 1);
                        if self.stack_items.len() > i {
                            self.stack_items.swap(i, i - 1);
                        }
                        self.settings_level = SettingsLevel::StackConfig(i - 1);
                    }
                    5 if i + 1 < len => {
                        self.preferences.stacks.swap(i, i + 1);
                        if self.stack_items.len() > i + 1 {
                            self.stack_items.swap(i, i + 1);
                        }
                        self.settings_level = SettingsLevel::StackConfig(i + 1);
                    }
                    6 => {
                        self.preferences.stacks.remove(i);
                        if self.stack_items.len() > i {
                            self.stack_items.remove(i);
                        }
                        self.settings_pop(
                            SettingsLevel::Section,
                            Target::StackRow(i.min(self.preferences.stacks.len())),
                        );
                    }
                    _ => return vec![],
                }
                return vec![Effect::Save];
            }
            Target::StackPick(pick) => {
                match self.settings_level {
                    SettingsLevel::StackCollection(i) => {
                        let choices = crate::stacks::collection_choices(self, i);
                        if let (Some(stack), Some((id, label))) =
                            (self.preferences.stacks.get_mut(i), choices.get(pick))
                        {
                            stack.collection = id.clone();
                            stack.list_title = label.clone();
                            stack.name = crate::stacks::row_title(stack);
                            // Albums are square; leaving music goes back to posters.
                            if stack.source == "jellyfin" {
                                let music = crate::jellyfin::music_collection(&stack.collection);
                                if music {
                                    stack.shape = crate::stacks::CardShape::Square;
                                } else if stack.shape == crate::stacks::CardShape::Square {
                                    stack.shape = crate::stacks::CardShape::Poster;
                                }
                            }
                            if let Some(items) = self.stack_items.get_mut(i) {
                                items.clear();
                            }
                        }
                        self.settings_pop(SettingsLevel::StackConfig(i), Target::StackOpt(0));
                        return vec![Effect::Save, Effect::StacksRefresh];
                    }
                    SettingsLevel::StackShape(i) => {
                        if let (Some(stack), Some(&shape)) = (
                            self.preferences.stacks.get_mut(i),
                            crate::stacks::CardShape::ALL.get(pick),
                        ) {
                            stack.shape = shape;
                        }
                        self.settings_pop(
                            SettingsLevel::StackConfig(i),
                            Target::StackOpt(
                                if crate::stacks::compact(&self.preferences.stacks[i].source) {
                                    1
                                } else {
                                    2
                                },
                            ),
                        );
                        return vec![Effect::Save];
                    }
                    _ => {}
                }
                return vec![];
            }
            Target::UiResolutionPick(index) => {
                if let Some(&choice) = crate::model::UiResolution::ALL.get(index) {
                    self.preferences.ui_resolution = choice;
                    return vec![Effect::Save];
                }
            }
            Target::HeadingFontOpen => {
                let i = crate::typography::HeadingFont::ALL
                    .iter()
                    .position(|&f| f == self.preferences.heading_font)
                    .unwrap_or(0);
                self.settings_push(SettingsLevel::HeadingFonts, Target::HeadingFontPick(i));
                return vec![];
            }
            Target::TextFontOpen => {
                let i = crate::typography::TextFont::ALL
                    .iter()
                    .position(|&f| f == self.preferences.text_font)
                    .unwrap_or(0);
                self.settings_push(SettingsLevel::TextFonts, Target::TextFontPick(i));
                return vec![];
            }
            Target::HeadingFontPick(i) => {
                if let Some(&font) = crate::typography::HeadingFont::ALL.get(i) {
                    self.preferences.heading_font = font;
                    return vec![Effect::Save];
                }
            }
            Target::TextFontPick(i) => {
                if let Some(&font) = crate::typography::TextFont::ALL.get(i) {
                    self.preferences.text_font = font;
                    return vec![Effect::Save];
                }
            }
            Target::HeadingSize | Target::TextSize => {
                if self.focus == Target::HeadingSize {
                    self.preferences.heading_size_pct = if self.preferences.heading_size_pct >= 170
                    {
                        85
                    } else {
                        self.preferences.heading_size_pct + 5
                    };
                } else {
                    self.preferences.text_size_pct = if self.preferences.text_size_pct >= 150 {
                        90
                    } else {
                        self.preferences.text_size_pct + 5
                    };
                }
                return vec![Effect::Save];
            }
            Target::UiSounds => {
                self.preferences.ui_sounds = !self.preferences.ui_sounds;
                return vec![Effect::Save];
            }
            Target::Screensaver => {
                let steps = SCREENSAVER_STEPS;
                let now = self.preferences.screensaver_minutes;
                let at = steps.iter().position(|&m| m == now).unwrap_or(0);
                self.preferences.screensaver_minutes = steps[(at + 1) % steps.len()];
                return vec![Effect::Save];
            }
            Target::PhraseTitleSize | Target::PhraseSubtitleSize => {
                let value = if self.focus == Target::PhraseTitleSize {
                    &mut self.preferences.phrase_title_pct
                } else {
                    &mut self.preferences.phrase_subtitle_pct
                };
                *value = if *value >= crate::model::PHRASE_PCT_MAX {
                    crate::model::PHRASE_PCT_MIN
                } else {
                    *value + 5
                };
                return vec![Effect::Save];
            }
            Target::SettingsBack => return self.back(),
            Target::AccentChoice(index) => {
                if index < 16 {
                    let previous = self.preferences.accent.colors();
                    self.preferences.accent.choose(index);
                    if previous != self.preferences.accent.colors() {
                        self.accent_transition = Some((previous.0, previous.1, Instant::now()));
                    }
                    return vec![Effect::Save];
                }
            }
            Target::AccentOption(index) => {
                match index {
                    0 => self.preferences.accent.use_focus = !self.preferences.accent.use_focus,
                    1 => self.preferences.accent.use_nav = !self.preferences.accent.use_nav,
                    2 => {
                        self.preferences.accent.use_headline = !self.preferences.accent.use_headline
                    }
                    _ => return vec![],
                }
                return vec![Effect::Save];
            }
            Target::VideoSelectAll(group) => {
                let groups = crate::video_catalog::groups(0);
                let Some((_, _, items)) = groups.get(group) else {
                    return vec![];
                };
                let all = items.iter().all(|&i| {
                    !self
                        .preferences
                        .video_excluded
                        .contains(&crate::video_catalog::all()[i].id)
                });
                for &index in items {
                    let id = &crate::video_catalog::all()[index].id;
                    if all {
                        self.preferences.video_excluded.insert(id.clone());
                    } else {
                        self.preferences.video_excluded.remove(id);
                    }
                }
                return vec![Effect::Save];
            }
            Target::Nav(index) => self.open(index),
            Target::SmartConnect => {
                if self.page != Page::Settings {
                    self.open_settings();
                }
                self.settings_level = SettingsLevel::Section;
                self.section = SMARTHOME_SECTION;
                self.subpage = Some(Subpage::SmartConnect);
                self.ha_address_draft = self.preferences.smart_home.server_url.clone();
                self.ha_discovery_since = Instant::now();
                self.ha_token_qr = false;
                self.ha_auth_status = "Starting phone sign-in…".into();
                self.ha_qr.clear();
                self.ha_fallback_qr.clear();
                self.focus = Target::SmartAddress;
                return vec![Effect::HomeAssistant(
                    crate::smart_home::Command::BeginPairing {
                        server: self.ha_address_draft.clone(),
                    },
                )];
            }
            Target::SmartServer(index) => {
                if let Some(server) = self.ha_servers.get(index) {
                    self.ha_address_draft = server.url.clone();
                    self.focus = Target::SmartRetry;
                    self.ha_auth_status = "Starting phone sign-in…".into();
                    return vec![Effect::HomeAssistant(
                        crate::smart_home::Command::BeginPairing {
                            server: self.ha_address_draft.clone(),
                        },
                    )];
                }
            }
            Target::SmartAddress => self.open_keyboard(TextField::HomeAddress),
            Target::SmartFallback => self.ha_token_qr = !self.ha_token_qr,
            Target::SmartRetry => {
                self.ha_auth_status = "Starting phone sign-in…".into();
                self.ha_qr.clear();
                self.ha_fallback_qr.clear();
                return vec![Effect::HomeAssistant(
                    crate::smart_home::Command::BeginPairing {
                        server: self.ha_address_draft.clone(),
                    },
                )];
            }
            Target::SmartSignOut => {
                if !self.ha_signout_armed {
                    self.ha_signout_armed = true;
                    self.notify("Press OK again to remove Home Assistant credentials");
                    return vec![];
                }
                self.ha_signout_armed = false;
                self.preferences.smart_home.server_url.clear();
                self.preferences.smart_home.show_tab = false;
                self.ha_online = false;
                self.ha_entities.clear();
                self.focus = Target::SmartConnect;
                return vec![
                    Effect::Save,
                    Effect::HomeAssistant(crate::smart_home::Command::Disconnect),
                ];
            }
            Target::SmartClose if self.ha_camera_menu.is_some() => {
                self.focus = Target::SmartCamera(self.ha_camera_menu.take().unwrap_or_default());
            }
            Target::SmartCameraHide => {
                if let Some(id) = self.ha_camera_menu.take() {
                    let hidden = &mut self.preferences.smart_home.hidden_cameras;
                    if let Some(index) = hidden.iter().position(|camera| camera == &id) {
                        hidden.remove(index);
                    } else {
                        hidden.push(id.clone());
                    }
                    // Drop the current frame: the next one comes blurred (or sharp).
                    self.ha_snapshots.remove(&id);
                    self.ha_snapshot_failed.remove(&id);
                    self.focus = Target::SmartCamera(id);
                    self.dirty = true;
                    return vec![Effect::Save];
                }
            }
            Target::SmartClose => {
                self.ha_detail = None;
                self.ha_camera = None;
                if !self.widget_camera_closed() {
                    self.focus = self.first_content();
                }
            }
            Target::SmartCamera(id) => {
                self.ha_camera = Some(id);
                self.ha_camera_opened = Instant::now();
                self.focus = Target::SmartClose;
            }
            Target::SmartEntity(id) => {
                if let Some(entity) = self.ha_entities.iter_mut().find(|e| e.id == id)
                    && entity.actionable()
                    && self.ha_online
                {
                    let turn_on = !entity.on();
                    let service = if entity.domain == "cover" {
                        if turn_on { "open_cover" } else { "close_cover" }
                    } else if turn_on {
                        "turn_on"
                    } else {
                        "turn_off"
                    };
                    entity.state = if turn_on { "on".into() } else { "off".into() };
                    return vec![Effect::HomeAssistant(crate::smart_home::Command::Call {
                        entity_id: id.clone(),
                        service: service.into(),
                        data: serde_json::json!({ "entity_id": id }),
                    })];
                }
            }
            Target::SmartScene(id) => {
                if self.ha_online {
                    self.ha_scene_fired = Some((id.clone(), Instant::now()));
                    self.dirty = true;
                    return vec![Effect::HomeAssistant(crate::smart_home::Command::Call {
                        entity_id: id.clone(),
                        service: "turn_on".into(),
                        data: serde_json::json!({ "entity_id": id }),
                    })];
                }
            }
            Target::SmartBrightness(_) => {
                if let Some(id) = &self.ha_detail
                    && let Some(e) = self.ha_entities.iter().find(|e| &e.id == id)
                    && e.domain == "light"
                    && self.ha_online
                {
                    return vec![Effect::HomeAssistant(crate::smart_home::Command::Call {
                        entity_id: id.clone(),
                        service: if e.on() { "turn_off" } else { "turn_on" }.into(),
                        data: serde_json::json!({ "entity_id": id }),
                    })];
                }
            }
            Target::SmartSection(section) => {
                self.subpage = Some(Subpage::SmartSelect(section.min(2)));
                self.scroll = 0.;
                self.scroll_goal = 0.;
                self.focus = Target::SmartBulk(section.min(2), 0);
                self.panel_started = Some(Instant::now());
            }
            Target::SmartPick(section, id) => {
                if section > 2 {
                    return vec![];
                }
                let all: Vec<String> = self
                    .ha_listed(section)
                    .into_iter()
                    .filter(|e| e.useful())
                    .map(|e| e.id.clone())
                    .collect();
                let selected = match section {
                    0 => &mut self.preferences.smart_home.selected_entities,
                    1 => &mut self.preferences.smart_home.selected_cameras,
                    _ => &mut self.preferences.smart_home.selected_scenes,
                };
                // Empty selection means recommended. Materialize it on first edit.
                if selected.is_empty() {
                    *selected = all;
                }
                if let Some(index) = selected.iter().position(|entry| entry == &id) {
                    selected.remove(index);
                    // Empty has the legacy meaning "use defaults"; a sentinel
                    // represents an explicit selection of no entities.
                    if selected.is_empty() {
                        selected.push("__none__".into());
                    }
                } else {
                    selected.retain(|entry| entry != "__none__");
                    selected.push(id);
                }
                return vec![Effect::Save];
            }
            Target::SmartBulk(section, kind) => {
                let all = self.ha_selection_entries(section);
                let selected = match section {
                    0 => &mut self.preferences.smart_home.selected_entities,
                    1 => &mut self.preferences.smart_home.selected_cameras,
                    _ => &mut self.preferences.smart_home.selected_scenes,
                };
                *selected = match kind {
                    0 => Vec::new(),
                    1 if !all.is_empty() => all,
                    _ => vec!["__none__".into()],
                };
                return vec![Effect::Save];
            }
            Target::SmartShowTab => {
                self.preferences.smart_home.show_tab = !self.preferences.smart_home.show_tab;
                if !self.show_smart_tab() && self.page == Page::SmartHome {
                    self.open_settings();
                }
                return vec![Effect::Save];
            }
            Target::Browse => self.open(1),
            Target::Retry => return vec![Effect::Reload],
            Target::DismissNotice => self.notice = None,
            Target::CloseDialog => return self.back(),
            Target::TvSettings => return vec![Effect::Launch("com.palm.app.settings".into())],
            Target::App(id) | Target::Input(id) if !self.launching => {
                return vec![Effect::Launch(id)];
            }
            Target::Filter(index) => {
                // "All" (0) is a tab like the others: OK selects, never toggles off.
                self.filter = index;
                self.scroll = 0.;
            }
            Target::ClearSearch => {
                self.query.clear();
                self.focus = Target::Search;
                self.scroll = 0.;
            }
            Target::Search => {
                self.open_keyboard(TextField::Apps);
            }
            Target::WeatherToggle => {
                // On means a weather widget is really on Home: turning it on
                // brings one back if it was removed in edit mode.
                if self.weather_widget_shown() {
                    self.preferences.show_weather = false;
                } else {
                    self.preferences.show_weather = true;
                    self.restore_weather_widget();
                }
                self.weather.invalidate_forecast();
                return vec![Effect::Save];
            }
            Target::WeatherLocation => self.open_location(),
            Target::WeatherTemperature => {
                self.subpage = Some(Subpage::Temperature);
                self.focus = Target::Choice(usize::from(
                    self.preferences.temperature_unit == Unit::Fahrenheit,
                ));
                if !self.preferences.reduced_motion {
                    self.panel_started = Some(Instant::now());
                }
            }
            Target::WeatherQuery => self.open_keyboard(TextField::Location),
            Target::GreetingStyle => {
                self.subpage = Some(Subpage::GreetingStyle);
                self.focus = Target::Choice(usize::from(
                    self.preferences.greeting_style == GreetingStyle::Rude,
                ));
                self.scroll = 0.;
                if !self.preferences.reduced_motion {
                    self.panel_started = Some(Instant::now());
                }
                return vec![];
            }
            Target::GreetingName => self.open_keyboard(TextField::Name),
            Target::GreetingNameClear => {
                self.preferences.greeting_name.clear();
                self.focus = Target::GreetingName;
                return vec![Effect::Save];
            }
            Target::AppNamesHome => {
                self.preferences.show_home_names = !self.preferences.show_home_names;
                return vec![Effect::Save];
            }
            Target::AppNamesApps => {
                self.preferences.show_app_names = !self.preferences.show_app_names;
                return vec![Effect::Save];
            }
            Target::ClockFormat | Target::ClockTimeFormat => {
                let format = self.focus == Target::ClockFormat;
                self.subpage = Some(if format {
                    Subpage::ClockFormat
                } else {
                    Subpage::TimeFormat
                });
                self.focus = Target::Choice(usize::from(if format {
                    self.preferences.hour_24
                } else {
                    self.preferences.show_seconds
                }));
                self.scroll = 0.;
                if !self.preferences.reduced_motion {
                    self.panel_started = Some(Instant::now());
                }
            }
            Target::InputRename => self.open_keyboard(TextField::InputName),
            Target::InputNameReset => {
                if let Some(Subpage::Input(id)) = &self.subpage
                    && self.preferences.input_names.remove(id).is_some()
                {
                    self.focus = Target::InputRename;
                    return vec![Effect::Save];
                }
            }
            Target::WeatherSearch => {
                return self
                    .weather
                    .search(self.preferences.language)
                    .map(|command| vec![Effect::Weather(command)])
                    .unwrap_or_default();
            }
            Target::WeatherClear => {
                self.preferences.weather_location = None;
                self.weather.remove_location();
                self.focus = Target::WeatherSearch;
                return vec![Effect::Save, Effect::Weather(weather::Command::ClearCache)];
            }
            Target::WeatherResult(index) => {
                if let Some(place) = self.weather.results.get(index).cloned() {
                    self.preferences.weather_location = Some(place);
                    self.weather.invalidate_forecast();
                    self.weather.cancel_search();
                    self.focus = Target::WeatherSearch;
                    return vec![Effect::Save];
                }
            }
            Target::Sidebar(index) => {
                self.section = index;
                self.subpage = None;
                let target = self.section_entry();
                self.settings_push(SettingsLevel::Section, target);
            }
            Target::WallpaperMenu(index) => match index {
                0 => self.settings_push(
                    SettingsLevel::WallpaperType,
                    Target::WallpaperTab(usize::from(self.preferences.background == "video")),
                ),
                1 => self.settings_push(SettingsLevel::WallpaperPhotos, Target::Background(0)),
                2 => self.settings_push(SettingsLevel::WallpaperVideos, Target::VideoGroup(0)),
                _ => {}
            },
            Target::Setting(index) => {
                match index {
                    0 => {
                        self.subpage = Some(Subpage::AppNames);
                        self.focus = Target::AppNamesHome;
                        self.scroll = 0.;
                        if !self.preferences.reduced_motion {
                            self.panel_started = Some(Instant::now());
                        }
                        return vec![];
                    }
                    1 => {
                        self.subpage = Some(Subpage::Clock);
                        self.scroll = 0.;
                        if !self.preferences.reduced_motion {
                            self.panel_started = Some(Instant::now());
                        }
                        self.focus = Target::ClockFormat;
                        return vec![];
                    }
                    2 => self.preferences.reduced_motion = !self.preferences.reduced_motion,
                    4 => {
                        self.subpage = Some(Subpage::Language);
                        self.focus = Target::Choice(self.preferences.language.index());
                        if !self.preferences.reduced_motion {
                            self.panel_started = Some(Instant::now());
                        }
                        return vec![];
                    }
                    5 => return vec![Effect::Quit],
                    9 | 10 => {
                        let Some(root) = self.root.as_mut() else {
                            return vec![];
                        };
                        let change = if index == 9 {
                            root.wake = !root.wake;
                            crate::platform::RootSet::Wake(root.wake)
                        } else {
                            let on = !root.home_auto_launch.unwrap_or(true);
                            root.home_auto_launch = Some(on);
                            crate::platform::RootSet::HomeAutoLaunch(on)
                        };
                        self.dirty = true;
                        return vec![Effect::Root(Some(change))];
                    }
                    7 => return vec![Effect::Launch("com.webos.app.home".into())],
                    8 => return vec![Effect::Launch("com.denisefremov.luma.aerials".into())],
                    6 => {
                        self.subpage = Some(Subpage::Greeting);
                        self.focus = Target::GreetingStyle;
                        self.scroll = 0.;
                        if !self.preferences.reduced_motion {
                            self.panel_started = Some(Instant::now());
                        }
                        return vec![];
                    }
                    _ => return vec![Effect::Reload],
                }
                return vec![Effect::Save];
            }
            Target::WallpaperTab(tab) => {
                let video = tab == 1;
                if video && self.preferences.background != "video" {
                    self.photo_background_backup = self.preferences.background.clone();
                    self.preferences.background = "video".into();
                } else if !video && self.preferences.background == "video" {
                    self.preferences.background = self.photo_background_backup.clone();
                }
                self.wallpaper_gallery_video = video;
                self.previous_background = None;
                self.settings_pop(SettingsLevel::Section, Target::WallpaperMenu(0));
                return vec![Effect::Save];
            }
            Target::VideoFilter(filter) => {
                if filter >= crate::video_catalog::FILTERS.len() {
                    return vec![];
                }
                self.video_filter = filter;
                self.scroll = 0.;
                self.focus = Target::VideoFilter(filter);
                self.focus_started = Instant::now();
                self.video_filter_changed_at = Instant::now();
                return vec![];
            }
            Target::VideoShuffle => {
                self.preferences.video_shuffle = !self.preferences.video_shuffle;
                return vec![Effect::Save];
            }
            Target::VideoSource(filter) => {
                let Some((_, source)) = crate::video_catalog::FILTERS.get(filter) else {
                    return vec![];
                };
                if filter == 0 {
                    return vec![];
                }
                let indices = crate::video_catalog::display_filter(filter);
                let selected = indices.iter().all(|&i| {
                    !self
                        .preferences
                        .video_excluded
                        .contains(&crate::video_catalog::all()[i].id)
                });
                for index in indices {
                    let id = &crate::video_catalog::all()[index].id;
                    if selected {
                        self.preferences.video_excluded.insert(id.clone());
                    } else {
                        self.preferences.video_excluded.remove(id);
                    }
                }
                let _ = source;
                return vec![Effect::Save];
            }
            Target::VideoGroup(group) => {
                let Some(index) = crate::video_catalog::groups(0)
                    .get(group)
                    .and_then(|(_, _, items)| items.first())
                    .copied()
                else {
                    return vec![];
                };
                self.settings_push(
                    SettingsLevel::WallpaperCategory(group),
                    Target::VideoPick(index),
                );
                return vec![];
            }
            Target::VideoToggle => {
                if self.preferences.background == "video" {
                    self.preferences.background = self.photo_background_backup.clone();
                    self.video_frame_ready = false;
                } else {
                    self.photo_background_backup = self.preferences.background.clone();
                    self.preferences.background = "video".into();
                }
                self.previous_background = None;
                return vec![Effect::Save];
            }
            Target::VideoPick(index) => {
                if index >= crate::video_catalog::all().len() {
                    return vec![];
                }
                if !crate::video_catalog::toggle(&mut self.preferences.video_excluded, index) {
                    self.notify("Keep at least one video selected");
                    return vec![];
                }
                return vec![Effect::Save];
            }
            Target::Background(index) => {
                let selected = if let Some((value, _)) = BACKGROUNDS.get(index) {
                    *value
                } else {
                    return vec![];
                };
                if !self.preferences.reduced_motion
                    && self.preferences.background != selected
                    && self.preferences.background != "video"
                    && selected != "video"
                {
                    self.previous_background =
                        Some((self.preferences.background.clone(), Instant::now()));
                } else {
                    self.previous_background = None;
                }
                self.preferences.background = selected.into();
                self.photo_background_backup = selected.into();
                return vec![Effect::Save];
            }
            Target::VideoNext => {
                self.preferences.video_index = crate::video_catalog::next_index(
                    self.preferences.video_index,
                    &self.preferences.video_excluded,
                    self.preferences.video_shuffle,
                    self.preferences.video_shuffle_seed,
                )
                .unwrap_or(self.preferences.video_index);
                return vec![Effect::Save];
            }
            Target::InputMode(id) => {
                let mode = self
                    .preferences
                    .input_modes
                    .get(&id)
                    .copied()
                    .unwrap_or_default();
                self.subpage = Some(Subpage::Input(id));
                if !self.preferences.reduced_motion {
                    self.panel_started = Some(Instant::now());
                }
                self.focus = Target::Choice(match mode {
                    InputMode::On => 0,
                    InputMode::Off => 1,
                    InputMode::Auto => 2,
                });
            }
            Target::Choice(index) => {
                match self.subpage.clone() {
                    Some(Subpage::GreetingStyle) => {
                        self.preferences.greeting_style = if index == 1 {
                            GreetingStyle::Rude
                        } else {
                            GreetingStyle::Classic
                        }
                    }
                    Some(Subpage::ClockFormat) => self.preferences.hour_24 = index == 1,
                    Some(Subpage::TimeFormat) => self.preferences.show_seconds = index == 1,
                    Some(Subpage::Input(id)) => {
                        self.preferences.input_modes.insert(
                            id,
                            [InputMode::On, InputMode::Off, InputMode::Auto][index.min(2)],
                        );
                    }
                    Some(Subpage::Temperature) => {
                        self.preferences.temperature_unit = if index == 1 {
                            Unit::Fahrenheit
                        } else {
                            Unit::Celsius
                        }
                    }
                    Some(Subpage::Language) => {
                        self.preferences.language =
                            Language::ALL[index.min(Language::ALL.len() - 1)];
                        self.weather.cancel_search();
                    }
                    Some(
                        Subpage::SmartConnect
                        | Subpage::SmartSelect(_)
                        | Subpage::WeatherLocation
                        | Subpage::Greeting
                        | Subpage::AppNames
                        | Subpage::Clock,
                    ) => {
                        return vec![];
                    }
                    None => return vec![],
                }
                self.close_subpage();
                return vec![Effect::Save];
            }
            Target::Option(index) => {
                let Some(Overlay::Options { id, restore }) = self.overlay.take() else {
                    return vec![];
                };
                self.panel_started = None;
                let old_ids: Vec<_> = self.apps().iter().map(|a| a.id.clone()).collect();
                let old_index = old_ids.iter().position(|item| item == &id).unwrap_or(0);
                let mut favorites = self.preferences.favorites(&self.inventory.apps);
                let title = self
                    .inventory
                    .apps
                    .iter()
                    .find(|app| app.id == id)
                    .map_or(id.clone(), |app| app.title.clone());
                if index == 0 {
                    if favorites.contains(&id) {
                        favorites.retain(|item| item != &id);
                        self.notify(
                            self.preferences
                                .language
                                .format("favoriteRemoved", &[("app", &title)]),
                        );
                    } else {
                        favorites.push(id.clone());
                        self.notify(
                            self.preferences
                                .language
                                .format("favoriteAdded", &[("app", &title)]),
                        );
                    }
                    self.preferences.hidden.retain(|item| item != &id);
                    self.preferences.favorites = Some(favorites);
                } else if index == 1 {
                    if self.preferences.hidden.contains(&id) {
                        self.preferences.hidden.retain(|item| item != &id);
                        self.notify(
                            self.preferences
                                .language
                                .format("appRestored", &[("app", &title)]),
                        );
                    } else {
                        self.preferences.hidden.push(id.clone());
                        self.notify(
                            self.preferences
                                .language
                                .format("appHidden", &[("app", &title)]),
                        );
                    }
                }
                if self.filter == 5 && self.hidden_count() == 0 {
                    self.filter = 0;
                }
                self.focus = restore;
                self.pointer = false;
                if let Target::App(id) = &self.focus {
                    let apps = self.apps();
                    if !apps.iter().any(|app| &app.id == id) {
                        self.focus = apps
                            .get(old_index.min(apps.len().saturating_sub(1)))
                            .map(|app| Target::App(app.id.clone()))
                            .unwrap_or_else(|| self.first_content());
                    }
                }
                return if index < 2 {
                    vec![Effect::Save]
                } else {
                    vec![]
                };
            }
            Target::Key(index) => match KEYS[index] {
                "Space" => self.text(" "),
                "Delete" => self.delete(),
                "Clear" => match self.keyboard_field {
                    TextField::HomeAddress => self.ha_address_draft.clear(),
                    TextField::MoonlightAddress => self.moonlight_ip_draft.clear(),
                    TextField::Location => self.weather.edit(String::new()),
                    TextField::Name => self.greeting_name_draft.clear(),
                    TextField::InputName => self.input_name.clear(),
                    TextField::Apps => self.query.clear(),
                    TextField::Library => {
                        if let Some(library) = self.library.as_mut() {
                            library.draft.clear();
                        }
                        self.library_typed();
                    }
                },
                "Done" => {
                    self.overlay = None;
                    self.panel_started = None;
                    self.focus = self.keyboard_return();
                    return self.commit_text_input();
                }
                key => self.text(key),
            },
            _ => {}
        }
        vec![]
    }

    pub fn back(&mut self) -> Vec<Effect> {
        if self.ambient_on() {
            self.leave_ambient();
            return vec![];
        }
        self.pointer = false;
        self.dirty = true;
        if self.moving.take().is_some() {
            return vec![];
        }
        if let Some(g) = self.game.as_mut()
            && g.shot.is_some()
        {
            g.shot = None;
            return vec![];
        }
        if self.game.is_some() {
            self.close_game();
            return vec![];
        }
        if self.album.is_some() {
            self.close_album();
            return vec![];
        }
        if let Some(playback) = self.playback.as_mut() {
            let cmds = playback.back();
            if cmds.contains(&crate::player::Command::Close) {
                if let Some(session) = self.playback.as_ref()
                    && let Some(id) = session.request.item_id
                {
                    self.preferences
                        .playback_progress
                        .insert(id.to_string(), session.position_ms);
                }
                self.playback = None;
                self.focus = Target::MoviePlay;
            }
            if cmds.contains(&crate::player::Command::Close) {
                let mut effects = vec![Effect::Save];
                effects.extend(cmds.into_iter().map(Effect::Player));
                return effects;
            }
            return cmds.into_iter().map(Effect::Player).collect();
        }
        if self.movie.is_some() {
            self.movie_back();
            return vec![];
        }
        if let Some(overlay) = self.overlay.take() {
            self.panel_started = None;
            if let Overlay::Options { restore, .. } = overlay {
                self.focus = restore;
                return vec![];
            }
            self.focus = self.keyboard_return();
            return self.commit_text_input();
        }
        if self.library.is_some() {
            self.library_back();
            return vec![];
        }
        if let Some(id) = self.ha_camera_menu.take() {
            self.focus = Target::SmartCamera(id);
            return vec![];
        }
        if self.ha_camera.take().is_some() || self.ha_detail.take().is_some() {
            if !self.widget_camera_closed() {
                self.focus = self.first_content();
            }
            return vec![];
        }
        if let Some(effects) = self.widget_back() {
            return effects;
        }
        if self.page == Page::Settings && self.subpage.is_some() {
            let pairing = self.subpage == Some(Subpage::SmartConnect);
            self.close_subpage();
            return if pairing {
                vec![Effect::HomeAssistant(
                    crate::smart_home::Command::EndPairing,
                )]
            } else {
                vec![]
            };
        }
        if self.page == Page::Settings {
            match self.settings_level {
                // Handled by widget_back.
                SettingsLevel::Widgets(_) => {}
                SettingsLevel::WallpaperCategory(group) => {
                    self.settings_pop(SettingsLevel::WallpaperVideos, Target::VideoGroup(group));
                }
                SettingsLevel::WallpaperVideos => {
                    self.settings_pop(SettingsLevel::Section, Target::WallpaperMenu(2))
                }
                SettingsLevel::WallpaperPhotos => {
                    self.settings_pop(SettingsLevel::Section, Target::WallpaperMenu(1))
                }
                SettingsLevel::WallpaperType => {
                    self.settings_pop(SettingsLevel::Section, Target::WallpaperMenu(0))
                }
                SettingsLevel::UiResolution => {
                    self.settings_pop(SettingsLevel::Section, Target::UiResolutionOpen)
                }
                SettingsLevel::StackAdd => self.settings_pop(
                    SettingsLevel::Section,
                    Target::StackRow(self.preferences.stacks.len()),
                ),
                SettingsLevel::MoonlightHosts => {
                    self.settings_pop(SettingsLevel::StackAdd, Target::StackSource(0))
                }
                SettingsLevel::PlexLink => {
                    self.settings_pop(SettingsLevel::StackAdd, Target::StackSource(2));
                    self.plex_code = None;
                    self.plex_status = None;
                    self.plex_qr.clear();
                    return vec![Effect::PlexCancel];
                }
                SettingsLevel::JellyfinLink => {
                    self.settings_pop(SettingsLevel::StackAdd, Target::StackSource(1));
                    self.jellyfin_reset();
                    return vec![Effect::JellyfinCancel];
                }
                SettingsLevel::MoonlightPin => {
                    self.settings_pop(SettingsLevel::MoonlightHosts, Target::MoonlightHost(0));
                    self.moonlight_status = Some("Select a PC to pair".into());
                    self.moonlight_pin = None;
                    self.moonlight_pair_error = false;
                    return vec![Effect::MoonlightCancel];
                }
                SettingsLevel::StackConfig(i) => {
                    self.settings_pop(SettingsLevel::Section, Target::StackRow(i))
                }
                SettingsLevel::StackCollection(i) => {
                    self.settings_pop(SettingsLevel::StackConfig(i), Target::StackOpt(0))
                }
                SettingsLevel::StackShape(i) => self.settings_pop(
                    SettingsLevel::StackConfig(i),
                    Target::StackOpt(
                        if self
                            .preferences
                            .stacks
                            .get(i)
                            .is_some_and(|s| crate::stacks::compact(&s.source))
                        {
                            1
                        } else {
                            2
                        },
                    ),
                ),
                SettingsLevel::HeadingFonts => {
                    self.settings_pop(SettingsLevel::Section, Target::HeadingFontOpen)
                }
                SettingsLevel::TextFonts => {
                    self.settings_pop(SettingsLevel::Section, Target::TextFontOpen)
                }
                SettingsLevel::Section => {
                    self.settings_pop(SettingsLevel::Root, Target::Sidebar(self.section))
                }
                SettingsLevel::Root => {
                    if matches!(self.focus, Target::Nav(_)) {
                        self.open(0);
                    } else {
                        self.focus = Target::Nav(self.settings_nav_index());
                    }
                }
            }
            return vec![];
        }
        // Settings content always returns to the current sidebar item. The
        // previous gallery-only exception swallowed Back in the grid and made
        // the remote feel trapped at the source tabs.
        if self.page == Page::Settings && !matches!(self.focus, Target::Nav(_)) {
            if matches!(self.focus, Target::Sidebar(_)) {
                self.focus = Target::Nav(self.settings_nav_index());
            } else {
                self.focus = Target::Sidebar(self.section);
            }
            self.scroll = 0.;
            return vec![];
        }
        if self.page == Page::Apps && matches!(self.focus, Target::Search | Target::ClearSearch) {
            self.query.clear();
            self.focus = Target::Nav(1);
            self.scroll = 0.;
            return vec![];
        }
        // Back from page content goes up to this page's own tab first; only
        // Back on the navigation itself returns Home.
        if matches!(self.page, Page::Apps | Page::SmartHome)
            && !matches!(self.focus, Target::Nav(_))
        {
            self.focus = Target::Nav(if self.page == Page::Apps { 1 } else { 2 });
            self.scroll_goal = 0.;
            return vec![];
        }
        if self.page == Page::Home && !matches!(self.focus, Target::Nav(_)) {
            self.focus = Target::Nav(0);
            self.scroll_goal = 0.;
            return vec![];
        }
        if self.page != Page::Home {
            self.open(0);
            vec![]
        } else {
            vec![Effect::Quit]
        }
    }
}

pub const STACK_RETURN_DELAY: f32 = 0.04;
/// Farther than this, the row starts its glide back from here.
pub const STACK_RETURN_MAX: f32 = 4. * 1920.;
const STACK_RETURN_GLIDE: f32 = 0.4;
pub const STACK_RETURN_TIME: f32 = STACK_RETURN_DELAY + STACK_RETURN_GLIDE;

/// Back-out: 0 -> 1, one swing past 1 (no wobble after it) and it lands
/// at rest exactly at p = 1. `s` sets the swing: 1 is about 3.7 %.
pub fn back_out(p: f32, s: f32) -> f32 {
    let u = p.clamp(0., 1.) - 1.;
    1. + (s + 1.) * u * u * u + s * u * u
}

/// The `s` for back_out whose single swing is `overshoot` (a fraction).
fn back_out_for(overshoot: f32) -> f32 {
    // Peak swing is 4 s^3 / (27 (s + 1)^2), rising with s.
    let (mut lo, mut hi) = (0., 2.);
    for _ in 0..24 {
        let s = (lo + hi) / 2.;
        if 4. * s * s * s / (27. * (s + 1.) * (s + 1.)) < overshoot {
            lo = s;
        } else {
            hi = s;
        }
    }
    lo
}

/// A short pause, then one quick glide home that swings a little past the
/// first card (4 %, at most 40 px) once and stops dead, 0.44 s in all
/// (the damped spring wobbled for too long at the end).
pub fn stack_return_curve(offset: f32, t: f32) -> f32 {
    let u = t - STACK_RETURN_DELAY;
    if u <= 0. {
        return offset;
    }
    if t >= STACK_RETURN_TIME || offset == 0. {
        return 0.;
    }
    let s = back_out_for((40. / offset.abs()).min(0.04));
    offset * (1. - back_out(u / STACK_RETURN_GLIDE, s))
}

/// Clean wallpaper mode: the caption's press, the fly-away, and the stagger
/// between the interface's blocks (and between stack rows).
pub const AMBIENT_PRESS: std::time::Duration = std::time::Duration::from_millis(160);
pub const AMBIENT_TIME: f32 = 0.32;
pub const AMBIENT_STAGGER: f32 = 0.02;
/// Last place in the fly-away order; coming back runs it in reverse.
pub const AMBIENT_ORDERS: f32 = 12.;

impl State {
    /// The interface is away or on its way out.
    pub fn ambient_on(&self) -> bool {
        self.ambient.is_some_and(|(on, _)| on)
    }

    /// How far a block that starts `delay` seconds into the move has gone
    /// (0 in place, 1 off screen). Coming back runs the same order.
    pub fn ambient_away(&self, delay: f32) -> f32 {
        let Some((on, start)) = self.ambient else {
            return 0.;
        };
        let t = start.elapsed().as_secs_f32() - delay;
        let k = if self.preferences.reduced_motion {
            if t >= 0. { 1. } else { 0. }
        } else {
            crate::motion::progress(t.max(0.), AMBIENT_TIME)
        };
        if on { k } else { 1. - k }
    }

    /// The caption's press: 0 before and after, peaks halfway through.
    pub fn ambient_press(&self) -> f32 {
        match self.ambient {
            Some((true, start)) if start > Instant::now() => {
                let left = (start - Instant::now()).as_secs_f32() / AMBIENT_PRESS.as_secs_f32();
                (std::f32::consts::PI * (1. - left)).sin()
            }
            _ => 0.,
        }
    }

    fn ambient_animating(&self) -> bool {
        self.ambient.is_some_and(|(_, start)| {
            Instant::now() < start
                || start.elapsed().as_secs_f32()
                    < AMBIENT_TIME + (AMBIENT_ORDERS + 1.) * AMBIENT_STAGGER
        })
    }

    /// Any key, click or wheel brings the interface back, like tvOS.
    pub fn leave_ambient(&mut self) {
        if self.ambient_on() {
            self.ambient = Some((false, Instant::now()));
            self.dirty = true;
        }
    }
}

/// Screensaver delays offered in Settings › Appearance, in minutes.
pub const SCREENSAVER_STEPS: [u8; 6] = [0, 5, 10, 15, 20, 30];

impl State {
    /// After the chosen minutes without input on a quiet Home, the interface
    /// clears away exactly as OK on the video caption does.
    fn screensaver_tick(&mut self) {
        let minutes = self.preferences.screensaver_minutes;
        if minutes == 0 || self.ambient.is_some_and(|(on, _)| on) {
            return;
        }
        let quiet = self.page == Page::Home
            && self.playback.is_none()
            && self.movie.is_none()
            && self.game.is_none()
            && self.album.is_none()
            && self.library.is_none()
            && self.overlay.is_none()
            && self.ha_detail.is_none()
            && self.ha_camera.is_none()
            && self.widget_detail.is_none()
            && self.widget_edit.is_none()
            && self.moving.is_none();
        if quiet && self.last_input.elapsed().as_secs() >= u64::from(minutes) * 60 {
            self.ambient = Some((true, Instant::now()));
            self.dirty = true;
        }
    }
}

impl State {
    /// Where an element `order` places into the fly-away is: 0 at home, 1
    /// gone. Leaving it draws back a touch first; coming back (in reverse
    /// order) it swings a little past home and settles, like the rows.
    pub fn ambient_k(&self, order: f32) -> f32 {
        let Some((on, start)) = self.ambient else {
            return 0.;
        };
        let delay = if on { order } else { AMBIENT_ORDERS - order }.max(0.) * AMBIENT_STAGGER;
        let t = ((start.elapsed().as_secs_f32() - delay) / AMBIENT_TIME).clamp(0., 1.);
        if self.preferences.reduced_motion {
            return if on { 1. } else { 0. };
        }
        if on {
            // Back-in: a 2-3 % wind-up, then away.
            const S: f32 = 0.6;
            t * t * ((S + 1.) * t - S)
        } else {
            // The rows' glide: one swing about 4 % past home, then still.
            1. - back_out(t, 1.)
        }
    }
}
