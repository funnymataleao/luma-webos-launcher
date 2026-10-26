use crate::i18n::Language;
use crate::weather::{Location, Unit};
use crate::weather_phrases::{DayRecord, GreetingStyle};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashSet};

pub const CATEGORIES: [&str; 4] = ["Streaming", "Music", "Gaming", "Utilities"];
// Only enable after verifying actual video compositing on the target TV.
// A running GStreamer process alone does not prove a working video wallpaper.
pub const VIDEO_WALLPAPER_SUPPORTED: bool = true;
pub const BACKGROUNDS: [(&str, &str); 9] = [
    ("slate", "Slate"),
    ("midnight", "Midnight"),
    ("prism", "Prism"),
    ("dunes", "Monument Valley"),
    ("san-francisco", "San Francisco"),
    ("summit", "Summit"),
    ("forest", "Forest"),
    ("yosemite", "Yosemite"),
    ("northern-lights", "Northern lights"),
];

// Generic fallbacks use delimited words, so "game" does not match "endgame".
// Non-ASCII bytes count as boundaries for localized titles. Curated vector
// branding is selected separately by exact ID in branding.rs.
fn contains_word(haystack: &str, needle: &str) -> bool {
    let bytes = haystack.as_bytes();
    haystack.match_indices(needle).any(|(start, matched)| {
        let end = start + matched.len();
        let before_ok = start == 0 || !bytes[start - 1].is_ascii_alphanumeric();
        let after_ok = end == bytes.len() || !bytes[end].is_ascii_alphanumeric();
        before_ok && after_ok
    })
}

#[derive(Clone, Debug, Serialize)]
pub struct App {
    pub id: String,
    pub title: String,
    pub brand: String,
    pub category: String,
    #[serde(skip)]
    pub icon: Option<crate::app_artwork::Source>,
}

impl App {
    pub fn new(id: &str, title: &str) -> Self {
        let identity = format!("{id} {title}").to_lowercase();
        let generic = [
            ("kodi", "kodi", "Streaming"),
            ("moonlight", "moonlight", "Gaming"),
            ("limelight", "moonlight", "Gaming"),
            ("game", "gaming", "Gaming"),
            ("hbchannel", "homebrew", "Utilities"),
        ]
        .into_iter()
        .find_map(|(needle, brand, category)| {
            contains_word(&identity, needle).then_some((brand, category))
        })
        .unwrap_or(("generic", "Utilities"));
        let (brand, category) =
            crate::branding::for_app(id).map_or(generic, |brand| (brand.key, brand.category));
        Self {
            id: id.into(),
            title: title.into(),
            brand: brand.into(),
            category: category.into(),
            icon: None,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct Input {
    pub id: String,
    pub title: String,
    pub port: String,
    pub connected: Option<bool>,
    /// The source is sending a picture (eim getHDMISignalInfo); a sleeping
    /// device keeps the cable's 5 V but stops the signal.
    pub signal: Option<bool>,
}

#[derive(Clone, Debug, Default)]
pub struct Inventory {
    pub apps: Vec<App>,
    pub inputs: Vec<Input>,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq)]
pub enum InputMode {
    #[default]
    Auto,
    On,
    Off,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct SmartHomePreferences {
    pub server_url: String,
    pub server_name: String,
    pub show_tab: bool,
    pub selected_entities: Vec<String>,
    pub selected_cameras: Vec<String>,
    pub selected_scenes: Vec<String>,
    /// 1: an empty selection means "recommended" instead of "everything".
    pub selection_version: u8,
    /// Cameras whose tile shows the picture only blurred.
    pub hidden_cameras: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(default)]
pub struct Accent {
    pub kind: String,
    pub id: String,
    pub use_focus: bool,
    pub use_nav: bool,
    pub use_headline: bool,
}
impl Default for Accent {
    fn default() -> Self {
        Self {
            kind: "gradient".into(),
            id: "Citrus".into(),
            use_focus: true,
            use_nav: true,
            use_headline: true,
        }
    }
}
// Final 18:55 palette (replaces all earlier neon values).
pub const ACCENT_SOLIDS: [(&str, u32); 8] = [
    ("Blue", 0x4D9DF7),
    ("Indigo", 0x7F7DE8),
    ("Purple", 0xC77FEA),
    ("Pink", 0xF2677F),
    ("Orange", 0xF5B04A),
    ("Green", 0x5CCB79),
    ("Teal", 0x7FCDEB),
    ("White", 0xF2F2F5),
];
pub const PHRASE_PCT_MIN: u8 = 70;
pub const PHRASE_PCT_MAX: u8 = 150;
pub const ACCENT_GRADIENTS: [(&str, u32, u32); 8] = [
    ("Aurora", 0xA978FF, 0x3FA2FF),
    ("Sunset", 0xF95B7D, 0xFAB942),
    ("Mint", 0x46E5AF, 0x42ADF1),
    ("Flamingo", 0xF760A8, 0xFCBB97),
    ("Citrus", 0xFFE94A, 0x3FD3FF),
    ("Ruby", 0xEC447B, 0x8E54DC),
    ("Gold", 0xFAD750, 0xF79445),
    ("Silver", 0xF7F8FA, 0x9EABBA),
];
impl Accent {
    pub fn normalized(&mut self) {
        let valid = if self.kind == "gradient" {
            ACCENT_GRADIENTS.iter().any(|(id, _, _)| *id == self.id)
        } else {
            ACCENT_SOLIDS.iter().any(|(id, _)| *id == self.id)
        };
        if !valid {
            self.kind = "gradient".into();
            self.id = "Aurora".into();
        }
    }
    pub fn colors(&self) -> (u32, u32) {
        if self.kind == "gradient" {
            ACCENT_GRADIENTS
                .iter()
                .find(|(id, _, _)| *id == self.id)
                .map(|(_, a, b)| (*a, *b))
                .unwrap_or((0xA978FF, 0x3FA2FF))
        } else {
            let color = ACCENT_SOLIDS
                .iter()
                .find(|(id, _)| *id == self.id)
                .map(|(_, n)| *n)
                .unwrap_or(0xffffff);
            (color, color)
        }
    }
    pub fn choose(&mut self, index: usize) {
        if index < ACCENT_SOLIDS.len() {
            self.kind = "solid".into();
            self.id = ACCENT_SOLIDS[index].0.into();
        } else if let Some((name, _, _)) = ACCENT_GRADIENTS.get(index - ACCENT_SOLIDS.len()) {
            self.kind = "gradient".into();
            self.id = (*name).into();
        }
    }
}
/// TV surface density. Layout is always authored at 1920×1080.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
pub enum UiResolution {
    #[default]
    #[serde(rename = "auto")]
    Auto,
    #[serde(rename = "4k")]
    Uhd,
    #[serde(rename = "1440p")]
    Qhd,
    #[serde(rename = "1080p")]
    Hd,
}
impl UiResolution {
    pub const ALL: [Self; 4] = [Self::Auto, Self::Uhd, Self::Qhd, Self::Hd];
    pub const fn label(self) -> &'static str {
        match self {
            Self::Auto => "Auto",
            Self::Uhd => "4K (3840×2160)",
            Self::Qhd => "1440p (2560×1440)",
            Self::Hd => "1080p (1920×1080)",
        }
    }
    pub const fn short(self) -> &'static str {
        match self {
            Self::Auto => "Auto",
            Self::Uhd => "4K",
            Self::Qhd => "1440p",
            Self::Hd => "1080p",
        }
    }
    pub const fn pixels(self) -> crate::raster::Size {
        match self {
            Self::Auto | Self::Uhd => crate::raster::Size::UHD,
            Self::Qhd => crate::raster::Size::new(2560, 1440),
            Self::Hd => crate::raster::Size::HD,
        }
    }
    pub const fn lower(self) -> Self {
        match self {
            Self::Auto | Self::Uhd => Self::Qhd,
            Self::Qhd => Self::Hd,
            Self::Hd => Self::Hd,
        }
    }
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct Preferences {
    pub favorites: Option<Vec<String>>,
    pub hidden: Vec<String>,
    pub background: String,
    pub video_index: usize,
    /// Bitset for Apple, Amazon, Aerial Community, RobinFrcd.
    pub video_sources_mask: u8,
    pub video_excluded: BTreeSet<String>,
    pub video_shuffle: bool,
    pub video_shuffle_seed: u64,
    pub show_home_names: bool,
    pub show_app_names: bool,
    pub hour_24: bool,
    pub reduced_motion: bool,
    pub show_seconds: bool,
    /// Optional name appended to the Home greeting ("Good evening, Ana."). Empty = plain greeting.
    pub greeting_name: String,
    pub heading_font: crate::typography::HeadingFont,
    pub text_font: crate::typography::TextFont,
    /// Relative UI sizes in percent. Bounds prevent a font change from breaking layout.
    pub heading_size_pct: u8,
    pub text_size_pct: u8,
    /// Home headline phrase and its subtitle, on top of the global sizes.
    pub phrase_title_pct: u8,
    pub phrase_subtitle_pct: u8,
    /// Soft clicks on remote focus moves and OK.
    pub ui_sounds: bool,
    /// Minutes without the remote before Home clears to the wallpaper
    /// (0 = never).
    pub screensaver_minutes: u8,
    /// User-requested UI surface density. Auto uses auto_ui_cap.
    pub ui_resolution: UiResolution,
    /// Last stable Auto ceiling for this particular TV. Lowered only; no oscillation.
    pub auto_ui_cap: UiResolution,

    pub greeting_style: GreetingStyle,
    pub weather_day_history: Vec<DayRecord>,
    pub input_modes: BTreeMap<String, InputMode>,
    /// Custom names by input app ID; the TV's port and label stay visible as detail.
    pub input_names: BTreeMap<String, String>,
    #[serde(deserialize_with = "crate::weather::deserialize_enabled")]
    pub show_weather: bool,
    #[serde(deserialize_with = "crate::weather::deserialize_location")]
    pub weather_location: Option<Location>,
    #[serde(deserialize_with = "crate::weather::deserialize_unit")]
    pub temperature_unit: Unit,
    #[serde(deserialize_with = "crate::i18n::deserialize_language")]
    pub language: Language,
    pub smart_home: SmartHomePreferences,
    pub accent: Accent,
    /// Home stack rows (Settings › Stacks).
    pub stacks: Vec<crate::stacks::StackConfig>,
    pub moonlight: crate::moonlight::MoonlightLink,
    /// Jellyfin sign-in (Quick Connect). The token is never logged.
    pub jellyfin: crate::jellyfin::JellyfinLink,
    /// Plex sign-in (plex.tv PIN). Tokens are never logged.
    pub plex: crate::plex::PlexLink,
    /// Played, first seen and favourite games (kept across re-pairing).
    pub games: crate::moonlight::GameLibrary,
    /// Last playback position, keyed by media item ID; never stores URLs or tokens.
    pub playback_progress: BTreeMap<String, u64>,
    /// Play opens Luma's own player instead of the TV's (Settings › Player).
    pub luma_player: bool,
    /// Home widgets right of the headline (Settings › Widgets).
    pub widgets: crate::widgets::WidgetPrefs,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            favorites: None,
            hidden: vec![],
            background: "dunes".into(),
            video_index: 0,
            video_sources_mask: crate::video_catalog::ALL_SOURCES,
            video_excluded: BTreeSet::new(),
            video_shuffle: true,
            video_shuffle_seed: 0,
            show_home_names: false,
            show_app_names: true,
            hour_24: false,
            reduced_motion: false,
            show_seconds: false,
            greeting_name: String::new(),
            heading_font: crate::typography::HeadingFont::Nunito,
            text_font: crate::typography::TextFont::Manrope,
            heading_size_pct: 130,
            text_size_pct: 90,
            phrase_title_pct: 100,
            phrase_subtitle_pct: 130,
            ui_sounds: true,
            screensaver_minutes: 10,
            ui_resolution: UiResolution::Qhd,
            auto_ui_cap: UiResolution::Uhd,

            greeting_style: GreetingStyle::Rude,
            weather_day_history: Vec::new(),
            input_modes: BTreeMap::new(),
            input_names: BTreeMap::new(),
            show_weather: true,
            weather_location: None,
            temperature_unit: Unit::Celsius,
            language: Language::En,
            smart_home: SmartHomePreferences::default(),
            accent: Accent::default(),
            stacks: vec![],
            moonlight: Default::default(),
            jellyfin: Default::default(),
            plex: Default::default(),
            games: Default::default(),
            playback_progress: BTreeMap::new(),
            luma_player: true,
            widgets: Default::default(),
        }
    }
}

impl Preferences {
    pub fn favorites(&self, apps: &[App]) -> Vec<String> {
        self.favorites.clone().unwrap_or_else(|| {
            let mut ids = Vec::new();
            for brand in [
                "netflix",
                "youtube",
                "apple-tv",
                "disney-plus",
                "prime-video",
                "live-tv",
            ] {
                if let Some(app) = apps.iter().find(|app| app.brand == brand) {
                    ids.push(app.id.clone());
                }
            }
            for app in apps {
                if ids.len() >= 6 {
                    break;
                }
                if !ids.contains(&app.id) {
                    ids.push(app.id.clone());
                }
            }
            ids
        })
    }

    pub fn input_visible(&self, input: &Input) -> bool {
        match self.input_modes.get(&input.id).copied().unwrap_or_default() {
            InputMode::On => true,
            InputMode::Off => false,
            // Auto: shown while the device sends a picture; without a
            // signal report, while the cable is plugged in.
            InputMode::Auto => input.signal.unwrap_or(input.connected == Some(true)),
        }
    }

    pub fn input_name(&self, input: &Input) -> Option<&str> {
        self.input_names.get(&input.id).map(String::as_str)
    }

    /// The custom name if one is saved, otherwise the port ("HDMI 2").
    pub fn input_title<'a>(&'a self, input: &'a Input) -> &'a str {
        self.input_name(input).unwrap_or(&input.port)
    }

    /// Saves a cleaned name; an empty name restores the port title.
    pub fn set_input_name(&mut self, id: &str, name: &str) -> bool {
        if !valid_app_id(id) {
            return false;
        }
        let name = clean_input_name(name);
        if name.is_empty() {
            self.input_names.remove(id).is_some()
        } else {
            self.input_names.insert(id.into(), name.clone()) != Some(name)
        }
    }

    pub fn resolved_ui_resolution(&self) -> UiResolution {
        if self.ui_resolution == UiResolution::Auto {
            match self.auto_ui_cap {
                UiResolution::Auto => UiResolution::Uhd,
                other => other,
            }
        } else {
            self.ui_resolution
        }
    }
    pub fn resolved_ui_size(&self) -> crate::raster::Size {
        self.resolved_ui_resolution().pixels()
    }
    pub fn ui_resolution_value(&self) -> String {
        if self.ui_resolution == UiResolution::Auto {
            format!("Auto · {}", self.resolved_ui_resolution().short())
        } else {
            self.ui_resolution.short().into()
        }
    }
    pub fn sanitize(&mut self) {
        if self.auto_ui_cap == UiResolution::Auto {
            self.auto_ui_cap = UiResolution::Uhd;
        }
        self.heading_size_pct = self.heading_size_pct.clamp(85, 170);
        self.text_size_pct = self.text_size_pct.clamp(90, 150);
        self.phrase_title_pct = self.phrase_title_pct.clamp(PHRASE_PCT_MIN, PHRASE_PCT_MAX);
        self.phrase_subtitle_pct = self
            .phrase_subtitle_pct
            .clamp(PHRASE_PCT_MIN, PHRASE_PCT_MAX);
        // Discard unsupported saved row sources while preserving linked media services.
        self.stacks
            .retain(|stack| matches!(stack.source.as_str(), "moonlight" | "jellyfin" | "plex"));
        let catalog = crate::video_catalog::all();
        self.video_excluded
            .retain(|id| catalog.iter().any(|v| &v.id == id));
        if crate::video_catalog::selected_count(&self.video_excluded) == 0 {
            self.video_excluded.clear();
        }
        self.video_sources_mask &= crate::video_catalog::ALL_SOURCES;
        if self.video_sources_mask == 0 {
            self.video_sources_mask = crate::video_catalog::ALL_SOURCES;
        }
        if (self.background == "video" && !VIDEO_WALLPAPER_SUPPORTED)
            || (self.background != "video"
                && !BACKGROUNDS.iter().any(|(id, _)| *id == self.background))
        {
            self.background = "dunes".into();
        }
        fn clean(ids: &mut Vec<String>) {
            let mut seen = HashSet::new();
            ids.retain(|id| valid_app_id(id) && seen.insert(id.clone()));
            ids.truncate(1000);
        }
        if let Some(ids) = &mut self.favorites {
            clean(ids);
        }
        clean(&mut self.hidden);
        self.input_modes.retain(|id, _| valid_app_id(id));
        self.input_names = std::mem::take(&mut self.input_names)
            .into_iter()
            .filter(|(id, _)| valid_app_id(id))
            .map(|(id, name)| (id, clean_input_name(&name)))
            .filter(|(_, name)| !name.is_empty())
            .take(128)
            .collect();
    }
}

pub const INPUT_NAME_LIMIT: usize = 40;

pub fn clean_input_name(name: &str) -> String {
    let clean: String = name.chars().filter(|c| !c.is_control()).collect();
    let clean: String = clean.trim().chars().take(INPUT_NAME_LIMIT).collect();
    clean.trim_end().into()
}

pub fn valid_app_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id
            .split(['.', '_', '-'])
            .all(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_alphanumeric()))
}

pub fn demo_inventory() -> Inventory {
    let mut inventory = Inventory {
        apps: [
            ("com.webos.app.livetv", "Live TV"),
            ("netflix", "Netflix"),
            ("youtube.leanback.v4", "YouTube"),
            ("com.apple.appletv", "Apple TV"),
            ("com.disney.disneyplus-prod", "Disney+"),
            ("amazon", "Prime Video"),
            ("plex", "Plex"),
            ("spotify-beehive", "Spotify"),
            ("org.jellyfin.webos", "Jellyfin"),
            ("org.xbmc.kodi", "Kodi"),
            ("com.limelight.webos", "Moonlight"),
            ("org.webosbrew.hbchannel", "Homebrew Channel"),
            ("com.webos.app.browser", "Web Browser"),
            ("com.arte.tv", "ARTE"),
            ("de.zdf.app.zdfm3", "ZDF"),
            ("ard.mediathek", "Das Erste"),
            ("coupangplay", "Coupang Play"),
            ("com.frograms.watchaplay.webos", "Watcha Play"),
            ("pooq", "Wavve"),
            ("com.cmb.rainbowtv", "Rainbow TV"),
            ("com.lgtvod.kr.app", "LG TV VOD"),
            ("com.webos.app.igallery", "올레드 갤러리"),
            ("com.webos.app.roomconnect", "룸투룸 셰어"),
        ]
        .into_iter()
        .map(|(id, title)| App::new(id, title))
        .collect(),
        inputs: vec![
            Input {
                id: "com.webos.app.hdmi1".into(),
                title: "PlayStation 5".into(),
                port: "HDMI 1".into(),
                connected: Some(true),
                signal: None,
            },
            Input {
                id: "com.webos.app.hdmi2".into(),
                title: "Apple TV".into(),
                port: "HDMI 2".into(),
                connected: Some(true),
                signal: None,
            },
            Input {
                id: "com.webos.app.hdmi3".into(),
                title: "HDMI 3".into(),
                port: "HDMI 3".into(),
                connected: Some(false),
                signal: None,
            },
        ],
    };
    // Desktop-only artwork gallery. TV inventory is always read from services.
    for brand in crate::branding::BRANDS {
        if !inventory.apps.iter().any(|app| app.brand == brand.key) {
            inventory.apps.push(App::new(brand.ids[0], brand.title));
        }
    }
    inventory
}

/// Store the latest picture report on each HDMI input; returns whether any
/// input changed.
pub fn apply_signals(inputs: &mut [Input], signals: &[(u8, bool)]) -> bool {
    let mut changed = false;
    for input in inputs {
        let number = input
            .id
            .strip_prefix("com.webos.app.hdmi")
            .and_then(|n| n.parse::<u8>().ok());
        if let Some(&(_, on)) = signals.iter().find(|(n, _)| Some(*n) == number)
            && input.signal != Some(on)
        {
            input.signal = Some(on);
            changed = true;
        }
    }
    changed
}
