//! Screen geometry and styling mirror the web UI's 1920×1080 / 16px-rem layout.
use crate::{
    i18n::Language,
    model::{App, BACKGROUNDS, CATEGORIES, Input, InputMode},
    motion,
    renderer::{Color, Draw, TextStyle, WHITE},
    state::{
        APPS_COLUMNS, Control, HOME_TILE_PITCH, HOME_TILE_WIDTH, KEYS, Overlay, Page, Rect,
        SYSTEM_SECTION, State, Subpage, Target, TextField, WEATHER_SECTION,
    },
    typography::{HeadingFont, TextFont, TextRole},
    weather::{self, Unit},
};
use std::{
    cell::Cell,
    collections::BTreeMap,
    ops::Range,
    path::{Path, PathBuf},
};
use time::OffsetDateTime;
#[path = "album_view.rs"]
mod album_view;
#[path = "game_view.rs"]
mod game_view;
#[path = "library_view.rs"]
mod library_view;
#[path = "mini_view.rs"]
mod mini_view;
#[path = "movie_view.rs"]
mod movie_view;
#[path = "player_view.rs"]
mod player_view;
#[path = "settings_art.rs"]
mod settings_art;
#[path = "smart_home_settings_view.rs"]
mod smart_home_settings_view;
#[path = "stacks_view.rs"]
mod stacks_view;
#[path = "tvos_settings_view.rs"]
pub(crate) mod tvos_settings_view;
#[path = "widgets_view.rs"]
mod widgets_view;

const MAIN_TOP: f32 = 108.8;
const FACE_HEIGHT: f32 = 154.4;
const GRID_PITCH: f32 = 296.4;

pub struct Assets {
    pub root: PathBuf,
    pub version: String,
    pub metrics: BTreeMap<String, BTreeMap<char, f32>>,
    pub catalog_metrics: BTreeMap<String, BTreeMap<char, f32>>,
    pub active_heading: Cell<HeadingFont>,
    pub active_text: Cell<TextFont>,
    pub heading_pct: Cell<u8>,
    pub text_pct: Cell<u8>,
    pub artwork: crate::app_artwork::Service,
}
impl Assets {
    pub fn load(root: PathBuf, webos: bool) -> Result<Self, String> {
        let load = |file: &str| {
            std::fs::read(root.join(file))
                .map_err(|e| format!("Native assets are missing: {e}. Run pnpm assets."))
        };
        let version = String::from_utf8(load("version.txt")?).map_err(|e| e.to_string())?;
        let metrics =
            serde_json::from_slice(&load("font-metrics.json")?).map_err(|e| e.to_string())?;
        let catalog_metrics = match load("font-catalog-metrics.json") {
            Ok(bytes) => serde_json::from_slice(&bytes).map_err(|e| e.to_string())?,
            Err(_) => BTreeMap::new(),
        };
        Ok(Self {
            root,
            version,
            metrics,
            catalog_metrics,
            active_heading: Cell::new(HeadingFont::default()),
            active_text: Cell::new(TextFont::default()),
            heading_pct: Cell::new(100),
            text_pct: Cell::new(100),
            artwork: crate::app_artwork::Service::new(webos),
        })
    }
    fn path(&self, path: impl AsRef<Path>) -> String {
        self.root.join(path).to_string_lossy().into_owned()
    }

    /// Exact glyph metrics, matched to the renderer's live font selection.
    fn glyph_advance(&self, c: char, style: TextStyle) -> f32 {
        let display = style.display.unwrap_or(false);
        let heading = style.heading_override.unwrap_or(self.active_heading.get());
        let body = style.text_override.unwrap_or(self.active_text.get());
        let family = if display { heading.id() } else { body.id() };
        let weight = if display {
            if family == "nunito" {
                if style.weight >= 850 { 900 } else { 800 }
            } else if matches!(family, "archivo" | "inter" | "unbounded") {
                800
            } else {
                700
            }
        } else if style.weight >= 550 {
            600
        } else if style.weight >= 450 {
            500
        } else {
            400
        };
        let key = format!(
            "{family}-{}-{weight}",
            if display { "heading" } else { "text" }
        );
        let fallback_key = format!(
            "{}-text-{}",
            body.id(),
            if style.weight >= 550 {
                600
            } else if style.weight >= 450 {
                500
            } else {
                400
            }
        );
        let base = if display {
            heading.scale() * self.heading_pct.get() as f32 / 100.
        } else {
            body.scale() * self.text_pct.get() as f32 / 100.
        };
        self.catalog_metrics
            .get(&key)
            .and_then(|m| m.get(&c))
            .or_else(|| {
                self.catalog_metrics
                    .get(&fallback_key)
                    .and_then(|m| m.get(&c))
            })
            .or_else(|| {
                self.metrics
                    .get(&format!(
                        "{}{}",
                        if display { "display-" } else { "" },
                        style.weight
                    ))
                    .and_then(|m| m.get(&c))
            })
            .copied()
            .unwrap_or(if c == ' ' { 0.28 } else { 0.53 })
            * style.size
            * base
    }
    fn char_width(&self, c: char, style: TextStyle, digit_max: f32) -> f32 {
        let display = style.display.unwrap_or(false);
        let base = if display {
            style
                .heading_override
                .unwrap_or(self.active_heading.get())
                .scale()
                * self.heading_pct.get() as f32
                / 100.
        } else {
            style
                .text_override
                .unwrap_or(self.active_text.get())
                .scale()
                * self.text_pct.get() as f32
                / 100.
        };
        (if style.tabular_nums && c.is_ascii_digit() {
            digit_max
        } else {
            self.glyph_advance(c, style)
        }) + style.tracking * style.size * base
    }
    fn max_digit(&self, style: TextStyle) -> f32 {
        if !style.tabular_nums {
            return 0.;
        }
        "0123456789"
            .chars()
            .map(|c| self.glyph_advance(c, style))
            .fold(0., f32::max)
    }
    fn width(&self, value: &str, style: TextStyle) -> f32 {
        let digit_max = self.max_digit(style);
        value
            .chars()
            .map(|c| self.char_width(c, style, digit_max))
            .sum()
    }

    fn lines(&self, value: &str, style: TextStyle, width: f32) -> usize {
        let digit_max = self.max_digit(style);
        let advance = |c: char| self.char_width(c, style, digit_max);
        let mut lines = 1;
        let mut used = 0.;
        for word in value.split_inclusive(char::is_whitespace) {
            let w: f32 = word.chars().map(advance).sum();
            if used > 0. && used + w > width {
                lines += 1;
                used = 0.;
            }
            for c in word.chars() {
                if c == '\n' {
                    lines += 1;
                    used = 0.;
                    continue;
                }
                let w = advance(c);
                if used > 0. && used + w > width {
                    lines += 1;
                    used = 0.;
                }
                used += w;
            }
        }
        lines
    }
}

pub struct Scene {
    pub draw: Vec<Draw>,
    pub controls: Vec<Control>,
    pub modal: Option<Rect>,
    pub body_range: Range<usize>,
    pub settings_sidebar_range: Range<usize>,
    /// Identity of an animated preview; 0 when the preview is static.
    pub settings_sidebar_key: u64,
    pub settings_panel_range: Range<usize>,
    pub overlay_range: Range<usize>,
    pub settings_visible_rows: usize,
    /// Bottom of all page content in screen coordinates, rows below the
    /// screen included (they register no controls); 0 when unknown.
    pub page_bottom: f32,
    /// Background image and tint that frosted plates blur; None = no frost.
    pub frost_source: Option<(String, Color)>,
    /// Draw ranges whose text keeps its own size (scaled widget previews).
    pub keep_text: Vec<Range<usize>>,
}
impl Scene {
    fn new() -> Self {
        Self {
            draw: Vec::new(),
            controls: Vec::new(),
            modal: None,
            body_range: 0..0,
            settings_sidebar_range: 0..0,
            settings_sidebar_key: 0,
            settings_panel_range: 0..0,
            overlay_range: 0..0,
            settings_visible_rows: 0,
            page_bottom: 0.,
            frost_source: None,
            keep_text: Vec::new(),
        }
    }
    pub(crate) fn shape(&mut self, rect: Rect, radius: f32, fill: Color) {
        self.draw.push(Draw::Shape(rect, radius, 0., fill));
    }
    /// Frosted glass: the blurred background under `rect`. Draw the plate's
    /// translucent fill on top. Returns whether there was anything to blur.
    pub(crate) fn frost(&mut self, rect: Rect, radius: f32) -> bool {
        self.frost_dimmed(rect, radius, 1.)
    }
    /// Frost under a plate that sits on a scrim: `keep` is how much of the
    /// background the scrim lets through.
    fn frost_dimmed(&mut self, rect: Rect, radius: f32, keep: f32) -> bool {
        let Some((path, tint)) = self.frost_source.clone() else {
            return false;
        };
        let tint = [tint[0] * keep, tint[1] * keep, tint[2] * keep, tint[3]];
        self.draw.push(Draw::TintedImage(
            format!("{path}#frost"),
            rect,
            radius,
            false,
            tint,
        ));
        true
    }
    /// The one dark plate of the UI (navigation, header circles, Settings
    /// panels, Apps bar, popups): frost, the same translucent fill, the same edge.
    pub(crate) fn glass(&mut self, rect: Rect, radius: f32) {
        self.glass_with(rect, radius, 1., 1.);
    }
    /// The plate faded out by `alpha`, for plates that hand over to a focus fill.
    pub(crate) fn glass_faded(&mut self, rect: Rect, radius: f32, alpha: f32) {
        self.glass_with(rect, radius, 1., alpha);
    }
    /// The plate of a popup over a scrim that lets `keep` of the background through.
    fn glass_on_scrim(&mut self, rect: Rect, radius: f32, keep: f32) {
        self.glass_with(rect, radius, keep, 1.);
    }
    fn glass_with(&mut self, rect: Rect, radius: f32, keep: f32, alpha: f32) {
        if alpha <= 0. {
            return;
        }
        let start = self.draw.len();
        let fill = if self.frost_dimmed(rect, radius, keep) {
            GLASS_FROSTED
        } else {
            GLASS_PLAIN
        };
        self.shape(rect, radius, c(GLASS, fill));
        self.border(rect, radius, 1., c(0xffffff, 0.08));
        if alpha < 1. {
            self.fade(start, alpha, 1.);
        }
    }
    fn border(&mut self, rect: Rect, radius: f32, width: f32, color: Color) {
        self.draw.push(Draw::Shape(rect, radius, width, color));
    }
    fn text(
        &mut self,
        text: impl Into<String>,
        rect: Rect,
        style: TextStyle,
        color: Color,
        center: bool,
    ) {
        self.draw
            .push(Draw::Text(text.into(), rect, style, color, center));
    }
    fn image(&mut self, path: String, rect: Rect, radius: f32, contain: bool) {
        self.draw.push(Draw::Image(path, rect, radius, contain));
    }
    fn icon(&mut self, assets: &Assets, name: &str, rect: Rect, color: Color) {
        self.draw.push(Draw::TintedImage(
            assets.path(format!("ui-icons/{name}.lvec")),
            rect,
            0.,
            true,
            color,
        ));
    }
    fn control(&mut self, target: Target, rect: Rect, scrollable: bool) {
        self.controls.push(Control {
            target,
            rect,
            scrollable,
        });
    }
    fn ring(&mut self, rect: Rect, radius: f32) {
        self.border(rect.inset(-6.9), radius + 6.9, 3.7, c(0xffffff, 0.10));
        self.border(rect.inset(-3.2), radius + 3.2, 3.2, c(0xf0f0f0, 1.));
    }
    fn accent_ring(&mut self, rect: Rect, radius: f32, state: &State) {
        // Thin hairline; the accent glow behind the card does the emphasis.
        let (a, b) = state.accent_colors();
        if state.preferences.accent.kind == "gradient" {
            self.draw.push(Draw::DiagonalGradientBorder(
                rect.inset(-1.5),
                radius + 1.5,
                1.5,
                c(a, 1.),
                c(b, 1.),
            ));
        } else {
            self.border(rect.inset(-1.5), radius + 1.5, 1.5, c(a, 1.));
        }
    }
    /// Home focus frame: a 3 px accent line around a focused card, faded by
    /// focus level, so a dark icon on a dark wallpaper still shows where focus is.
    pub(crate) fn focus_ring(&mut self, rect: Rect, radius: f32, level: f32, state: &State) {
        if level <= 0. {
            return;
        }
        const W: f32 = 3.;
        let (a, b) = state.accent_colors();
        let outer = rect.inset(-W);
        if state.preferences.accent.kind == "gradient" {
            self.draw.push(Draw::DiagonalGradientBorder(
                outer,
                radius + W,
                W,
                c(a, level),
                c(b, level),
            ));
        } else {
            self.border(outer, radius + W, W, c(a, level));
        }
    }
    /// Soft accent halo; draw it right after the card shadow, before the card.
    /// Focus lift: a wide, soft black shadow that pools under the card, so a
    /// focused card floats. Neutral on purpose: colored glows looked cheap.
    fn accent_glow(&mut self, rect: Rect, radius: f32, level: f32, _state: &State) {
        if level <= 0. {
            return;
        }
        let down = |r: Rect, y: f32| Rect::new(r.x, r.y + y, r.w, r.h);
        self.draw.push(Draw::Shadow(
            down(rect.inset(8.), 16.),
            radius,
            90.,
            c(0, 0.3 * level),
        ));
    }
    fn shadow(&mut self, rect: Rect, radius: f32, blur: f32, y: f32, alpha: f32) {
        self.draw.push(Draw::Shadow(
            Rect::new(rect.x, rect.y + y, rect.w, rect.h),
            radius,
            blur,
            c(0, alpha),
        ));
    }
    fn pill(&mut self, rect: Rect, focused: bool, selected: bool) {
        self.pill_progress(rect, if focused { 1. } else { 0. }, selected);
    }
    fn pill_progress(&mut self, rect: Rect, focus: f32, selected: bool) {
        self.shadow(rect, rect.h / 2., 18., 4., 0.06);
        self.shape(
            rect,
            rect.h / 2.,
            if selected {
                c(0x303438, 1.)
            } else {
                c(0x171c20, 0.72)
            },
        );
        self.border(rect, rect.h / 2., 1., c(0xffffff, 0.07));
        if focus > 0. {
            self.draw.push(Draw::RoundedGradient(
                rect,
                rect.h / 2.,
                c(0xfafafa, focus),
                c(0xe5e5e5, focus),
            ));
        }
    }
    /// Animate an already composed group. Hit regions stay stable for the remote.
    fn enter(&mut self, from: usize, state: &State, delay: f32, rise: f32) {
        if state.preferences.reduced_motion {
            return;
        }
        let first = state.first_home && state.page == Page::Home;
        // Restore original orchestration only for the first Home appearance.
        let delay = if first {
            delay
        } else {
            delay.min(3. * motion::STAGGER)
        };
        let duration = if first { 0.36 } else { motion::PAGE_IN - delay };
        let progress = motion::progress(
            (state.page_started.elapsed().as_secs_f32() - delay).max(0.),
            duration.max(0.05),
        );
        if progress >= 1. {
            return;
        }
        // Fast routine navigation; first launch alone gets the expressive entrance.
        let y = if state.preferences.reduced_motion {
            0.
        } else {
            (if first { rise } else { rise.min(16.) }) * (1. - progress)
        };
        motion::transform_group(&mut self.draw[from..], 0., y, progress);
    }
    fn input_cascade(&mut self, from: usize, state: &State, rank: usize) {
        if state.preferences.reduced_motion {
            return;
        }
        let delay = if state.first_home && state.page == Page::Home {
            0.43
        } else {
            0.
        };
        let t = motion::progress(
            (state.page_started.elapsed().as_secs_f32() - delay - motion::cascade_delay(rank))
                .max(0.),
            motion::CARD_IN,
        );
        let rise = motion::CARD_RISE * (1. - t);
        motion::transform_group(&mut self.draw[from..], 0., rise, t);
        if let Some(control) = self.controls.last_mut()
            && matches!(control.target, Target::Input(_))
        {
            control.rect.y += rise;
        }
    }

    fn fade(&mut self, from: usize, alpha: f32, brightness: f32) {
        for draw in &mut self.draw[from..] {
            let apply = |color: &mut Color| {
                for channel in &mut color[..3] {
                    *channel *= brightness;
                }
                color[3] *= alpha;
            };
            match draw {
                Draw::Shape(_, _, _, color)
                | Draw::Shadow(_, _, _, color)
                | Draw::Sheen(_, _, _, color)
                | Draw::Text(_, _, _, color, _)
                | Draw::TintedImage(_, _, _, _, color)
                | Draw::AppImage(_, _, color) => apply(color),
                Draw::RoundedGradient(_, _, a, b)
                | Draw::DiagonalGradient(_, _, a, b)
                | Draw::HGradient(_, _, a, b)
                | Draw::DiagonalGradientBorder(_, _, _, a, b) => {
                    apply(a);
                    apply(b);
                }
                Draw::Image(path, rect, radius, contain) => {
                    *draw = Draw::TintedImage(
                        path.clone(),
                        *rect,
                        *radius,
                        *contain,
                        [brightness, brightness, brightness, alpha],
                    );
                }
                Draw::Clip(_) | Draw::RoundClip(_) | Draw::EdgeFade(_) => {}
            }
        }
    }
}

fn blend(a: Color, b: Color, t: f32) -> Color {
    let t = t.clamp(0., 1.);
    [0, 1, 2, 3].map(|i| a[i] * (1. - t) + b[i] * t)
}
fn c(rgb: u32, alpha: f32) -> Color {
    [
        ((rgb >> 16) & 255) as f32 / 255.,
        ((rgb >> 8) & 255) as f32 / 255.,
        (rgb & 255) as f32 / 255.,
        alpha,
    ]
}
/// Map every legacy callsite to the single television type scale.
/// Geometry and focus bounds stay authored independently of font metrics.
fn text(size: f32, weight: u16, _tracking: f32) -> TextStyle {
    if size < 27. {
        // Small labels keep their own size and a light weight: rounding them
        // all up to the 25/600 caption made every hint big and bold. TV
        // floor 18 px; only 500 or 600 (no thin faces, no 700).
        let token = TextRole::Caption.token();
        return TextStyle::new(size.max(18.))
            .weight(if weight >= 600 { 600 } else { 500 })
            .tracking(token.tracking)
            .leading(token.line_height)
            .display(false)
            .tabular();
    }
    let role = if size >= 80. {
        TextRole::Hero
    } else if size >= 42. {
        TextRole::Title
    } else if size >= 33. {
        TextRole::Headline
    } else if size >= 30. {
        TextRole::Body
    } else if size >= 27. {
        TextRole::Callout
    } else {
        TextRole::Caption
    };
    let token = role.token();
    TextStyle::new(token.size)
        .weight(token.weight)
        .tracking(token.tracking)
        .leading(token.line_height)
        .display(false)
        .tabular()
}

/// Give the renderer the complete control bounds so its font ascent/descent metrics
/// can place the baseline optically in the vertical center. Never fake centering
/// with a fixed top inset; that drifts across weights and fallback faces.
fn centered_label(rect: Rect, leading: f32, trailing: f32) -> Rect {
    Rect::new(
        rect.x + leading,
        rect.y,
        (rect.w - leading - trailing).max(0.),
        rect.h,
    )
}

pub struct Navigation {
    pub surface: Rect,
    pub items: Vec<Rect>,
}
pub fn navigation_for(assets: &Assets, language: Language, show_smart: bool) -> Navigation {
    let keys = if show_smart {
        vec!["home", "apps", "Smart Home", "settings"]
    } else {
        vec!["home", "apps", "settings"]
    };
    let widths: Vec<f32> = keys
        .iter()
        .map(|key| {
            (assets.width(
                if *key == "Smart Home" {
                    key
                } else {
                    language.t(key)
                },
                nav_label_style(),
            ) + 2. * NAV_TAB_PAD_X)
                .max(104.)
        })
        .collect();
    // One padding everywhere: the pill sits NAV_PAD from every surface edge.
    let width =
        widths.iter().sum::<f32>() + 2. * NAV_PAD + (widths.len() as f32 - 1.) * NAV_TAB_GAP;
    let mut x = (1920. - width) / 2. + NAV_PAD;
    let items = widths
        .into_iter()
        .map(|width| {
            let rect = Rect::new(x, NAV_TOP + NAV_PAD, width, NAV_PILL);
            x += width + NAV_TAB_GAP;
            rect
        })
        .collect();
    Navigation {
        surface: Rect::new((1920. - width) / 2., NAV_TOP, width, NAV_HEIGHT),
        items,
    }
}

/// Full-resolution first frame of a wallpaper clip, shown while it loads.
pub fn video_still_path(assets: &Assets, index: usize) -> String {
    assets.path(format!(
        "video-stills/{}.jpg",
        index.min(crate::video_catalog::all().len() - 1)
    ))
}

pub fn scene_with_pill(
    state: &State,
    assets: &Assets,
    pill_x: f32,
    pill_width: f32,
    now: OffsetDateTime,
) -> Scene {
    settings_art::set_now(now);
    let mut out = Scene::new();
    if state.preferences.background == "video" {
        // Keep one stable scrim over both the transition still and the live
        // hardware plane. The still fades out only after the first real frame,
        // so there is never a transparent/black gap or a bright-to-dim jump.
        let handoff_progress = state.video_transition_started.map_or(
            if state.video_frame_ready { 1. } else { 0. },
            |started| {
                (started.elapsed().as_secs_f32() / crate::state::VIDEO_HANDOFF.as_secs_f32())
                    .clamp(0., 1.)
            },
        );
        let still_alpha = (1. - handoff_progress).clamp(0., 1.);
        if still_alpha > 0. {
            out.draw.push(Draw::TintedImage(
                video_still_path(assets, state.preferences.video_index),
                Rect::new(0., 0., 1920., 1080.),
                0.,
                false,
                // The still is the clip's exact first frame, at full brightness
                // under the same scrim, so the live plane takes over in place.
                [1., 1., 1., still_alpha],
            ));
        }
        let clear = if state.page == Page::Home {
            state.ambient_away(0.)
        } else {
            0.
        };
        out.shape(
            Rect::new(0., 0., 1920., 1080.),
            0.,
            c(0x04090f, 0.23 * (1. - clear)),
        );
    } else if let Some((previous, start)) = &state.previous_background {
        out.image(
            assets.path(format!("wallpapers/{previous}.jpg")),
            Rect::new(0., 0., 1920., 1080.),
            0.,
            false,
        );
        let alpha = (start.elapsed().as_secs_f32() / 0.4).min(1.);
        out.draw.push(Draw::TintedImage(
            assets.path(format!("wallpapers/{}.jpg", state.preferences.background)),
            Rect::new(0., 0., 1920., 1080.),
            0.,
            false,
            [1., 1., 1., alpha],
        ));
    } else {
        out.image(
            assets.path(format!("wallpapers/{}.jpg", state.preferences.background)),
            Rect::new(0., 0., 1920., 1080.),
            0.,
            false,
        );
    }
    // Frosted plates blur what lies under them. Live video is a hardware
    // plane GL never sees, so it is stood in for by the clip's still.
    // Settings sits on the same wallpaper as every page (the
    // extra dark scrim there looked cheap); its plates are plain glass.
    let dim = 1.;
    out.frost_source = Some(if state.preferences.background == "video" {
        (
            video_still_path(assets, state.preferences.video_index),
            [0.6 * dim, 0.6 * dim, 0.6 * dim, 1.],
        )
    } else {
        (
            assets.path(format!("wallpapers/{}.jpg", state.preferences.background)),
            [dim, dim, dim, 1.],
        )
    });
    // Home: a black floor under the rows so cards read over any wallpaper.
    // It fades out at the weather widget and lies behind the cards, never over them.
    if state.page == Page::Home {
        let clear = state.ambient_away(0.);
        out.draw.push(Draw::RoundedGradient(
            Rect::new(0., 300., 1920., 780.),
            0.,
            c(0x000000, 0.),
            c(0x000000, 0.6 * (1. - clear)),
        ));
    }
    let main_bottom = state.main_bottom();
    let body_start = out.draw.len();
    // Horizontal Favorites media may reach the physical screen edge; keep
    // Apps and Settings inside their established 64.0.1856 content fields.
    let (body_left, body_width) = if matches!(state.page, Page::Home | Page::Settings) {
        (0., 1920.)
    } else {
        (64., 1792.)
    };
    out.draw.push(Draw::Clip(Some(Rect::new(
        body_left,
        if state.page == Page::Settings {
            0.
        } else {
            MAIN_TOP
        },
        body_width,
        main_bottom
            - if state.page == Page::Settings {
                0.
            } else {
                MAIN_TOP
            },
    ))));
    if state.page == Page::Home {
        // Scrolled Home content dissolves before it reaches the clock and date;
        // the bottom stays open, posters run to the screen edge unmasked.
        out.draw
            .push(Draw::EdgeFade(Some([150., 206., 2000., 2001.])));
    }
    if state.page == Page::SmartHome {
        // Like Home: scrolled tiles are gone before they reach the clock and date.
        out.draw
            .push(Draw::EdgeFade(Some([150., SMART_REVEAL_TOP, 2000., 2001.])));
    }
    if state.page == Page::Apps {
        // Scrolled tiles dissolve under the header only; the bottom stays open.
        out.draw
            .push(Draw::EdgeFade(Some([MAIN_TOP, 152.8, 2000., 2001.])));
    }
    match state.page {
        Page::Home => home(&mut out, state, assets, now),
        Page::Apps => library(&mut out, state, assets),
        Page::SmartHome => smart_home_view(&mut out, state, assets),
        Page::Settings => settings(&mut out, state, assets),
    }
    // Living-room typography guarantee across legacy subpages and Smart Home.
    if state.page == Page::Settings {
        for (index, command) in out.draw.iter_mut().enumerate().skip(body_start) {
            if out.keep_text.iter().any(|r| r.contains(&index)) {
                continue;
            }
            if let Draw::Text(_, _, style, _, _) = command {
                style.size = style.size.max(25.);
                style.weight = style.weight.max(500);
            }
        }
    }
    // All animated body content is inside the content clip and below the stable header.
    let body_end = out.draw.len();
    if state.page == Page::Settings {
        out.settings_panel_range.end = body_end;
        // Native tvOS entrance uses the Home/Apps cascade: left glass preview,
        // right glass surface, then each right-hand row in reading order.
        // Depth and Back share the same entrance: zero horizontal movement.
        let elapsed = state
            .panel_started
            .map(|start| start.elapsed().as_secs_f32())
            .unwrap_or_else(|| state.page_started.elapsed().as_secs_f32());
        // The last frame's glass backdrop is a reference for a 220ms height
        // interpolation. This only changes h, never x/y/opacity or scale.
        let outgoing = state.previous_panel_layer.as_ref();
        let height_progress = outgoing
            .map(|(_, started, _)| motion::progress(started.elapsed().as_secs_f32(), 0.220))
            .unwrap_or(1.);
        let body = out.settings_panel_range.clone();
        for command in &mut out.draw[body] {
            if let Some(backdrop) = tvos_settings_view::backdrop_rect(command) {
                if let Some((old, _, _)) = outgoing {
                    let old_h = old
                        .iter()
                        .filter_map(tvos_settings_view::backdrop_rect)
                        .find(|r| {
                            (r.x - backdrop.x).abs() < 0.01 && (r.w - backdrop.w).abs() < 0.01
                        })
                        .map(|r| r.h)
                        .unwrap_or(backdrop.h);
                    let next_height = old_h + (backdrop.h - old_h) * height_progress;
                    match command {
                        Draw::Shape(r, _, _, _) | Draw::Shadow(r, _, _, _) => r.h = next_height,
                        _ => {}
                    }
                }
                // Glass is always spatially/visually steady, even during
                // first tab entry and interrupted navigation.
                continue;
            }
            // Direct scene inspections (without a previously composed frame) are
            // fully settled. Real navigation has a previous frame to animate from.
            if state.last_body.is_empty() {
                continue;
            }
            let rect = match command {
                Draw::Shape(rect, _, _, _)
                | Draw::RoundedGradient(rect, _, _, _)
                | Draw::DiagonalGradient(rect, _, _, _)
                | Draw::HGradient(rect, _, _, _)
                | Draw::DiagonalGradientBorder(rect, _, _, _, _)
                | Draw::Shadow(rect, _, _, _)
                | Draw::Sheen(rect, _, _, _)
                | Draw::Text(_, rect, _, _, _)
                | Draw::Image(_, rect, _, _)
                | Draw::TintedImage(_, rect, _, _, _)
                | Draw::AppImage(_, rect, _) => Some(*rect),
                Draw::Clip(_) | Draw::RoundClip(_) | Draw::EdgeFade(_) => None,
            };
            let Some(rect) = rect else {
                continue;
            };
            let rank = if rect.x < tvos_settings_view::RIGHT {
                0
            } else if rect.x <= tvos_settings_view::RIGHT + 1.
                && rect.w >= tvos_settings_view::WIDTH - 1.
            {
                1
            } else {
                let y = (rect.y - (tvos_settings_view::TOP + tvos_settings_view::S24)).max(0.);
                2 + (y / tvos_settings_view::STEP).floor() as usize
            };
            let (progress, rise) = if state.preferences.reduced_motion {
                (motion::progress(elapsed, 0.120), 0.)
            } else {
                let progress = motion::cascade_progress(elapsed, rank);
                (progress, 12. * (1. - progress))
            };
            motion::transform(command, 0., rise, progress);
        }
        // Controls retain their static hitboxes so navigation never jumps.
    } else if !(state.first_home && state.page == Page::Home) {
        let page_t = motion::progress(state.page_started.elapsed().as_secs_f32(), motion::PAGE_IN);
        let direction = state
            .previous_page_layer
            .as_ref()
            .map_or(1., |(_, _, dir)| *dir);
        let dx = 0.;
        let _ = direction;
        motion::transform_group(&mut out.draw[body_start..body_end], dx, 0., page_t);
        for control in &mut out.controls {
            control.rect.x += dx;
        }
    }
    // Crossfade frozen snapshots of the outgoing composited content. When another
    // navigation arrives mid-transition, compose() retargets from the visible frame.
    if let Some((previous, started, direction)) = &state.previous_page_layer {
        let t = motion::progress(started.elapsed().as_secs_f32(), motion::PAGE_OUT);
        let dx = 0.;
        let _ = direction;
        let mut layer = previous.clone();
        // Bright content (logos, icons, text) leaves first and card surfaces
        // last, the mirror of the Apps entrance: a uniform fade left white
        // logos floating over see-through squares.
        let a = 1. - t;
        for draw in &mut layer {
            let alpha = match draw {
                Draw::Text(..) | Draw::Image(..) | Draw::TintedImage(..) | Draw::AppImage(..) => {
                    a * a
                }
                _ => (a * 2.2).min(1.),
            };
            motion::transform(draw, dx, 0., alpha);
        }
        out.draw.extend(layer);
    } else if state.page == Page::Settings {
        if let Some((previous, started, _is_subpage)) = &state.previous_panel_layer {
            let progress = motion::progress(started.elapsed().as_secs_f32(), motion::PANEL_OUT);
            let mut layer = previous
                .iter()
                .filter(|d| tvos_settings_view::backdrop_rect(d).is_none())
                .cloned()
                .collect::<Vec<_>>();
            motion::transform_group(&mut layer, 0., 0., 1. - progress);
            out.draw.extend(layer);
        }
        // Moving focus without changing the preview draws the same content
        // twice; overlaying it made text and the miniature flicker.
        // Animated previews change every frame, so they compare by identity.
        if let Some((previous, started)) = &state.previous_sidebar_layer
            && if out.settings_sidebar_key != 0 && state.previous_sidebar_key != 0 {
                out.settings_sidebar_key != state.previous_sidebar_key
            } else {
                previous.as_slice() != &out.draw[out.settings_sidebar_range.clone()]
            }
        {
            let t = motion::progress(started.elapsed().as_secs_f32(), motion::FOCUS_IN);
            let mut layer = previous.clone();
            motion::transform_group(&mut layer, 0., 0., 1. - t);
            out.draw.extend(layer);
        }
    }
    out.body_range = body_start..out.draw.len();
    out.draw.push(Draw::EdgeFade(None));
    out.draw.push(Draw::Clip(None));
    let header_start = out.draw.len();
    header(&mut out, state, assets, now, pill_x, pill_width);
    if state.page != Page::Settings {
        footer(&mut out, state, assets);
    }
    if state.page == Page::Home {
        // Clock, then navigation, then the circles.
        ambient_move(&mut out.draw[header_start..], state, 0., -260., |r| {
            2. * r.x / 1920.
        });
        if state.ambient_on() {
            out.controls.clear();
        }
        // Clean wallpaper: the clip's name, small, bottom left, as on tvOS.
        let shown = state.ambient_away(4. * AMBIENT_STAGGER);
        if shown > 0. && state.preferences.background == "video" {
            let name = crate::video_catalog::all()
                .get(state.preferences.video_index)
                .map(|v| v.name.trim())
                .unwrap_or("");
            if !name.is_empty() {
                let style = TextStyle::new(24.).weight(500).tracking(0.01);
                let rect = Rect::new(84., 1080. - 72. - 32., 1200., 32.);
                out.text(
                    name,
                    Rect::new(rect.x, rect.y + 1.5, rect.w, rect.h),
                    style,
                    c(0x000000, 0.35 * shown),
                    false,
                );
                out.text(name, rect, style, c(0xffffff, 0.82 * shown), false);
            }
        }
    }
    if state.ha_detail.is_some() {
        smart_home_detail(&mut out, state, assets);
    }
    if state.ha_camera.is_some() {
        smart_home_camera(&mut out, state, assets);
    }
    if state.ha_camera_menu.is_some() {
        smart_camera_menu(&mut out, state, assets);
    }
    if state.library.is_some() || state.library_closing.is_some() {
        library_view::library_page(&mut out, state, assets);
    }
    if state.movie.is_some() || state.movie_closing.is_some() {
        movie_view::movie_card(&mut out, state, assets);
    } else {
        movie_view::warm(&mut out, state);
    }
    if state.game.is_some() || state.game_closing.is_some() {
        game_view::game_card(&mut out, state, assets);
    }
    if state.album.is_some() || state.album_closing.is_some() {
        album_view::album_card(&mut out, state, assets);
    }
    if state.widget_detail.is_some() {
        widgets_view::detail(&mut out, state, assets);
    }
    if state.playback.is_some() {
        player_view::draw(&mut out, state, assets);
        out.body_range = 0..0;
        out.settings_panel_range = 0..0;
        out.settings_sidebar_range = 0..0;
    }
    if let Some(overlay @ (Overlay::Options { .. } | Overlay::Keyboard)) = &state.overlay {
        out.controls.clear();
        let start = out.draw.len();
        out.shape(Rect::new(0., 0., 1920., 1080.), 0., c(0x080b12, 0.70));
        match overlay {
            Overlay::Options { id, .. } => options(&mut out, state, assets, id),
            Overlay::Keyboard => keyboard(&mut out, state, assets),
            Overlay::SystemKeyboard => {}
        }
        if let Some(opened) = state.panel_started {
            let t = motion::progress(
                opened.elapsed().as_secs_f32(),
                if state.preferences.reduced_motion {
                    motion::PANEL_OUT
                } else {
                    motion::PANEL_IN
                },
            );
            let dy = if state.preferences.reduced_motion {
                0.
            } else {
                12. * (1. - t)
            };
            motion::transform_group(&mut out.draw[start..], 0., dy, t);
            for control in &mut out.controls {
                control.rect.y += dy;
            }
        }
        out.overlay_range = start..out.draw.len();
    }
    if let Some((previous, started)) = &state.previous_overlay_layer {
        let t = motion::progress(started.elapsed().as_secs_f32(), motion::PANEL_OUT);
        let dy = if state.preferences.reduced_motion {
            0.
        } else {
            12. * t
        };
        let mut layer = previous.clone();
        motion::transform_group(&mut layer, 0., dy, 1. - t);
        out.draw.extend(layer);
    }
    if let Some((notice, _)) = &state.notice {
        toast(&mut out, state, assets, notice);
    }
    out
}

pub(crate) fn contrast_text_for_accent(a: u32, b: u32) -> Color {
    let luma = |v: u32| {
        let channel = |shift| {
            let srgb = ((v >> shift) & 0xffu32) as f32 / 255.;
            if srgb <= 0.04045 {
                srgb / 12.92
            } else {
                ((srgb + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * channel(16) + 0.7152 * channel(8) + 0.0722 * channel(0)
    };
    let mean = (luma(a) + luma(b)) * 0.5;
    if mean > 0.21 { c(0x121519, 1.) } else { WHITE }
}

fn header(
    out: &mut Scene,
    state: &State,
    assets: &Assets,
    now: OffsetDateTime,
    pill_x: f32,
    pill_width: f32,
) {
    let language = state.preferences.language;
    let clock = language.clock(
        now,
        state.preferences.hour_24,
        state.preferences.show_seconds,
    );
    let layout = navigation_for(assets, language, state.show_smart_tab());
    // The clock digits are exactly as tall as the selected navigation pill
    // (52 px, NAV_TOP + NAV_PAD to its bottom); the date sits close under it.
    // Bitcoin stays in state for a future widget.
    let clock_style = heading_fixed(assets, 64., 700).tracking(-0.03);
    out.text(
        &clock,
        Rect::new(crate::state::HOME_STRIP_LEFT, 49.5, 760., 78.),
        clock_style,
        WHITE,
        false,
    );
    // Caps start 0.41 of the size below the box top: 14 px under the digits.
    out.text(
        language.date(now),
        Rect::new(
            crate::state::HOME_STRIP_LEFT,
            NAV_TOP + NAV_PAD + NAV_PILL + 14. - 0.41 * 27.,
            760.,
            40.,
        ),
        text(27., 500, -0.02),
        c(0xf0f3f6, 0.82),
        false,
    );
    let nav = layout.surface;
    out.shadow(nav, NAV_HEIGHT / 2., 18., 4., 0.06);
    out.glass(nav, NAV_HEIGHT / 2.);
    let pill = Rect::new(pill_x, NAV_TOP + NAV_PAD, pill_width, NAV_PILL);
    let nav_accent = state.preferences.accent.use_nav;
    let (first, second) = state.accent_colors();
    // The selected tab is one solid accent surface with dark-or-light ink chosen
    // by contrast. No outline: the fill itself is the selection and focus cue.
    let selected_nav_color = if nav_accent {
        accent_ink(state)
    } else {
        c(0x15171b, 1.)
    };
    out.draw.push(if nav_accent {
        if state.preferences.accent.kind == "gradient" {
            Draw::DiagonalGradient(pill, NAV_PILL / 2., c(first, 1.), c(second, 1.))
        } else {
            Draw::RoundedGradient(pill, NAV_PILL / 2., c(first, 1.), c(first, 1.))
        }
    } else {
        Draw::RoundedGradient(pill, NAV_PILL / 2., c(0xfafafa, 1.), c(0xe5e5e5, 1.))
    });
    let keys = if state.show_smart_tab() {
        vec!["home", "apps", "Smart Home", "settings"]
    } else {
        vec!["home", "apps", "settings"]
    };
    for (index, key) in keys.into_iter().enumerate() {
        let rect = layout.items[index];
        out.text(
            if key == "Smart Home" {
                key
            } else {
                language.t(key)
            },
            line_box(rect, nav_label_style()),
            nav_label_style(),
            {
                let overlap = ((pill.x + pill.w).min(rect.x + rect.w) - pill.x.max(rect.x)).max(0.);
                let coverage = (overlap / rect.w).clamp(0., 1.);
                blend(c(0xeeeeee, 0.73), selected_nav_color, coverage)
            },
            true,
        );
        out.control(Target::Nav(index), rect, false);
    }
    header_circles(out, state, assets);
    if state.page == Page::Home && state.music.now.is_some() {
        mini_view::mini_player(out, state, assets);
    }
}

/// Text color on an accent surface, picked by contrast against the actual fill.
pub(crate) fn accent_ink(state: &State) -> Color {
    let (a, b) = state.accent_colors();
    let b = if state.preferences.accent.kind == "gradient" {
        b
    } else {
        a
    };
    contrast_text_for_accent(a, b)
}

/// Fill a rounded rect with the user's accent (solid or diagonal gradient).
fn accent_fill(out: &mut Scene, rect: Rect, radius: f32, state: &State) {
    let (a, b) = state.accent_colors();
    if state.preferences.accent.kind == "gradient" {
        out.draw
            .push(Draw::DiagonalGradient(rect, radius, c(a, 1.), c(b, 1.)));
    } else {
        out.shape(rect, radius, c(a, 1.));
    }
}

pub(crate) const NAV_TOP: f32 = 44.;
pub(crate) const NAV_HEIGHT: f32 = 68.;
pub(crate) const NAV_PAD: f32 = 8.;
pub(crate) const NAV_PILL: f32 = NAV_HEIGHT - 2. * NAV_PAD;
const NAV_TAB_PAD_X: f32 = 28.;
const NAV_TAB_GAP: f32 = 4.;
/// Header circles (inputs and TV settings) match the navigation height.
pub(crate) const HEADER_CIRCLE: f32 = NAV_HEIGHT;
const HEADER_CIRCLE_GAP: f32 = 16.;

fn nav_label_style() -> TextStyle {
    text(20.8, 550, -0.02)
}

/// One line box vertically centered inside `rect`. The renderer places a line
/// at the rect top, so centering must be resolved here from the line height.
fn line_box(rect: Rect, style: TextStyle) -> Rect {
    let line = style.size * style.line_height;
    Rect::new(rect.x, rect.y + (rect.h - line) / 2., rect.w, line + 1.)
}

/// Header circles, right to left: TV settings first, then each visible input.
/// A newly connected HDMI simply appears further left.
pub(crate) fn header_circle_targets(state: &State) -> Vec<(Target, String, String)> {
    let mut items = vec![(
        Target::TvSettings,
        "settings".to_owned(),
        state.preferences.language.t("settings").to_owned(),
    )];
    for input in state
        .inventory
        .inputs
        .iter()
        .filter(|input| state.preferences.input_visible(input))
    {
        let name = state.preferences.input_name(input);
        let icon = format!("input-{}", input_icon(input, name));
        let title = state.preferences.input_title(input);
        let caption = if title == input.port {
            title.to_owned()
        } else {
            format!("{title} · {}", input.port)
        };
        items.push((Target::Input(input.id.clone()), icon, caption));
    }
    items
}

pub(crate) fn header_circle_rect(slot: usize) -> Rect {
    Rect::new(
        crate::state::HOME_STRIP_RIGHT
            - HEADER_CIRCLE
            - slot as f32 * (HEADER_CIRCLE + HEADER_CIRCLE_GAP),
        NAV_TOP,
        HEADER_CIRCLE,
        HEADER_CIRCLE,
    )
}

fn header_circles(out: &mut Scene, state: &State, assets: &Assets) {
    for (slot, (target, icon, caption)) in header_circle_targets(state).into_iter().enumerate() {
        let rect = header_circle_rect(slot);
        let focus = state.focus_level(&target);
        let lift = rect.inset(-2.7 * focus);
        let radius = lift.w / 2.;
        out.shadow(
            lift,
            radius,
            8. + 16. * focus,
            2. + 8. * focus,
            0.06 + 0.24 * focus,
        );
        out.glass_faded(lift, radius, 1. - focus);
        if focus > 0. {
            let start = out.draw.len();
            accent_fill(out, lift, radius, state);
            out.fade(start, focus, 1.);
        }
        let ink = blend(c(0xffffff, 0.92), accent_ink(state), focus);
        let size = if icon == "input-apple-tv" { 38. } else { 30. };
        out.icon(
            assets,
            &icon,
            Rect::new(
                rect.x + (rect.w - size) / 2.,
                rect.y + (rect.h - size) / 2.,
                size,
                size,
            ),
            ink,
        );
        if focus > 0. && !state.pointer {
            let style = TextStyle::new(20.).weight(600).leading(1.3);
            let width = assets.width(&caption, style) + 8.;
            // Keep the caption on screen even for the rightmost circle.
            let x = (rect.x + rect.w / 2. - width / 2.).min(1920. - 24. - width);
            out.text(
                caption,
                Rect::new(x, rect.y + rect.h + 12., width, 28.),
                style,
                c(0xffffff, 0.95 * focus),
                true,
            );
        }
        out.control(target, rect, false);
    }
}

/// App tiles and stack posters share one generous corner radius.
pub(crate) const TILE_RADIUS: f32 = 26.;

/// Whether a tile shows its caption and its pinned mark.
struct TileOptions {
    names: bool,
    pinned: bool,
}

fn tile(
    out: &mut Scene,
    state: &State,
    assets: &Assets,
    app: &App,
    rect: Rect,
    options: TileOptions,
    entrance_index: usize,
) {
    let TileOptions { names, pinned } = options;
    let caption_height = if names {
        caption_height(state, assets, app, rect.w)
    } else {
        0.
    };
    let label_height = if names { 18.4 + caption_height } else { 0. };
    out.control(
        Target::App(app.id.clone()),
        Rect::new(rect.x, rect.y, rect.w, rect.h + label_height),
        true,
    );
    if rect.y + rect.h + label_height < MAIN_TOP
        || rect.y > 1054.
        || rect.x + rect.w + 40. < 0.
        || rect.x - 40. > 1920.
    {
        return;
    }
    let start = out.draw.len();
    let focused = state.focus == Target::App(app.id.clone());
    // One beat per visible card on EVERY tab entry, up to eight in reading order.
    // Hidden/offscreen cards aren't submitted at all and never consume stagger slots.
    let entrance_eased = if state.preferences.reduced_motion {
        1.
    } else {
        motion::cascade_progress(state.page_started.elapsed().as_secs_f32(), entrance_index)
    };
    let rise = if state.preferences.reduced_motion {
        0.
    } else {
        motion::CARD_RISE * (1. - entrance_eased)
    };
    let rect = Rect::new(rect.x, rect.y + rise, rect.w, rect.h);
    let level = state.focus_level(&Target::App(app.id.clone()));
    let (pop, sheen) = state.focus_pop(&Target::App(app.id.clone()));
    let zoom = if state.preferences.reduced_motion {
        1.
    } else {
        1. + 0.07 * pop
    };
    let rect = Rect::new(
        rect.x - rect.w * (zoom - 1.) / 2.,
        rect.y - rect.h * (zoom - 1.) / 2.,
        rect.w * zoom,
        rect.h * zoom,
    );
    // The Apps grid repaints dozens of cards each frame of a focus sheen:
    // resting cards there skip their blurred shadow to keep the sheen smooth.
    if state.page != Page::Apps || level > 0. {
        out.shadow(
            rect,
            TILE_RADIUS,
            26. + 14. * level,
            8. + 4. * level,
            0.14 + 0.1 * level,
        );
    }
    out.accent_glow(rect, TILE_RADIUS, level, state);
    let logo = crate::branding::for_key(&app.brand);
    let artwork = logo
        .is_none()
        .then(|| {
            app.icon
                .as_ref()
                .and_then(|source| assets.artwork.get(source))
        })
        .flatten();
    if let Some(image) = artwork.as_ref() {
        let rgb = image.color;
        out.shape(
            rect,
            TILE_RADIUS,
            [
                rgb[0] as f32 / 255.,
                rgb[1] as f32 / 255.,
                rgb[2] as f32 / 255.,
                // App cards are always opaque: LG ships square JPEG/PNG icons
                // that show as boxes on any translucent card.
                1.,
            ],
        );
    } else {
        out.draw.push(Draw::TintedImage(
            assets.path(format!("surfaces/{}.png", app.brand)),
            rect,
            TILE_RADIUS,
            false,
            [1.; 4],
        ));
    }
    out.border(rect, TILE_RADIUS, 1., c(0xffffff, 0.063));
    out.focus_ring(rect, TILE_RADIUS, level, state);
    let surface_end = out.draw.len();
    if let Some(brand) = logo {
        let (ratio, h) = brand.artwork_size();
        out.image(
            assets.path(format!("logos/{}.lvec", app.brand)),
            Rect::new(
                rect.x + rect.w * (1. - ratio) / 2.,
                rect.y + (rect.h - h) / 2.,
                rect.w * ratio,
                h,
            ),
            0.,
            true,
        );
    } else if let Some(image) = artwork {
        out.draw.push(Draw::AppImage(
            image,
            Rect::new(
                rect.x + rect.w * 0.16,
                rect.y + rect.h * 0.16,
                rect.w * 0.68,
                rect.h * 0.68,
            ),
            [1.; 4],
        ));
    } else {
        brand_mark(out, assets, app, rect, state.preferences.language);
    }
    if pinned {
        out.icon(
            assets,
            "heart-filled",
            Rect::new(rect.x + rect.w - 27.2, rect.y + 11.2, 16., 16.),
            c(0xffffff, 0.9),
        );
    }
    if state.moving.as_ref() == Some(&app.id) {
        let indicator = Rect::new(
            rect.x + rect.w / 2. - 37.6,
            rect.y + rect.h - 33.6,
            75.2,
            24.,
        );
        out.shape(indicator, 16., c(0x101318, 0.79));
        out.icon(
            assets,
            "left",
            Rect::new(indicator.x + 8.8, indicator.y + 4., 16., 16.),
            c(0xededed, 1.),
        );
        out.icon(
            assets,
            "arrow",
            Rect::new(indicator.x + 40., indicator.y + 4., 16., 16.),
            c(0xededed, 1.),
        );
    }
    if let Some(progress) = sheen {
        out.draw
            .push(Draw::Sheen(rect, TILE_RADIUS, progress, c(0xffffff, 0.14)));
    }
    // App tiles show focus by zoom, sheen and lift; no outline.
    if state.pressed.as_ref() == Some(&Target::App(app.id.clone())) {
        out.fade(start, 1., 0.85);
    }
    if names {
        out.text(
            &app.title,
            Rect::new(
                rect.x + 4.,
                rect.y + rect.h + 18.4,
                rect.w - 8.,
                caption_height,
            ),
            text(17.6, if focused { 550 } else { 450 }, -0.02).leading(1.4),
            if focused {
                c(0xffffff, 1.)
            } else {
                c(0xececec, 0.66)
            },
            true,
        );
    }
    if state.moving.is_some() && state.moving.as_ref() != Some(&app.id) {
        out.fade(start, 0.48, 1.);
    }
    if entrance_eased < 1. {
        // The card surface leads and the bright logo and caption follow: a
        // uniform fade showed white logos over see-through squares first.
        let content = out.draw.split_off(surface_end);
        out.fade(start, (entrance_eased * 2.2).min(1.), 1.);
        let from = out.draw.len();
        out.draw.extend(content);
        out.fade(from, entrance_eased * entrance_eased, 1.);
    }
}

fn caption_height(state: &State, assets: &Assets, app: &App, width: f32) -> f32 {
    let title = &app.title;
    let style = text(
        17.6,
        if state.focus == Target::App(app.id.clone()) {
            550
        } else {
            450
        },
        -0.02,
    )
    .leading(1.4);
    (assets.lines(title, style, width - 8.) as f32 * style.size * style.line_height).max(49.28)
}

fn brand_mark(out: &mut Scene, assets: &Assets, app: &App, rect: Rect, language: Language) {
    let (cx, cy) = rect.center();
    match app.brand.as_str() {
        "plex" => {
            out.text(
                "ple",
                Rect::new(cx - 63., cy - 35.2, 130., 70.4),
                text(70.4, 650, -0.08).leading(1.),
                WHITE,
                false,
            );
            out.text(
                "x",
                Rect::new(cx + 20., cy - 35.2, 60., 70.4),
                text(70.4, 800, -0.08).leading(1.),
                c(0xe6aa25, 1.),
                false,
            );
        }
        "spotify" => {
            out.icon(
                assets,
                "spotify",
                Rect::new(cx - 77., cy - 20., 40., 40.),
                c(0x5ae08c, 1.),
            );
            out.text(
                "Spotify",
                Rect::new(cx - 29., cy - 14.4, 120., 36.),
                text(28.8, 650, -0.04).leading(1.),
                c(0x5ae08c, 1.),
                false,
            );
        }
        "kodi" => {
            out.icon(
                assets,
                "kodi",
                Rect::new(cx - 76., cy - 22.5, 45., 45.),
                WHITE,
            );
            out.text(
                "kodi",
                Rect::new(cx - 18., cy - 24.8, 130., 60.),
                text(49.6, 350, -0.04).leading(1.),
                WHITE,
                false,
            );
        }
        brand => {
            let (icon, label) = match brand {
                "live-tv" => ("tv", language.t("liveTV")),
                "homebrew" => ("coffee", language.t("homebrew")),
                "browser" => ("globe", language.t("browser")),
                "moonlight" => ("moonlight", "Moonlight"),
                "gaming" => ("gaming", app.title.as_str()),
                _ => ("generic", app.title.as_str()),
            };
            let font = if brand == "generic" {
                text(18.4, 500, -0.035).leading(1.35)
            } else {
                text(24., 500, -0.035).leading(1.2)
            };
            let w = (assets.width(label, font) + 54.4).min(rect.w - 32.);
            let label_w = rect.w - 54.4 - 32.;
            let label_h =
                (assets.lines(label, font, label_w) as f32 * font.size * font.line_height)
                    .min(rect.h - 32.);
            out.icon(
                assets,
                icon,
                Rect::new(cx - w / 2., cy - 20.8, 41.6, 41.6),
                WHITE,
            );
            out.text(
                label,
                Rect::new(cx - w / 2. + 54.4, cy - label_h / 2., label_w, label_h),
                font,
                WHITE,
                false,
            );
        }
    }
}

/// Heading font at a fixed size. Only the Home headline follows the Heading
/// size slider; clock, weather temperature and section titles keep their size.
pub(crate) fn heading_fixed(assets: &Assets, size: f32, weight: u16) -> TextStyle {
    let pct = (assets.heading_pct.get() as f32 / 100.).max(0.5);
    TextStyle::new(size / pct)
        .weight(weight)
        .display(true)
        .leading(1.2)
}

/// Home section title (Favorites, stacks): heading font, 31 px, 20 px to the row.
pub(crate) const SECTION_TITLE_H: f32 = 38.;
pub(crate) const SECTION_TITLE_GAP: f32 = 20.;
pub(crate) fn section_title(out: &mut Scene, assets: &Assets, label: &str, y: f32) {
    out.text(
        label,
        Rect::new(84., y, 1200., SECTION_TITLE_H),
        heading_fixed(assets, 31., 700).tracking(-0.01),
        c(0xffffff, 0.94),
        false,
    );
}

/// Variant D: a quiet caption under the headline. Hidden (space kept) when the
/// clip has no name; on focus a capsule with ‹ › appears.
fn video_caption(out: &mut Scene, state: &State, assets: &Assets, y: f32, max_w: f32) {
    let rect_h = 48.;
    if state.preferences.background != "video" {
        return;
    }
    let video = crate::video_catalog::all().get(state.preferences.video_index);
    let name = video.map(|v| v.name.trim()).unwrap_or("");
    // No location pin: clip names cannot tell a real place from CG art.
    let place = false;
    let f = state.focus_level(&Target::VideoTitle);
    let style = TextStyle::new(25.).weight(550).tracking(-0.01).leading(1.3);
    let text_w = assets.width(name, style);
    let icon_w = if place { 30. } else { 0. };
    let arrows = 44. * f;
    let max_text = (max_w - icon_w - 2. * arrows).max(160.);
    // A little slack: the TV measures glyphs a hair wider than the layout.
    let shown_w = (text_w + 6.).min(max_text);
    let pad = 18. * f;
    let w = if name.is_empty() {
        96.
    } else {
        icon_w + shown_w + 2. * arrows + 2. * pad
    };
    let x = 84. - pad;
    let rect = Rect::new(x, y, w, rect_h);
    let accent = state.preferences.accent.use_focus;
    let dark = accent_ink(state);
    if f > 0. {
        let start = out.draw.len();
        if accent {
            accent_fill(out, rect, rect_h / 2., state);
            out.fade(start, f, 1.);
        } else {
            out.shape(rect, rect_h / 2., c(0x171c20, 0.72 * f));
            out.border(rect, rect_h / 2., 1., c(0xffffff, 0.12 * f));
        }
        let ink = if accent {
            [dark[0], dark[1], dark[2], f]
        } else {
            c(0xffffff, f)
        };
        // Mirror-image chevrons, each centered in its own arrow slot.
        let slot = |sx: f32| Rect::new(sx + (arrows - 22.) / 2., y + (rect_h - 22.) / 2., 22., 22.);
        out.icon(assets, "chevron-left", slot(x + pad), ink);
        out.icon(assets, "right", slot(x + w - pad - arrows), ink);
    }
    let mut cx = x + pad + arrows;
    let alpha = 0.72 + 0.28 * f;
    let ink = if accent {
        blend(c(0xffffff, alpha), [dark[0], dark[1], dark[2], 1.], f)
    } else {
        c(0xffffff, alpha)
    };
    if place {
        out.icon(
            assets,
            "location",
            Rect::new(cx, y + 13., 22., 22.),
            c(0xffffff, alpha),
        );
        cx += icon_w;
    }
    if !name.is_empty() {
        let line = line_box(Rect::new(cx, y, shown_w, rect_h), style);
        if text_w > max_text {
            // Marquee inside the caption width: pause, scroll, pause, restart.
            let travel = text_w - max_text + 24.;
            let speed = 40.;
            let cycle = 2. * 1.6 + travel / speed;
            let t = state.page_started.elapsed().as_secs_f32() % cycle;
            let off = if state.preferences.reduced_motion {
                0.
            } else {
                ((t - 1.6) * speed).clamp(0., travel)
            };
            out.draw
                .push(Draw::Clip(Some(Rect::new(line.x, y, line.w, rect_h))));
            out.text(
                name,
                Rect::new(line.x - off, line.y, text_w + 200., line.h),
                style,
                ink,
                false,
            );
            out.draw.push(Draw::Clip(None));
        } else {
            // Centered between the arrows; widened evenly on both sides so a
            // measuring mismatch never wraps into an ellipsis.
            let wide = Rect::new(line.x - 100., line.y, line.w + 200., line.h);
            out.text(name, wide, style, ink, true);
        }
    }
    out.control(Target::VideoTitle, rect, false);
}

/// Headline accent override, title/subtitle size percentages and wrap width.
struct PhraseLook {
    configured_accent: Option<(u32, u32)>,
    sizes: (u8, u8),
    wrap: f32,
}

fn draw_rude_phrase(
    out: &mut Scene,
    assets: &Assets,
    phrase: &crate::weather_phrases::Phrase,
    scroll: f32,
    alpha: f32,
    look: PhraseLook,
) -> (f32, f32) {
    let PhraseLook {
        configured_accent,
        sizes,
        wrap,
    } = look;
    // The subtitle may run wider than the title, but never under the widgets.
    let subtitle_wrap = if wrap < PHRASE_W { wrap } else { 1250. };
    let title_k = sizes.0 as f32 / 100.;
    let subtitle_k = sizes.1 as f32 / 100.;
    // The former text(74) silently mapped to the 52px Title token. Use an
    // explicit display size, then shrink ONLY when needed for two lines.
    let mut size = 78. * title_k;
    let style = loop {
        let candidate = TextStyle::new(size)
            .weight(800)
            .tracking(-0.04)
            .display(true)
            .leading(1.03);
        if assets.lines(&phrase.title, candidate, wrap) <= 2 || size <= 42. * title_k {
            break candidate;
        }
        size -= 2.;
    };
    let line_step =
        style.size * assets.active_heading.get().scale() * assets.heading_pct.get() as f32 / 100.
            * style.line_height;
    let subtitle_style = TextStyle::new(33. * subtitle_k)
        .weight(600)
        .tracking(-0.012)
        .leading(1.13);
    let subtitle_line_height =
        subtitle_style.size * assets.active_text.get().scale() * assets.text_pct.get() as f32
            / 100.
            * subtitle_style.line_height;
    let accent = match phrase.category {
        crate::weather_phrases::Category::Frost
        | crate::weather_phrases::Category::Cold
        | crate::weather_phrases::Category::Chilly => c(0x7db9ff, alpha),
        crate::weather_phrases::Category::Hot => c(0xffb651, alpha),
        crate::weather_phrases::Category::Snow
        | crate::weather_phrases::Category::Rain
        | crate::weather_phrases::Category::Thunder => c(0x9bdbfa, alpha),
        crate::weather_phrases::Category::Cloudy | crate::weather_phrases::Category::Fog => {
            c(0xc4c7d3, alpha)
        }
        crate::weather_phrases::Category::ClearNice => c(0xff859d, alpha),
        _ => c(0xffa66c, alpha),
    };
    // Gradient accents run across each highlighted word, letter by letter.
    let (accent, accent_end) =
        configured_accent.map_or((accent, accent), |(a, b)| (c(a, alpha), c(b, alpha)));
    // Every swear word takes the accent; a clean phrase accents one weather word.
    let swearing = phrase.title.split_whitespace().any(is_swear);
    let accent_word = if swearing {
        ""
    } else {
        phrase
            .title
            .split_whitespace()
            .find(|token| {
                [
                    "oven", "fridge", "grey", "snow", "sun", "rain", "wind", "cold", "fog", "hot",
                    "night", "thunder",
                ]
                .iter()
                .any(|key| token.to_ascii_lowercase().contains(key))
            })
            .unwrap_or("")
    };
    let mut x = 84.;
    let mut y = PHRASE_TOP - scroll;
    let mut line = 0;
    let mut widest = 0.0f32;
    for (i, word) in phrase.title.split_whitespace().enumerate() {
        let next = format!(
            "{word}{}",
            if i + 1 == phrase.title.split_whitespace().count() {
                ""
            } else {
                " "
            }
        );
        let width = assets.width(&next, style);
        if x > 84. && x + width > 84. + wrap && line == 0 {
            line += 1;
            x = 84.;
            y += line_step;
        }
        if (swearing && is_swear(word)) || (!accent_word.is_empty() && word == accent_word) {
            gradient_word(
                out,
                assets,
                &next,
                x,
                y,
                line_step + 3.,
                style,
                accent,
                accent_end,
            );
        } else {
            out.text(
                next,
                Rect::new(x, y, width + 4., line_step + 3.),
                style,
                c(0xffffff, alpha),
                false,
            );
        }
        x += width;
        widest = widest.max(x - 84.);
    }
    // Subheading follows the LAST rendered headline line, not a fixed Y that
    // collides with multiline headlines and hangs above Favorites on one line.
    // Treat the subtitle as part of the headline, not a separate date row.
    // Subtitle hugs the headline; the video caption keeps its own distance.
    let subtitle_y = y + line_step - 4.;
    let subtitle_lines = assets
        .lines(&phrase.subtitle, subtitle_style, subtitle_wrap)
        .clamp(1, 2) as f32;
    // Swear words in the subtitle take the accent, the rest stays soft white.
    let subtitle_words: Vec<&str> = phrase.subtitle.split_whitespace().collect();
    let (mut sx, mut sy) = (84.0f32, subtitle_y);
    for (i, word) in subtitle_words.iter().enumerate() {
        let next = if i + 1 == subtitle_words.len() {
            word.to_string()
        } else {
            format!("{word} ")
        };
        let width = assets.width(&next, subtitle_style);
        if sx > 84. && sx + width > 84. + subtitle_wrap {
            sx = 84.;
            sy += subtitle_line_height;
        }
        if is_swear(word) {
            gradient_word(
                out,
                assets,
                &next,
                sx,
                sy,
                subtitle_line_height + 2.,
                subtitle_style,
                accent,
                accent_end,
            );
        } else {
            out.text(
                next,
                Rect::new(sx, sy, width + 4., subtitle_line_height + 2.),
                subtitle_style,
                c(0xf5f5f8, alpha * 0.85),
                false,
            );
        }
        sx += width;
    }
    widest = widest.max(assets.width(&phrase.subtitle, subtitle_style).min(wrap));
    (subtitle_y + subtitle_line_height * subtitle_lines, widest)
}
/// One word with its colour interpolated from `from` to `to` across the letters.
#[allow(clippy::too_many_arguments)]
fn gradient_word(
    out: &mut Scene,
    assets: &Assets,
    word: &str,
    x: f32,
    y: f32,
    h: f32,
    style: TextStyle,
    from: Color,
    to: Color,
) {
    if from == to {
        let w = assets.width(word, style);
        out.text(word, Rect::new(x, y, w + 4., h), style, from, false);
        return;
    }
    let letters = word.trim_end().chars().count().max(2) as f32 - 1.;
    let mut cx = x;
    for (i, ch) in word.chars().enumerate() {
        let glyph = ch.to_string();
        let w = assets.width(&glyph, style);
        if !ch.is_whitespace() {
            // Half the accent range per word, so a short word is not squeezed.
            let t = 0.55 * (i as f32 / letters).min(1.);
            out.text(
                &glyph,
                Rect::new(cx, y, w + 30., h),
                style,
                blend(from, to, t),
                false,
            );
        }
        cx += w;
    }
}
fn is_swear(word: &str) -> bool {
    let w = word
        .trim_matches(|ch: char| !ch.is_alphanumeric())
        .to_lowercase();
    const ROOTS: [&str; 22] = [
        "fuck",
        "shit",
        "damn",
        "bitch",
        "crap",
        "piss",
        "bastard",
        "arse",
        "bollock",
        "wank",
        "dick",
        "хуй",
        "хуё",
        "хуе",
        "бля",
        "пизд",
        "ебат",
        "ебан",
        "ёб",
        "жоп",
        "говн",
        "дерьм",
    ];
    ROOTS.iter().any(|root| w.contains(root))
        || matches!(
            w.as_str(),
            "hell" | "ass" | "balls" | "bloody" | "wtf" | "сука" | "сучий" | "хер"
        )
        || w.starts_with("balls-")
}
/// Home content sits this much lower than the headline grid, so at rest only
/// the top of the first stack row peeks above the screen edge.
pub(crate) const HOME_DROP: f32 = 40.;
/// Plate tint, and its opacity over frost or (no background to blur) alone.
const GLASS: u32 = 0x181a1e;
const GLASS_FROSTED: f32 = 0.34;
const GLASS_PLAIN: f32 = 0.66;
/// Headline block: top and wrap width (stops 44 px before the weather card).
const PHRASE_TOP: f32 = 268.;
const PHRASE_W: f32 = 1140.;

fn home(out: &mut Scene, state: &State, assets: &Assets, now: OffsetDateTime) {
    let language = state.preferences.language;
    let s = state.scroll + state.bounce_at(crate::state::BounceAxis::Page) - HOME_DROP;
    // Weather gets a distinct TV-sized widget instead of reducing the greeting.
    let shift = 0.0f32;
    let greeting_start = out.draw.len();
    let mut greeting_bottom = 0.0f32;
    let mut greeting_width = 900.0f32;
    if state.preferences.greeting_style == crate::weather_phrases::GreetingStyle::Rude {
        if let Some(phrase) = &state.rude_weather.current {
            let fraction = state.rude_weather.changed_at.map_or(1., |started| {
                motion::progress(started.elapsed().as_secs_f32(), 0.42)
            });
            if fraction < 1.
                && !state.preferences.reduced_motion
                && let Some(previous) = &state.rude_weather.previous
            {
                let _ = draw_rude_phrase(
                    out,
                    assets,
                    previous,
                    s,
                    1. - fraction,
                    PhraseLook {
                        configured_accent: state
                            .preferences
                            .accent
                            .use_headline
                            .then(|| state.accent_colors()),
                        sizes: (
                            state.preferences.phrase_title_pct,
                            state.preferences.phrase_subtitle_pct,
                        ),
                        wrap: widgets_view::phrase_width(state),
                    },
                );
            }
            (greeting_bottom, greeting_width) = draw_rude_phrase(
                out,
                assets,
                phrase,
                s,
                if state.preferences.reduced_motion {
                    1.
                } else {
                    fraction
                },
                PhraseLook {
                    configured_accent: state
                        .preferences
                        .accent
                        .use_headline
                        .then(|| state.accent_colors()),
                    sizes: (
                        state.preferences.phrase_title_pct,
                        state.preferences.phrase_subtitle_pct,
                    ),
                    wrap: widgets_view::phrase_width(state),
                },
            );
        } else {
            let greeting = language.greeting(now.hour(), &state.preferences.greeting_name);
            out.text(
                &greeting,
                Rect::new(
                    84.,
                    PHRASE_TOP - shift - s,
                    widgets_view::phrase_width(state),
                    109.,
                ),
                text(99., 550, -0.055).display(true).leading(1.05),
                WHITE,
                false,
            );
        }
    } else {
        let greeting = language.greeting(now.hour(), &state.preferences.greeting_name);
        out.text(
            &greeting,
            Rect::new(
                84.,
                PHRASE_TOP - shift - s,
                widgets_view::phrase_width(state),
                95.,
            ),
            text(90.4, 500, -0.065).display(true).leading(1.05),
            WHITE,
            false,
        );
    }
    if greeting_bottom <= 0. {
        greeting_bottom = PHRASE_TOP + 110. - s;
    }
    out.enter(greeting_start, state, 0.13, 32.);
    // Variant D video caption sits under the headline; its slot is always kept.
    let caption_y = greeting_bottom + 34.;
    let caption_start = out.draw.len();
    video_caption(out, state, assets, caption_y, greeting_width.max(420.));
    out.enter(caption_start, state, 0.18, 22.);
    let press = state.ambient_press();
    if press > 0. {
        press_scale(&mut out.draw[caption_start..], 1. - 0.06 * press);
    }
    // The caption goes first, then the headline line by line from the bottom.
    ambient_move(&mut out.draw[caption_start..], state, -1100., 0., |_| 1.);
    let lowest = out.draw[greeting_start..caption_start]
        .iter_mut()
        .filter_map(rect_mut)
        .fold(0f32, |m, r| m.max(r.y));
    ambient_move(
        &mut out.draw[greeting_start..caption_start],
        state,
        -1100.,
        0.,
        |r| 2. + ((lowest - r.y) / 70.).floor().min(3.),
    );
    // Favorites title rests at 548 and only moves down for a taller headline.
    let home_content_offset = (caption_y + 48. + 36. + s - FAVORITES_TOP)
        .max(widgets_view::bottom() + 40. - FAVORITES_TOP)
        .max(0.);
    if state.widget_edit.is_none() {
        let start = out.draw.len();
        widgets_view::home(out, state, assets);
        out.enter(start, state, 0.21, 29.);
        // One widget after another, the rightmost first.
        let blocks: Vec<Rect> = widgets_view::zone(state)
            .blocks
            .iter()
            .map(|b| b.2)
            .collect();
        let count = blocks.len() as f32;
        ambient_move(&mut out.draw[start..], state, 1100., 0., |r| {
            let (cx, cy) = (r.x + r.w / 2., r.y + r.h / 2.);
            blocks
                .iter()
                .position(|b| b.contains(cx, cy))
                .map_or(1., |i| 1. + (count - 1. - i as f32))
        });
    }
    let fav_label_start = out.draw.len();
    section_title(
        out,
        assets,
        language.t("favoriteApps"),
        FAVORITES_TOP + home_content_offset - s,
    );
    out.enter(fav_label_start, state, 0.3, 22.);
    if state.moving.is_some() {
        out.text(
            language.t("moveApps"),
            Rect::new(
                1686.,
                FAVORITES_TOP + 7. + home_content_offset - s,
                150.,
                24.,
            ),
            text(14.4, 400, 0.),
            c(0xdedede, 1.),
            true,
        );
    }
    let apps = state.apps();
    let names = state.preferences.show_home_names;
    let mut grid_height = FACE_HEIGHT;
    if state.loading && state.inventory.apps.is_empty() {
        for index in 0..6 {
            out.shape(
                Rect::new(
                    84. + index as f32 * GRID_PITCH,
                    FAVORITES_ROW + home_content_offset - s,
                    270.,
                    FACE_HEIGHT,
                ),
                19.2,
                c(0xffffff, 0.06),
            );
        }
    } else if apps.is_empty() {
        let error = state.error.is_some() && state.inventory.apps.is_empty();
        empty(
            out,
            state,
            assets,
            EmptyState {
                rect: Rect::new(
                    84.,
                    FAVORITES_ROW - 11. + home_content_offset - s,
                    1752.,
                    258.7,
                ),
                icon: "heart",
                title: if error {
                    language.t("aLittleHelp")
                } else {
                    language.t("makeFavorites")
                },
                hint: if error {
                    language.t("appsRetryHint")
                } else {
                    language.t("pinHint")
                },
                action: (!error).then_some(Target::Browse),
            },
        );
        grid_height = 247.5;
    }
    if !apps.is_empty() {
        grid_height = FACE_HEIGHT
            + if names {
                18.4 + apps
                    .iter()
                    .map(|app| caption_height(state, assets, app, HOME_TILE_WIDTH))
                    .fold(49.28, f32::max)
            } else {
                0.
            };
    }
    let mut visible_rank = 0;
    for (index, app) in apps.iter().enumerate() {
        let rect = Rect::new(
            84. + index as f32 * HOME_TILE_PITCH
                - state.home_scroll
                - state.bounce_at(crate::state::BounceAxis::Favorites),
            FAVORITES_ROW + home_content_offset - s,
            HOME_TILE_WIDTH,
            FACE_HEIGHT,
        );
        let visible = rect.x + rect.w + 40. >= 64.
            && rect.x - 40. <= 1856.
            && rect.y + rect.h >= MAIN_TOP
            && rect.y <= state.main_bottom();
        tile(
            out,
            state,
            assets,
            app,
            rect,
            TileOptions {
                names,
                pinned: false,
            },
            visible_rank,
        );
        if visible {
            visible_rank += 1;
        }
    }
    // Favorites: the title, then the tiles left to right.
    ambient_move(&mut out.draw[fav_label_start..], state, 0., 900., |r| {
        4. + 3. * (r.x / 1920.).clamp(0., 1.)
    });
    let stacks_y = FAVORITES_ROW + grid_height + 47. + home_content_offset - s;
    stacks_view::home_stacks(out, state, assets, stacks_y);
    // Edit mode dims the page, so the widgets draw over everything else.
    if state.widget_edit.is_some() {
        widgets_view::home(out, state, assets);
    }
}

/// Favorites section title top and its row (title 38 + 20 gap).
pub(crate) const FAVORITES_TOP: f32 = 548.;
pub(crate) const FAVORITES_ROW: f32 = FAVORITES_TOP + SECTION_TITLE_H + SECTION_TITLE_GAP;

/// A tab of a second row of tabs (Apps filters, Library): only the focused tab
/// is filled with the accent; the chosen one keeps an accent outline, so two
/// filled pills never compete on one screen. Returns the label ink.
pub(crate) fn sub_tab(
    out: &mut Scene,
    state: &State,
    rect: Rect,
    focused: bool,
    selected: bool,
) -> Color {
    let r = rect.h / 2.;
    let (first, second) = state.accent_colors();
    let gradient = state.preferences.accent.kind == "gradient";
    if focused {
        out.draw.push(if gradient {
            Draw::DiagonalGradient(rect, r, c(first, 1.), c(second, 1.))
        } else {
            Draw::RoundedGradient(rect, r, c(first, 1.), c(first, 1.))
        });
        return accent_ink(state);
    }
    if selected {
        accent_outline(out, state, rect, r);
        return c(0xffffff, 0.95);
    }
    c(0xeeeeee, 0.73)
}

/// The chosen item that does not hold focus: a thin accent outline, never a
/// fill. Only the focused item on screen is filled.
pub(crate) fn accent_outline(out: &mut Scene, state: &State, rect: Rect, radius: f32) {
    const W: f32 = 2.5;
    let (first, second) = state.accent_colors();
    let second = if state.preferences.accent.kind == "gradient" {
        second
    } else {
        first
    };
    out.draw.push(Draw::DiagonalGradientBorder(
        rect.inset(W / 2.),
        (radius - W / 2.).max(0.),
        W,
        c(first, 1.),
        c(second, 1.),
    ));
}
/// Weather card: columns 5–6, from under the header to 44 px above Favorites.
// Top edge sits on the cap line of the big headline.
const WEATHER_TOP: f32 = PHRASE_TOP;
fn condition_icon(icon: &str) -> &str {
    match icon {
        "sun" => "weather-sun",
        "moon" => "weather-moon",
        "partly-cloudy" => "weather-partly-cloudy",
        "fog" => "weather-fog",
        "wind" => "weather-cloud",
        "drizzle" => "weather-drizzle",
        "rain" => "weather-rain",
        "storm" => "weather-storm",
        "snow" => "weather-snow",
        _ => "weather-cloud",
    }
}
fn clip_weather_line(value: &str, max_width: f32, style: TextStyle, assets: &Assets) -> String {
    if assets.width(value, style) <= max_width {
        return value.to_owned();
    }
    let mut result = String::new();
    for ch in value.chars() {
        let candidate = format!("{result}{ch}…");
        if assets.width(&candidate, style) > max_width {
            break;
        }
        result.push(ch);
    }
    result.push('…');
    result
}

// Follow the current Heading/Text font preferences without forcing the global
// Settings-only token sizes onto the denser NOPE six-hour weather grid.
fn weather_type(assets: &Assets, size: f32, weight: u16, display: bool) -> TextStyle {
    // Undo the global size sliders: the card has a fixed box.
    let pct = if display {
        assets.heading_pct.get()
    } else {
        assets.text_pct.get()
    } as f32
        / 100.;
    TextStyle::new(size / pct.max(0.5))
        .weight(weight)
        .tracking(-0.015)
        .leading(1.16)
        .display(display)
        .tabular()
}

fn library(out: &mut Scene, state: &State, assets: &Assets) {
    let language = state.preferences.language;
    let s = state.scroll;
    // Category tabs: a smaller twin of the top navigation, as tall as the
    // search field and on its line. "All" comes first and is the default.
    let style = nav_label_style();
    let mut tabs: Vec<(Target, String)> =
        vec![(Target::Filter(0), language.t("allApps").to_owned())];
    for (index, label) in CATEGORIES.into_iter().enumerate() {
        tabs.push((
            Target::Filter(index + 1),
            language.category(label).to_owned(),
        ));
    }
    if state.hidden_count() > 0 {
        tabs.push((Target::Filter(5), language.t("hidden").to_owned()));
    }
    let bar_h = 56.;
    let pad = 4.;
    let widths: Vec<f32> = tabs
        .iter()
        .map(|(_, label)| assets.width(label, style) + 2. * 24.)
        .collect();
    let bar = Rect::new(
        84.,
        184. - s,
        widths.iter().sum::<f32>() + pad * 2. + NAV_TAB_GAP * (tabs.len() - 1) as f32,
        bar_h,
    );
    out.shadow(bar, bar_h / 2., 18., 4., 0.06);
    out.glass(bar, bar_h / 2.);
    let mut x = bar.x + pad;
    for ((target, label), w) in tabs.iter().zip(widths) {
        let rect = Rect::new(x, bar.y + pad, w, bar_h - 2. * pad);
        let selected = matches!(target, Target::Filter(i) if *i == state.filter);
        let focused = state.focus == *target;
        let ink = sub_tab(out, state, rect, focused, selected);
        out.text(label.as_str(), line_box(rect, style), style, ink, true);
        out.control(target.clone(), rect, true);
        x += w + NAV_TAB_GAP;
    }
    let search = Rect::new(1532., 184. - s, 304., 56.);
    let focused = matches!(state.focus, Target::Search | Target::ClearSearch);
    if focused {
        out.shape(search, 48., c(0x303438, 1.));
    } else {
        out.glass(search, 48.);
    }
    out.border(
        search,
        48.,
        1.,
        c(0xffffff, if focused { 0.70 } else { 0. }),
    );
    if focused {
        out.border(search.inset(-3.), 51., 3., c(0xffffff, 0.063));
    }
    out.icon(
        assets,
        "search",
        Rect::new(search.x + 21., search.y + (search.h - 20.) / 2., 20., 20.),
        c(0xeeeeee, 0.73),
    );
    out.text(
        if state.query.is_empty() {
            language.t("findApp")
        } else {
            &state.query
        },
        line_box(
            Rect::new(
                search.x + 52.,
                search.y,
                if state.query.is_empty() { 234. } else { 202. },
                search.h,
            ),
            nav_label_style(),
        ),
        nav_label_style(),
        c(0xeeeeee, if state.query.is_empty() { 0.73 } else { 1. }),
        false,
    );
    out.control(Target::Search, search, true);
    if !state.query.is_empty() {
        let clear = Rect::new(search.x + search.w - 45.8, search.y + 14.8, 26.4, 26.4);
        out.shape(clear, 13.2, c(0xffffff, 0.055));
        out.icon(assets, "close", clear.inset(5.2), WHITE);
        if state.focus == Target::ClearSearch {
            out.border(clear.inset(-2.), 15.2, 2., WHITE);
        }
        out.control(Target::ClearSearch, clear, true);
    }
    let favorites = state.preferences.favorites(&state.inventory.apps);
    let apps = state.apps();
    if apps.is_empty() {
        let title = if state.loading {
            language.t("findingApps")
        } else if state.error.is_some() {
            language.t("aLittleHelp")
        } else if !state.query.is_empty() {
            language.t("noMatches")
        } else if state.filter == 5 {
            language.t("noHidden")
        } else {
            language.t("roomForNew")
        };
        let hint = state.error.as_ref().map_or_else(
            || {
                if !state.query.is_empty() {
                    language.format("noMatchesHint", &[("query", &state.query)])
                } else if state.filter == 5 {
                    language.t("hiddenHint").into()
                } else {
                    language.t("noCategoryApps").into()
                }
            },
            |_| language.t("appsRetryHint").into(),
        );
        empty(
            out,
            state,
            assets,
            EmptyState {
                rect: Rect::new(84., 259.2 - s, 1752., 304.),
                icon: "grid",
                title,
                hint: &hint,
                action: None,
            },
        );
    }
    let mut y = 290.;
    let names = state.preferences.show_app_names;
    // Entrance wave: each visible card gets its own beat along the diagonal
    // (row + column), so the grid fills card by card instead of the first
    // eight cards and then everything else in one block.
    let mut visible_row = 0;
    for row in apps.chunks(APPS_COLUMNS) {
        let row_height = FACE_HEIGHT
            + if names {
                18.4 + row
                    .iter()
                    .map(|app| caption_height(state, assets, app, 270.))
                    .fold(49.28, f32::max)
            } else {
                0.
            };
        let mut row_visible = false;
        for (column, app) in row.iter().enumerate() {
            let rect = Rect::new(84. + column as f32 * GRID_PITCH, y - s, 270., FACE_HEIGHT);
            let visible = rect.y + rect.h >= MAIN_TOP && rect.y <= state.main_bottom();
            row_visible |= visible;
            tile(
                out,
                state,
                assets,
                app,
                rect,
                TileOptions {
                    names,
                    pinned: favorites.contains(&app.id),
                },
                visible_row + column,
            );
        }
        if row_visible {
            visible_row += 1;
        }
        y += row_height + 38.4;
    }
}

struct Row<'a> {
    target: Target,
    rect: Rect,
    title: &'a str,
    icon: Option<&'a str>,
    value: Option<&'a str>,
    description: Option<&'a str>,
    selected: bool,
    chevron: bool,
}
fn row(out: &mut Scene, state: &State, assets: &Assets, row: Row<'_>) {
    let start = out.draw.len();
    let rank = out.settings_visible_rows;
    if row.rect.y + row.rect.h >= MAIN_TOP && row.rect.y <= state.main_bottom() {
        out.settings_visible_rows += 1;
    }
    let _ = row.icon; // A single focus style replaces mixed decorative row icons.
    let slot = ((row.rect.y + state.scroll - 164.8) / 84.).max(0.).round();
    let rect = Rect::new(
        tvos_settings_view::RIGHT + tvos_settings_view::S32,
        tvos_settings_view::TOP
            + tvos_settings_view::S32
            + slot
                * if matches!(state.subpage, Some(Subpage::Greeting)) {
                    116.
                } else {
                    92.
                }
            - state.scroll,
        tvos_settings_view::WIDTH - 2. * tvos_settings_view::S32,
        if row.description.is_some() {
            104.
        } else {
            tvos_settings_view::ROW_HEIGHT
        },
    );
    tvos_settings_view::cell(
        out,
        state,
        assets,
        row.target,
        rect,
        tvos_settings_view::CellContent {
            name: row.title,
            value: row.value,
            desc: row.description,
            chevron: row.chevron,
            checked: row.selected,
            partial: false,
        },
    );
    let _ = (start, rank);
}

fn weather_settings(out: &mut Scene, state: &State, assets: &Assets) {
    let language = state.preferences.language;
    let s = state.scroll;
    row(
        out,
        state,
        assets,
        Row {
            target: Target::WeatherToggle,
            rect: Rect::new(640., 164.8 - s, 1120., 72.),
            title: language.t("showWidget"),
            icon: Some("eye"),
            value: Some(if state.weather_widget_shown() {
                language.t("stateOn")
            } else {
                language.t("stateOff")
            }),
            description: None,
            selected: false,
            chevron: false,
        },
    );
    let city = state
        .preferences
        .weather_location
        .as_ref()
        .map(|place| place.name.as_str());
    let style = text(18.4, 450, 0.).leading(1.4);
    let city_lines = city.map_or(1, |city| assets.lines(city, style, 450.));
    let height = (34. + city_lines as f32 * 25.76).max(72.);
    let rect = Rect::new(640., 264.8 - s, 1120., height);
    row(
        out,
        state,
        assets,
        Row {
            target: Target::WeatherLocation,
            rect,
            title: language.t("location"),
            icon: Some("location"),
            value: None,
            description: None,
            selected: false,
            chevron: true,
        },
    );
    let _ = (city, style, city_lines);
    let y = rect.y + height + 12.;
    row(
        out,
        state,
        assets,
        Row {
            target: Target::WeatherTemperature,
            rect: Rect::new(640., y, 1120., 72.),
            title: language.t("temperature"),
            icon: Some("thermometer"),
            value: Some(state.preferences.temperature_unit.symbol()),
            description: None,
            selected: false,
            chevron: true,
        },
    );
}

fn location_settings(out: &mut Scene, state: &State, assets: &Assets) {
    // The Weather location screen uses exactly the same right-hand glass inset
    // and 80px vertical rhythm as every other Settings page. The panel itself
    // is created by tvos_settings_view::page_head.
    let x = tvos_settings_view::RIGHT + tvos_settings_view::S24;
    let top = tvos_settings_view::TOP + tvos_settings_view::S24;
    let width = tvos_settings_view::WIDTH - 2. * tvos_settings_view::S24;
    let height = tvos_settings_view::ROW_HEIGHT;
    let step = tvos_settings_view::STEP;
    let radius = tvos_settings_view::CELL_RADIUS;
    let language = state.preferences.language;
    let mut y = top;
    if let Some(place) = &state.preferences.weather_location {
        let rect = Rect::new(x, y, width, height);
        out.shape(rect, radius, c(0xffffff, 0.07));
        out.text(
            tvos_settings_view::ellipsis_for_smart(
                assets,
                &place.name,
                text(31., 600, 0.),
                width - 132.,
            ),
            Rect::new(x + 24., y + 5., width - 132., 40.),
            text(31., 600, 0.),
            WHITE,
            false,
        );
        out.text(
            tvos_settings_view::ellipsis_for_smart(
                assets,
                &place.detail(),
                text(25., 500, 0.),
                width - 132.,
            ),
            Rect::new(x + 24., y + 44., width - 132., 32.),
            text(25., 500, 0.),
            c(0xffffff, 0.62),
            false,
        );
        let clear = Rect::new(x + width - 64., y + 12., 56., 56.);
        let focused = state.focus_level(&Target::WeatherClear);
        out.shape(
            clear,
            28.,
            c(0xffffff, if focused > 0. { focused } else { 0.07 }),
        );
        out.icon(
            assets,
            "close",
            clear.inset(16.),
            if focused > 0.5 {
                c(0x15171b, 1.)
            } else {
                WHITE
            },
        );
        out.control(Target::WeatherClear, clear, false);
        y += step;
    }
    let search = Rect::new(x, y, width, height);
    let query_focus = state.focus_level(&Target::WeatherQuery);
    out.shape(
        search,
        radius,
        c(0xffffff, if query_focus > 0. { query_focus } else { 0.07 }),
    );
    let query_text = if state.weather.query.is_empty() {
        language.t("cityPostcode")
    } else {
        state.weather.query.as_str()
    };
    out.text(
        tvos_settings_view::ellipsis_for_smart(
            assets,
            query_text,
            text(31., 500, 0.),
            width - 150.,
        ),
        Rect::new(x + 24., y + 19., width - 150., 44.),
        text(31., 500, 0.),
        if query_focus > 0.5 {
            c(0x15171b, 1.)
        } else {
            c(0xffffff, 0.78)
        },
        false,
    );
    out.control(
        Target::WeatherQuery,
        Rect::new(x, y, width - 80., height),
        false,
    );
    let search_button = Rect::new(x + width - 72., y + 12., 56., 56.);
    let search_focus = state.focus_level(&Target::WeatherSearch);
    out.shape(
        search_button,
        radius,
        c(
            0xffffff,
            if search_focus > 0. {
                search_focus
            } else {
                0.10
            },
        ),
    );
    out.icon(
        assets,
        if state.weather.searching {
            "refresh"
        } else {
            "search"
        },
        search_button.inset(16.),
        if search_focus > 0.5 {
            c(0x15171b, 1.)
        } else {
            WHITE
        },
    );
    out.control(Target::WeatherSearch, search_button, false);
    y += step;
    if state.weather.searching || state.weather.message.is_some() {
        let message = if state.weather.searching {
            language.t("searching")
        } else {
            language.t(state.weather.message.as_deref().unwrap_or(""))
        };
        out.text(
            message,
            Rect::new(x + 24., y + 16., width - 48., 44.),
            text(25., 500, 0.),
            c(0xffffff, 0.68),
            false,
        );
        y += step;
    }
    // Only result rows scroll. Their clip ensures all text and hit regions
    // remain inside the right panel, including with very long city names.
    let end = tvos_settings_view::TOP + tvos_settings_view::MAX_HEIGHT - tvos_settings_view::S24;
    out.draw.push(Draw::Clip(Some(Rect::new(
        x,
        y.min(end),
        width,
        (end - y).max(0.),
    ))));
    for (index, place) in state.weather.results.iter().enumerate() {
        let row_y = y + index as f32 * step - state.scroll;
        if row_y + height < y || row_y >= end {
            continue;
        }
        let rect = Rect::new(x, row_y, width, height);
        tvos_settings_view::cell(
            out,
            state,
            assets,
            Target::WeatherResult(index),
            rect,
            tvos_settings_view::CellContent {
                name: &place.name,
                value: None,
                desc: Some(&place.detail()),
                chevron: false,
                checked: false,
                partial: false,
            },
        );
    }
    out.draw.push(Draw::Clip(Some(Rect::new(
        0.,
        0.,
        1920.,
        state.main_bottom(),
    ))));
}

fn settings(out: &mut Scene, state: &State, assets: &Assets) {
    // A settings level owns the entire canvas. Navigation pushes to new pages,
    // rather than keeping a permanent left-hand sidebar.
    let start = out.draw.len();
    out.settings_sidebar_range = start..start;
    out.settings_panel_range.start = start;
    if state.subpage.is_none()
        && (state.settings_level != crate::state::SettingsLevel::Section
            || state.section == 1
            || state.section == crate::state::APPEARANCE_SECTION
            || state.section == crate::state::STACKS_SECTION)
    {
        tvos_settings_view::render(out, state, assets);
        return;
    }
    let (title, icon, detail) = match state.section {
        0 => ("General", "settings", "Personalize Luma"),
        WEATHER_SECTION => ("Weather", "clock", "Local conditions on Home"),
        crate::state::INPUTS_SECTION => ("Inputs", "tv", "Manage connected devices"),
        crate::state::SMARTHOME_SECTION => ("Smart Home", "tv", "Home Assistant devices"),
        _ if state.root.is_some() => ("System", "settings", "Launcher and root features"),
        _ => ("System", "settings", "Device and launcher controls"),
    };
    tvos_settings_view::page_head(out, state, assets, title, icon, detail);
    legacy_settings_body(out, state, assets);
}

fn legacy_settings_body(out: &mut Scene, state: &State, assets: &Assets) {
    let language = state.preferences.language;
    let s = state.scroll;
    if let Some(subpage) = &state.subpage {
        if matches!(
            subpage,
            Subpage::SmartConnect
                | Subpage::SmartSelect(_)
                | Subpage::WeatherLocation
                | Subpage::Greeting
                | Subpage::AppNames
                | Subpage::Clock
        ) {
            match subpage {
                Subpage::SmartConnect | Subpage::SmartSelect(_) => {
                    smart_home_settings_view::render(out, state, assets)
                }
                Subpage::WeatherLocation => location_settings(out, state, assets),
                Subpage::Greeting => greeting_settings(out, state, assets),
                Subpage::AppNames => app_name_settings(out, state, assets),
                Subpage::Clock => clock_settings(out, state, assets),
                _ => unreachable!(),
            }
            return;
        }
        let options: Vec<(&str, Option<&str>, bool)> = match subpage {
            Subpage::ClockFormat => vec![
                (language.t("hour12"), None, !state.preferences.hour_24),
                (language.t("hour24"), None, state.preferences.hour_24),
            ],
            Subpage::TimeFormat => vec![
                ("HH:MM", None, !state.preferences.show_seconds),
                ("HH:MM:SS", None, state.preferences.show_seconds),
            ],
            Subpage::Input(id) => {
                let current = state
                    .preferences
                    .input_modes
                    .get(id)
                    .copied()
                    .unwrap_or_default();
                vec![
                    (
                        language.t("stateOn"),
                        Some(language.t("inputAlways")),
                        current == InputMode::On,
                    ),
                    (
                        language.t("stateOff"),
                        Some(language.t("inputNever")),
                        current == InputMode::Off,
                    ),
                    (
                        language.t("stateAuto"),
                        Some(language.t("inputWhenConnected")),
                        current == InputMode::Auto,
                    ),
                ]
            }
            Subpage::GreetingStyle => vec![
                (
                    "Classic",
                    None,
                    state.preferences.greeting_style
                        == crate::weather_phrases::GreetingStyle::Classic,
                ),
                (
                    "Rude",
                    None,
                    state.preferences.greeting_style == crate::weather_phrases::GreetingStyle::Rude,
                ),
            ],
            Subpage::Temperature => vec![
                (
                    language.t("celsius"),
                    None,
                    state.preferences.temperature_unit == Unit::Celsius,
                ),
                (
                    language.t("fahrenheit"),
                    None,
                    state.preferences.temperature_unit == Unit::Fahrenheit,
                ),
            ],
            Subpage::SmartConnect
            | Subpage::SmartSelect(_)
            | Subpage::WeatherLocation
            | Subpage::Greeting
            | Subpage::AppNames
            | Subpage::Clock => {
                unreachable!()
            }
            Subpage::Language => Language::ALL
                .into_iter()
                .map(|option| (option.name(), None, option == language))
                .collect(),
        };
        for (index, (title, description, selected)) in options.into_iter().enumerate() {
            let h = if description.is_some() { 90.7 } else { 72. };
            row(
                out,
                state,
                assets,
                Row {
                    target: Target::Choice(index),
                    rect: Rect::new(640., 164.8 + index as f32 * (h + 12.) - s, 1120., h),
                    title,
                    icon: None,
                    value: None,
                    description,
                    selected,
                    chevron: false,
                },
            );
        }
        if let Subpage::Input(id) = subpage {
            input_name_rows(out, state, assets, id, 164.8 + 3. * 102.7 + 16. - s);
        }
        return;
    }
    match state.section {
        0 => {
            for (position, (index, key, icon, value, chevron)) in [
                (0, "appNames", "type", None, true),
                (1, "clock", "clock", None, true),
                (
                    2,
                    "reducedMotion",
                    "motion",
                    Some(if state.preferences.reduced_motion {
                        language.t("stateOn")
                    } else {
                        language.t("stateOff")
                    }),
                    false,
                ),
                (4, "language", "language", Some(language.name()), true),
                (
                    6,
                    "greeting",
                    "type",
                    Some(if state.preferences.greeting_name.trim().is_empty() {
                        language.t("stateOff")
                    } else {
                        state.preferences.greeting_name.as_str()
                    }),
                    true,
                ),
            ]
            .into_iter()
            .enumerate()
            {
                row(
                    out,
                    state,
                    assets,
                    Row {
                        target: Target::Setting(index),
                        rect: Rect::new(640., 164.8 + position as f32 * 84. - s, 1120., 72.),
                        title: language.t(key),
                        icon: Some(icon),
                        value,
                        description: None,
                        selected: false,
                        chevron,
                    },
                );
            }
        }
        1 => background_settings(out, state, assets),
        WEATHER_SECTION => weather_settings(out, state, assets),
        crate::state::SMARTHOME_SECTION => smart_home_settings_view::render(out, state, assets),
        SYSTEM_SECTION => {
            for (position, (index, title, icon)) in [
                (
                    3,
                    language.t(if state.loading {
                        "refreshNotice"
                    } else {
                        "refreshApps"
                    }),
                    "refresh",
                ),
                (5, language.t("quitApp"), "power"),
                (7, "Original LG Home", "tv"),
                (8, "Video library · full screen", "tv"),
            ]
            .into_iter()
            .enumerate()
            {
                row(
                    out,
                    state,
                    assets,
                    Row {
                        target: Target::Setting(index),
                        rect: Rect::new(640., 164.8 + position as f32 * 84. - s, 1120., 72.),
                        title,
                        icon: Some(icon),
                        value: None,
                        description: None,
                        selected: false,
                        chevron: false,
                    },
                );
            }
            if let Some(root) = state.root {
                root_feature_rows(out, state, assets, root);
            }
        }
        _ => input_settings_groups(out, state, assets),
    }
}

/// Settings › System › Root features: shown only when Homebrew has root.
fn root_feature_rows(
    out: &mut Scene,
    state: &State,
    assets: &Assets,
    root: crate::platform::RootStatus,
) {
    let language = state.preferences.language;
    let x = tvos_settings_view::RIGHT + tvos_settings_view::S32;
    let w = tvos_settings_view::WIDTH - 2. * tvos_settings_view::S32;
    let mut y = tvos_settings_view::TOP + tvos_settings_view::S32 + 4. * 92. + 16. - state.scroll;
    out.text(
        "ROOT FEATURES",
        Rect::new(x + 32., y + 6., w - 64., 36.),
        text(25., 600, 0.04),
        c(0xffffff, 0.55),
        false,
    );
    y += 48.;
    let state_text = |on: bool| language.t(if on { "stateOn" } else { "stateOff" });
    tvos_settings_view::cell(
        out,
        state,
        assets,
        Target::Setting(9),
        Rect::new(x, y, w, 104.),
        tvos_settings_view::CellContent {
            name: "Open Luma at power on",
            value: Some(state_text(root.wake)),
            desc: Some("Instead of the last input or LG Home"),
            chevron: false,
            checked: false,
            partial: false,
        },
    );
    y += 116.;
    tvos_settings_view::cell(
        out,
        state,
        assets,
        Target::Setting(10),
        Rect::new(x, y, w, 104.),
        tvos_settings_view::CellContent {
            name: "LG Home Auto Launch",
            value: Some(match root.home_auto_launch {
                Some(on) => state_text(on),
                None => "?",
            }),
            desc: Some("Keep Off · LG: General › System › Additional Settings › Home Settings"),
            chevron: false,
            checked: false,
            partial: false,
        },
    );
}

fn input_settings_groups(out: &mut Scene, state: &State, assets: &Assets) {
    let language = state.preferences.language;
    let x = tvos_settings_view::RIGHT + tvos_settings_view::S32;
    let w = tvos_settings_view::WIDTH - 2. * tvos_settings_view::S32;
    let step = 116.;
    let mut y = tvos_settings_view::TOP + tvos_settings_view::S32 - state.scroll;
    // Never insert blank slots for absent/hidden HDMI ports. Sort only the
    // actual devices and keep the real port id as the stable action target.
    for (connected, heading) in [(true, "CONNECTED"), (false, "NOT CONNECTED")] {
        let mut inputs: Vec<_> = state
            .inventory
            .inputs
            .iter()
            .filter(|input| input.connected.unwrap_or(false) == connected)
            .collect();
        inputs.sort_by_key(|input| {
            input
                .port
                .chars()
                .filter(|c| c.is_ascii_digit())
                .collect::<String>()
                .parse::<u32>()
                .unwrap_or(u32::MAX)
        });
        if inputs.is_empty() {
            continue;
        }
        let group_h = 48. + inputs.len() as f32 * step + 16.;
        out.shape(
            Rect::new(x - 8., y - 8., w + 16., group_h),
            16.,
            c(0xffffff, 0.035),
        );
        out.text(
            heading,
            Rect::new(x + 32., y + 6., w - 64., 36.),
            text(25., 600, 0.04),
            c(0xffffff, 0.55),
            false,
        );
        y += 48.;
        for input in inputs {
            let mode = state
                .preferences
                .input_modes
                .get(&input.id)
                .copied()
                .unwrap_or_default();
            let value = language.t(match mode {
                InputMode::On => "stateOn",
                InputMode::Off => "stateOff",
                InputMode::Auto => "stateAuto",
            });
            let desc = input_status(
                input,
                state.preferences.input_name(input).is_some(),
                language,
            );
            tvos_settings_view::cell(
                out,
                state,
                assets,
                Target::InputMode(input.id.clone()),
                Rect::new(x, y, w, 104.),
                tvos_settings_view::CellContent {
                    name: state.preferences.input_title(input),
                    value: Some(value),
                    desc: Some(&desc),
                    chevron: true,
                    checked: false,
                    partial: false,
                },
            );
            y += step;
        }
        y += 32.;
    }
    tvos_settings_view::cell(
        out,
        state,
        assets,
        Target::Setting(3),
        Rect::new(x, y, w, 80.),
        tvos_settings_view::CellContent {
            name: if state.loading {
                language.t("refreshNotice")
            } else {
                language.t("refreshInputs")
            },
            value: None,
            desc: None,
            chevron: false,
            checked: false,
            partial: false,
        },
    );
}

/// Settings > Background: neutral gallery, not an opaque blue TV-icon strip.
/// Only the nine visible video posters are decoded by the native renderer.
fn background_settings(out: &mut Scene, state: &State, assets: &Assets) {
    let s = state.scroll;
    let browser = state.wallpaper_gallery_video;
    // One segmented control for the whole gallery. Its geometry is fixed so
    // switching Photos/Videos never moves the panel or changes the focus map.
    let mode = Rect::new(640., 164.8, 320., 56.);
    out.glass(mode, 28.);
    for (i, name) in ["Photos", "Videos"].into_iter().enumerate() {
        let item = Rect::new(mode.x + 6. + i as f32 * 154., mode.y + 6., 148., 44.);
        let selected = browser == (i == 1);
        let focused = state.focus == Target::WallpaperTab(i);
        if selected {
            out.shape(item, 22., WHITE);
        }
        if focused {
            out.ring(item, 22.);
        }
        out.text(
            name,
            centered_label(item, 10., 10.),
            text(21., 600, -0.02),
            if selected {
                c(0x202124, 1.)
            } else {
                c(0xffffff, 0.76)
            },
            true,
        );
        out.control(Target::WallpaperTab(i), item, true);
    }
    if !browser {
        for (index, (id, _)) in BACKGROUNDS.iter().enumerate() {
            let rect = Rect::new(
                640. + (index % 3) as f32 * 378.,
                266. + (index / 3) as f32 * 226. - s,
                354.,
                183.,
            );
            out.shadow(rect, 15., 11., 3., 0.12);
            out.image(
                assets.path(format!("thumbnails/{id}.jpg")),
                rect,
                15.,
                false,
            );
            if state.preferences.background == *id {
                out.border(rect.inset(-2.), 17., 2.7, c(0xffffff, 0.95));
                let check = Rect::new(rect.x + rect.w - 36., rect.y + 12., 24., 24.);
                out.shape(check, 12., WHITE);
                out.icon(assets, "check", check.inset(4.), c(0x292929, 1.));
            } else {
                out.border(rect, 15., 1., c(0xffffff, 0.15));
            }
            if state.focus == Target::Background(index) {
                out.ring(rect, 15.);
            }
            out.text(
                state.preferences.language.wallpaper(id),
                Rect::new(rect.x + 5., rect.y + rect.h + 9., 345., 31.),
                text(18.8, 500, -0.02),
                c(0xffffff, 0.91),
                false,
            );
            out.control(
                Target::Background(index),
                Rect::new(rect.x, rect.y, rect.w, rect.h + 38.),
                true,
            );
        }
        return;
    }
    let catalog = crate::video_catalog::all();
    let excluded = &state.preferences.video_excluded;
    let selected = state.video_selected_count();
    let playing = state.preferences.background == "video";
    // The gallery chrome is fixed. Only the grid below it participates in scroll.
    let title = Rect::new(640., 244., 700., 54.);
    out.text(
        "Video wallpapers",
        title,
        text(44., 700, -0.025),
        WHITE,
        false,
    );
    out.text(
        format!("{selected} of {} in rotation", catalog.len()),
        Rect::new(640., 300., 650., 30.),
        text(20., 500, -0.012),
        c(0xffffff, 0.70),
        false,
    );

    let actions = [
        (
            Target::VideoToggle,
            if playing { "Pause" } else { "Play" },
            if playing { "pause" } else { "play" },
            240.,
        ),
        (Target::VideoShuffle, "Shuffle", "shuffle", 188.),
    ];
    let mut action_x = 1312.;
    for (target, label, icon, width) in actions {
        let rect = Rect::new(action_x, 244., width, 56.);
        let focused = state.focus == target;
        let primary = target == Target::VideoToggle;
        let enabled = target != Target::VideoShuffle || state.preferences.video_shuffle;
        out.shape(rect, 28., if primary { WHITE } else { c(0x303135, 0.96) });
        if !primary {
            out.border(
                rect,
                28.,
                1.,
                c(0xffffff, if enabled { 0.28 } else { 0.14 }),
            );
        }
        out.icon(
            assets,
            icon,
            Rect::new(rect.x + 20., rect.y + 18., 20., 20.),
            if primary {
                c(0x202124, 1.)
            } else {
                c(0xffffff, 0.88)
            },
        );
        out.text(
            label,
            centered_label(rect, 52., 18.),
            text(19., 600, -0.012),
            if primary { c(0x202124, 1.) } else { WHITE },
            true,
        );
        if target == Target::VideoShuffle && state.preferences.video_shuffle {
            out.icon(
                assets,
                "check",
                Rect::new(rect.x + rect.w - 31., rect.y + 18., 20., 20.),
                c(0xffffff, 0.9),
            );
        }
        if focused {
            out.ring(rect, 28.);
        }
        out.control(target, rect, true);
        action_x += width + 20.;
    }

    // One stable source segment. Width is based on the label/count pair, never
    // on which item is active, so switching filters cannot reflow this row.
    let source_style = text(18., 600, -0.012);
    let mut source_widths = [0.; crate::video_catalog::FILTERS.len()];
    for (i, (name, _)) in crate::video_catalog::FILTERS.iter().enumerate() {
        let items = state.video_items_for_filter(i);
        let count = items
            .iter()
            .filter(|&&index| !excluded.contains(&catalog[index].id))
            .count();
        let label = if count == 0 {
            (*name).to_string()
        } else {
            format!("{name} {count}")
        };
        source_widths[i] = (assets.width(&label, source_style) + 36.).max(92.);
    }
    let source_total = source_widths.iter().sum::<f32>() + 12.;
    let source_surface = Rect::new(640., 350., source_total, 56.);
    out.glass(source_surface, 28.);
    let mut source_x = source_surface.x + 6.;
    for (i, (name, _)) in crate::video_catalog::FILTERS.iter().enumerate() {
        let items = state.video_items_for_filter(i);
        let count = items
            .iter()
            .filter(|&&index| !excluded.contains(&catalog[index].id))
            .count();
        let label = if count == 0 {
            (*name).to_string()
        } else {
            format!("{name} {count}")
        };
        let item = Rect::new(source_x, source_surface.y + 6., source_widths[i], 44.);
        let active = state.video_filter == i;
        let focused = state.focus == Target::VideoFilter(i);
        if active {
            out.shape(item, 22., WHITE);
        }
        out.text(
            label,
            centered_label(item, 12., 12.),
            source_style,
            if active {
                c(0x202124, 1.)
            } else {
                c(0xffffff, 0.76)
            },
            true,
        );
        if focused {
            out.ring(item, 22.);
        }
        out.control(Target::VideoFilter(i), item, true);
        source_x += source_widths[i];
    }
    // Source changes reset only the grid scroll. The mode/title/actions/source
    // chrome above stays at exactly the same coordinates for every filter.
    let grid_start = out.draw.len();
    let mut y = 446.;
    for (group_index, (source, category, indices)) in state
        .video_groups_for_filter(state.video_filter)
        .iter()
        .enumerate()
    {
        let in_group = indices
            .iter()
            .filter(|&&i| !excluded.contains(&catalog[i].id))
            .count();
        let all_selected = in_group == indices.len();
        let heading_y = y - s;
        let is_all = state.video_filter == 0;
        let title_y = heading_y + if is_all { 23. } else { 0. };
        if heading_y > 20. && title_y < 1110. {
            if is_all {
                out.text(
                    source.as_str(),
                    Rect::new(640., heading_y, 700., 22.),
                    text(18., 600, 0.045),
                    c(0xffffff, 0.58),
                    false,
                );
            }
            out.text(
                category.as_str(),
                Rect::new(640., title_y, 705., 34.),
                text(26., 600, -0.018),
                WHITE,
                false,
            );
            out.text(
                format!("{in_group} of {}", indices.len()),
                centered_label(Rect::new(1335., title_y, 176., 34.), 0., 0.),
                text(18., 500, 0.),
                c(0xffffff, 0.64),
                false,
            );
            let action = Rect::new(1590., title_y - 5., 170., 44.);
            let label = if all_selected {
                "Clear all"
            } else {
                "Select all"
            };
            out.shape(action, 22., c(0x343537, 0.98));
            out.border(action, 22., 1., c(0xffffff, 0.20));
            out.text(
                label,
                centered_label(action, 12., 12.),
                text(18., 600, -0.01),
                WHITE,
                true,
            );
            if state.focus == Target::VideoGroup(group_index) {
                out.ring(action, 22.);
            }
        }
        let action = Rect::new(1590., title_y - 5., 170., 44.);
        out.control(Target::VideoGroup(group_index), action, true);
        let card_y = title_y + 49.;
        y = card_y;
        for (position, index) in indices.iter().enumerate() {
            let x = 640. + position.rem_euclid(4) as f32 * 284.;
            let image_y = y + (position / 4) as f32 * 233. - s;
            let rect = Rect::new(x, image_y, 258., 207.);
            out.control(Target::VideoPick(*index), rect, true);
            // One viewport and one next row: no offscreen JPEG decoding/GPU uploads.
            if image_y > 1200. || image_y + rect.h < 100. {
                continue;
            }
            let chosen = !excluded.contains(&catalog[*index].id);
            let focused = state.focus == Target::VideoPick(*index);
            let lift = if focused && !state.preferences.reduced_motion {
                let t = (state.focus_started.elapsed().as_secs_f32() / 0.18).min(1.);
                1. + 0.08 * (1. - (1. - t).powi(3))
            } else {
                1.
            };
            let poster = Rect::new(x, image_y, 258., 145.125);
            let lift_rect = Rect::new(
                poster.x - (poster.w * (lift - 1.) / 2.),
                poster.y,
                poster.w * lift,
                poster.h * lift,
            );
            if focused {
                out.shadow(lift_rect, 15., 15., 5., 0.33);
            }
            out.shape(lift_rect, 15., c(0x1c1d1e, 1.));
            let path = assets.path(format!("video-thumbnails/{index}.jpg"));
            if std::path::Path::new(&path).is_file() {
                if chosen {
                    out.image(path, lift_rect, 15., false);
                } else {
                    out.draw.push(Draw::TintedImage(
                        path,
                        lift_rect,
                        15.,
                        false,
                        [0.67, 0.67, 0.67, 0.42],
                    ));
                }
            } else {
                out.shape(lift_rect, 15., c(0x37393b, 1.));
            }
            if focused {
                out.border(lift_rect.inset(-3.), 18., 3., WHITE);
            } else {
                out.border(lift_rect, 15., 1., c(0xffffff, 0.13));
            }
            let indicator = Rect::new(lift_rect.x + lift_rect.w - 36., lift_rect.y + 11., 25., 25.);
            if chosen {
                out.shape(indicator, 13., WHITE);
                out.icon(assets, "check", indicator.inset(5.), c(0x202124, 1.));
            } else {
                out.shape(indicator, 13., c(0x141618, 0.42));
                out.border(indicator, 13., 2., WHITE);
            }
            let mut label = catalog[*index].name.clone();
            let style = text(20., 600, -0.015);
            while assets.width(&label, style) > 254. && label.chars().count() > 2 {
                label.pop();
            }
            if label != catalog[*index].name {
                label.push('…');
            }
            out.text(
                label,
                centered_label(Rect::new(x, image_y + 151., 258., 38.), 0., 0.),
                style,
                c(0xffffff, if chosen { 0.95 } else { 0.53 }),
                false,
            );
        }
        y += (indices.len().div_ceil(4) as f32) * 233. + 40.;
    }
    // A short, interruptible fade gives the new grid a clear change without
    // moving any chrome or letting a stale scroll position leak across filters.
    let grid_alpha = if state.preferences.reduced_motion {
        1.
    } else {
        motion::progress(state.video_filter_changed_at.elapsed().as_secs_f32(), 0.15)
    };
    out.fade(grid_start, grid_alpha, 1.);
}

fn greeting_settings(out: &mut Scene, state: &State, assets: &Assets) {
    let language = state.preferences.language;
    let style_value =
        if state.preferences.greeting_style == crate::weather_phrases::GreetingStyle::Rude {
            "Rude"
        } else {
            "Classic"
        };
    row(
        out,
        state,
        assets,
        Row {
            target: Target::GreetingStyle,
            rect: Rect::new(640., 164.8 - state.scroll, 1120., 90.7),
            title: "Greeting style",
            icon: Some("type"),
            value: Some(style_value),
            description: Some("Classic greeting or weather-aware Rude headlines"),
            selected: false,
            chevron: true,
        },
    );
    let description = if state.text_input_active() && state.keyboard_field == TextField::Name {
        format!("{}|", state.greeting_name_draft)
    } else if state.preferences.greeting_name.is_empty() {
        language.t("stateOff").into()
    } else {
        state.preferences.greeting_name.clone()
    };
    row(
        out,
        state,
        assets,
        Row {
            target: Target::GreetingName,
            rect: Rect::new(640., 267.5 - state.scroll, 1120., 90.7),
            title: language.t("name"),
            icon: Some("type"),
            value: None,
            description: Some(&description),
            selected: false,
            chevron: true,
        },
    );
    // Always present so completing name entry cannot move a clicked control.
    row(
        out,
        state,
        assets,
        Row {
            target: Target::GreetingNameClear,
            rect: Rect::new(640., 370.2 - state.scroll, 1120., 72.),
            title: language.t("clearName"),
            icon: Some("refresh"),
            value: None,
            description: None,
            selected: false,
            chevron: false,
        },
    );
    // Keep the Rude preview in the same padded right-hand glass as the rows.
    // All text widths are bounded to the panel, including translated phrases.
    let left = tvos_settings_view::RIGHT + tvos_settings_view::S32;
    let usable = tvos_settings_view::WIDTH - 2. * tvos_settings_view::S32;
    // Clear all three greeting rows, including the 104px description cell.
    let y = tvos_settings_view::TOP + tvos_settings_view::S32 + 3. * 116. + tvos_settings_view::S32;
    out.text(
        "RUDE PREVIEW",
        Rect::new(left, y, usable, 36.),
        text(25., 600, 0.025),
        c(0xffffff, 0.55),
        false,
    );
    if let Some(phrase) = &state.rude_weather.current {
        let style = text(34., 650, -0.025);
        let mut lines: Vec<String> = vec![String::new()];
        for word in phrase.title.split_whitespace() {
            let last = lines.last().cloned().unwrap_or_default();
            let candidate = if last.is_empty() {
                word.into()
            } else {
                format!("{last} {word}")
            };
            if assets.width(&candidate, style) > usable && !last.is_empty() && lines.len() < 3 {
                lines.push(word.to_string());
            } else if let Some(last) = lines.last_mut() {
                *last = candidate;
            }
        }
        for (n, line) in lines.iter().enumerate() {
            out.text(
                tvos_settings_view::ellipsis_for_smart(assets, line, style, usable),
                Rect::new(left, y + 40. + n as f32 * 44., usable, 44.),
                style,
                WHITE,
                false,
            );
        }
        let subtitle_y = y + 40. + lines.len() as f32 * 44. + tvos_settings_view::S8;
        out.text(
            tvos_settings_view::ellipsis_for_smart(
                assets,
                &phrase.subtitle,
                text(25., 500, 0.),
                usable,
            ),
            Rect::new(left, subtitle_y, usable, 41.),
            text(25., 500, 0.),
            c(0xffffff, 0.70),
            false,
        );
    } else {
        out.text(
            "Set a weather location to preview Rude",
            Rect::new(left, y + 48., usable, 44.),
            text(25., 500, 0.),
            c(0xffffff, 0.73),
            false,
        );
    }
}

fn clock_settings(out: &mut Scene, state: &State, assets: &Assets) {
    let language = state.preferences.language;
    for (index, (target, key, value)) in [
        (
            Target::ClockFormat,
            "clockFormat",
            language.t(if state.preferences.hour_24 {
                "hour24"
            } else {
                "hour12"
            }),
        ),
        (
            Target::ClockTimeFormat,
            "timeFormat",
            if state.preferences.show_seconds {
                "HH:MM:SS"
            } else {
                "HH:MM"
            },
        ),
    ]
    .into_iter()
    .enumerate()
    {
        row(
            out,
            state,
            assets,
            Row {
                target,
                rect: Rect::new(640., 164.8 + index as f32 * 84. - state.scroll, 1120., 72.),
                title: language.t(key),
                icon: None,
                value: Some(value),
                description: None,
                selected: false,
                chevron: true,
            },
        );
    }
}

fn app_name_settings(out: &mut Scene, state: &State, assets: &Assets) {
    let language = state.preferences.language;
    for (index, (target, title, enabled)) in [
        (
            Target::AppNamesHome,
            "showOnHome",
            state.preferences.show_home_names,
        ),
        (
            Target::AppNamesApps,
            "showOnApps",
            state.preferences.show_app_names,
        ),
    ]
    .into_iter()
    .enumerate()
    {
        row(
            out,
            state,
            assets,
            Row {
                target,
                rect: Rect::new(640., 164.8 + index as f32 * 84. - state.scroll, 1120., 72.),
                title: language.t(title),
                icon: None,
                value: Some(language.t(if enabled { "stateOn" } else { "stateOff" })),
                description: None,
                selected: false,
                chevron: false,
            },
        );
    }
}

fn input_name_rows(out: &mut Scene, state: &State, assets: &Assets, id: &str, y: f32) {
    let language = state.preferences.language;
    let custom = state.preferences.input_names.get(id);
    let editing = state.text_input_active() && state.keyboard_field == TextField::InputName;
    let description = if editing {
        format!("{}|", state.input_name)
    } else {
        custom
            .cloned()
            .unwrap_or_else(|| language.t("inputNameHint").into())
    };
    row(
        out,
        state,
        assets,
        Row {
            target: Target::InputRename,
            rect: Rect::new(640., y, 1120., 90.7),
            title: language.t("renameInput"),
            icon: Some("type"),
            value: None,
            description: Some(&description),
            selected: false,
            chevron: false,
        },
    );
    if custom.is_some() {
        row(
            out,
            state,
            assets,
            Row {
                target: Target::InputNameReset,
                rect: Rect::new(640., y + 102.7, 1120., 72.),
                title: language.t("clearName"),
                icon: Some("refresh"),
                value: None,
                description: None,
                selected: false,
                chevron: false,
            },
        );
    }
}

/// A custom name such as "PS5" can choose the icon; otherwise the TV's label.
fn input_icon(input: &Input, custom: Option<&str>) -> &'static str {
    custom
        .map(device_icon)
        .filter(|icon| *icon != "hdmi")
        .unwrap_or_else(|| device_icon(&input.title))
}

fn device_icon(name: &str) -> &'static str {
    let name = name.to_lowercase().replace(' ', "");
    if name.contains("playstation") || ["ps1", "ps2", "ps3", "ps4", "ps5"].contains(&name.as_str())
    {
        "playstation"
    } else if name.contains("appletv") {
        "apple-tv"
    } else if name.contains("usb") {
        "usb"
    } else if ["xbox", "switch", "console"]
        .iter()
        .any(|part| name.contains(part))
    {
        "console"
    } else if ["pc", "computer", "laptop"]
        .iter()
        .any(|part| name.contains(part))
    {
        "computer"
    } else if ["stream", "fire", "roku"]
        .iter()
        .any(|part| name.contains(part))
    {
        "streamer"
    } else {
        "hdmi"
    }
}
/// Detail under an input title: the TV's own label (when connected and not just
/// the port), the port when a custom name replaces it, then the status.
pub fn input_status(input: &Input, renamed: bool, language: Language) -> String {
    let mut parts = vec![];
    if input.connected == Some(true) && input.title != input.port {
        parts.push(input.title.as_str());
    }
    if renamed {
        parts.push(input.port.as_str());
    }
    parts.push(match input.connected {
        Some(false) => language.t("noDevice"),
        None => language.t("externalInput"),
        Some(true) => language.t("connected"),
    });
    parts.join(" · ")
}

fn footer(out: &mut Scene, state: &State, assets: &Assets) {
    // Only while an app is being moved: one glass hint, no keycap boxes.
    if state.moving.is_none() {
        return;
    }
    let language = state.preferences.language;
    let hint = format!("← →  {}     OK  {}", language.t("move"), language.t("done"));
    let style = text(22., 500, 0.);
    let width = assets.width(&hint, style) + 64.;
    let rect = Rect::new(960. - width / 2., 1080. - 56. - 64., width, 64.);
    out.shadow(rect, 32., 40., 8., 0.3);
    out.glass(rect, 32.);
    out.text(hint, line_box(rect, style), style, c(0xffffff, 0.86), true);
}

struct EmptyState<'a> {
    rect: Rect,
    icon: &'a str,
    title: &'a str,
    hint: &'a str,
    action: Option<Target>,
}
fn empty(out: &mut Scene, state: &State, assets: &Assets, content: EmptyState<'_>) {
    let EmptyState {
        rect,
        icon,
        title,
        hint,
        action,
    } = content;
    // Our plate, TV-sized type and one round button: no upstream card.
    out.glass(rect, 28.);
    let center = rect.x + rect.w / 2.;
    let height = 40. + 16. + 36. + 8. + 30. + if action.is_some() { 24. + 64. } else { 0. };
    let top = rect.y + ((rect.h - height) / 2.).max(16.);
    out.icon(
        assets,
        icon,
        Rect::new(center - 20., top, 40., 40.),
        c(0xffffff, 0.72),
    );
    let title_style = text(30., 600, -0.02);
    out.text(
        title,
        Rect::new(rect.x + 32., top + 56., rect.w - 64., 36.),
        title_style,
        WHITE,
        true,
    );
    let hint_style = text(22., 400, 0.);
    out.text(
        hint,
        Rect::new(rect.x + 32., top + 100., rect.w - 64., 30.),
        hint_style,
        c(0xffffff, 0.72),
        true,
    );
    if let Some(target) = action {
        let label = state.preferences.language.t(if target == Target::Retry {
            "retry"
        } else {
            "exploreApps"
        });
        let style = text(24., 600, 0.);
        let width = assets.width(label, style) + 2. * 40.;
        let button = Rect::new(center - width / 2., top + 154., width, 64.);
        let focus = state.focus_level(&target);
        out.pill_progress(button, focus, false);
        let ink = blend(c(0xffffff, 0.92), accent_ink(state), focus);
        out.text(label, line_box(button, style), style, ink, true);
        out.control(target, button, true);
    }
}

fn options(out: &mut Scene, state: &State, assets: &Assets, id: &str) {
    // App menu in the shared style: glass panel, round close, round rows,
    // TV-sized type and no key hints.
    let language = state.preferences.language;
    let app = state.inventory.apps.iter().find(|app| app.id == id);
    let title = app.map_or(id, |app| app.title.as_str());
    let title_style = text(38., 600, -0.03).leading(1.1);
    let extra = assets.lines(title, title_style, 440.).saturating_sub(1) as f32 * 42.;
    let (row_h, row_gap) = (72., 16.);
    let panel_h = 40. + 46. + extra + 8. + 30. + 32. + 2. * row_h + row_gap + 40.;
    let panel = Rect::new(660., (1080. - panel_h) / 2., 600., panel_h);
    out.modal = Some(panel);
    out.shadow(panel, 32., 120., 25., 0.47);
    out.glass_on_scrim(panel, 32., 0.3);
    let left = panel.x + 40.;
    out.text(
        title,
        Rect::new(left, panel.y + 40., 440., 46. + extra),
        title_style,
        WHITE,
        false,
    );
    out.text(
        language.category(app.map_or("Utilities", |app| app.category.as_str())),
        Rect::new(left, panel.y + 94. + extra, 440., 30.),
        text(22., 400, 0.),
        c(0xffffff, 0.64),
        false,
    );
    let close = Rect::new(panel.x + panel.w - 40. - 56., panel.y + 36., 56., 56.);
    let focus = state.focus_level(&Target::CloseDialog);
    out.pill_progress(close, focus, false);
    out.icon(
        assets,
        "close",
        close.inset(18.),
        blend(c(0xffffff, 0.92), accent_ink(state), focus),
    );
    out.control(Target::CloseDialog, close, false);
    let favorite = state
        .preferences
        .favorites(&state.inventory.apps)
        .iter()
        .any(|item| item == id);
    let hidden = state.preferences.hidden.iter().any(|item| item == id);
    let label_style = text(24., 500, 0.);
    for (index, label) in [
        if favorite {
            language.t("removeFavorite")
        } else {
            language.t("addFavorite")
        },
        language.t(if hidden { "showApp" } else { "hideApp" }),
    ]
    .into_iter()
    .enumerate()
    {
        let rect = Rect::new(
            left,
            panel.y + 156. + extra + index as f32 * (row_h + row_gap),
            panel.w - 80.,
            row_h,
        );
        let focus = state.focus_level(&Target::Option(index));
        out.pill_progress(rect, focus, false);
        let ink = blend(c(0xffffff, 0.92), accent_ink(state), focus);
        out.icon(
            assets,
            if index == 0 {
                if favorite { "heart-filled" } else { "heart" }
            } else if hidden {
                "eye"
            } else {
                "eye-off"
            },
            Rect::new(rect.x + 28., rect.y + (row_h - 26.) / 2., 26., 26.),
            ink,
        );
        out.text(
            label,
            line_box(
                Rect::new(rect.x + 72., rect.y, rect.w - 100., row_h),
                label_style,
            ),
            label_style,
            ink,
            false,
        );
        out.control(Target::Option(index), rect, false);
    }
}

fn keyboard(out: &mut Scene, state: &State, assets: &Assets) {
    let language = state.preferences.language;
    let panel = Rect::new(350., 290., 1220., 530.);
    out.modal = Some(panel);
    out.shadow(panel, 27.2, 90., 20., 0.45);
    out.glass_on_scrim(panel, 27.2, 0.3);
    out.text(
        match state.keyboard_field {
            TextField::HomeAddress => "Home Assistant server",
            TextField::MoonlightAddress => "PC IPv4 address",
            TextField::Location => language.t("cityPostcode"),
            TextField::Name => language.t("greetingName"),
            TextField::InputName => language.t("inputName"),
            TextField::Apps => language.t("findApp"),
            TextField::Library => "Find a title",
        },
        Rect::new(389., 324., 1000., 44.),
        text(37.6, 500, -0.055).leading(1.1),
        WHITE,
        false,
    );
    let close = Rect::new(1497., 328., 33.6, 33.6);
    out.icon(assets, "close", close.inset(7.6), WHITE);
    out.control(Target::CloseDialog, close, false);
    if state.focus == Target::CloseDialog {
        out.border(close, 16.8, 2., WHITE);
    }
    out.shape(Rect::new(389., 390., 1142., 56.), 28., c(0x171c20, 0.72));
    out.icon(
        assets,
        "search",
        Rect::new(409., 408., 20., 20.),
        c(0xeeeeee, 0.73),
    );
    out.text(
        format!("{}|", state.editing_text()),
        Rect::new(446., 406., 1058., 25.),
        text(20.8, 400, -0.02),
        WHITE,
        false,
    );
    for (index, key) in KEYS.iter().enumerate() {
        let label = match *key {
            "Space" => language.t("space"),
            "Delete" => language.t("delete"),
            "Clear" => language.t("clear"),
            "Done" => language.t("done"),
            _ => key,
        };
        let rect = Rect::new(
            389. + (index % 10) as f32 * 115.6,
            474. + (index / 10) as f32 * 74.,
            101.6,
            60.,
        );
        let focused = state.focus == Target::Key(index);
        out.pill(rect, focused, false);
        let mut style = text(18.4, 550, -0.02);
        let width = assets.width(label, style);
        if width > rect.w - 12. {
            style.size *= (rect.w - 12.) / width;
        }
        out.text(
            label,
            Rect::new(rect.x, rect.y + 18., rect.w, 25.),
            style,
            if focused { c(0x2c2c2c, 1.) } else { WHITE },
            true,
        );
        out.control(Target::Key(index), rect, false);
    }
}

fn toast(out: &mut Scene, state: &State, assets: &Assets, notice: &str) {
    // Our plate with TV-sized text and a round close button.
    let start = out.draw.len();
    let style = text(22., 500, 0.).leading(1.4);
    let width =
        (assets.width(notice, style) + 12. + 32. + 28. + 16. + 24. + 48. + 32.).clamp(360., 1600.);
    let lines = assets.lines(notice, style, width - 180.).max(1) as f32;
    let height = (lines * 31. + 40.).max(80.);
    let rect = Rect::new(960. - width / 2., 1080. - 56. - height, width, height);
    let radius = 40f32.min(height / 2.);
    out.shadow(rect, radius, 40., 8., 0.33);
    out.glass(rect, radius);
    out.icon(
        assets,
        "sparkles",
        Rect::new(rect.x + 32., rect.y + (height - 28.) / 2., 28., 28.),
        c(0xffffff, 0.8),
    );
    let body = Rect::new(rect.x + 76., rect.y, width - 180., height);
    let body = if lines > 1. {
        Rect::new(
            body.x,
            rect.y + (height - lines * 31.) / 2.,
            body.w,
            lines * 31.,
        )
    } else {
        line_box(body, style)
    };
    out.text(notice, body, style, WHITE, false);
    let close = Rect::new(
        rect.x + width - 24. - 48.,
        rect.y + (height - 48.) / 2.,
        48.,
        48.,
    );
    let focus = state.focus_level(&Target::DismissNotice);
    out.pill_progress(close, focus, false);
    let ink = blend(c(0xffffff, 0.92), accent_ink(state), focus);
    out.icon(assets, "close", close.inset(14.), ink);
    if state.overlay.is_none() {
        out.control(Target::DismissNotice, close, false);
    }
    if let Some((_, opened)) = &state.notice {
        if state.preferences.reduced_motion {
            return;
        }
        let t = opened.elapsed().as_secs_f32();
        let (alpha, dy) = if t > 5. {
            let end = motion::progress(t - 5., motion::PANEL_OUT);
            (
                1. - end,
                if state.preferences.reduced_motion {
                    0.
                } else {
                    12. * end
                },
            )
        } else {
            let enter = motion::progress(
                t,
                if state.preferences.reduced_motion {
                    motion::PANEL_OUT
                } else {
                    motion::PANEL_IN
                },
            );
            (
                enter,
                if state.preferences.reduced_motion {
                    0.
                } else {
                    12. * (1. - enter)
                },
            )
        };
        motion::transform_group(&mut out.draw[start..], 0., dy, alpha);
    }
}

const SMART_X: f32 = 84.;
const SMART_W: f32 = 1920. - 2. * SMART_X;
const SMART_GAP: f32 = 24.;
const SMART_RADIUS: f32 = 28.;
use crate::state::SMART_REVEAL_TOP;

/// Card entrance shared with Apps: one beat per visible tile in reading order.
/// Page entrance: a top-left to bottom-right wave by on-screen position;
/// each tile rises and fades in as one piece.
fn smart_enter(out: &mut Scene, state: &State, from: usize, plate: Rect) {
    if state.preferences.reduced_motion {
        return;
    }
    let delay = ((plate.y - MAIN_TOP).max(0.) / 1080.).min(1.) * 0.36 + plate.x / 1920. * 0.12;
    let t = motion::progress(
        (state.page_started.elapsed().as_secs_f32() - delay).max(0.),
        motion::CARD_IN,
    );
    if t >= 1. {
        return;
    }
    motion::transform_group(&mut out.draw[from..], 0., motion::CARD_RISE * (1. - t), t);
}
fn smart_camera_offline(state: &State, e: &crate::smart_home::Entity) -> bool {
    !state.ha_online
        || matches!(e.state.as_str(), "unavailable" | "unknown")
        || state.ha_snapshot_failed.contains(&e.id)
}
/// Centered icon and line on a camera tile without a live picture.
fn smart_camera_status(out: &mut Scene, assets: &Assets, r: Rect, label: &str, icon: &str) {
    let place = Rect::new(r.x + r.w / 2. - 22., r.y + r.h / 2. - 46., 44., 44.);
    out.icon(assets, icon, place, c(0xffffff, 0.8));
    out.text(
        label,
        Rect::new(r.x, r.y + r.h / 2. + 12., r.w, 36.),
        text(24., 600, 0.),
        c(0xffffff, 0.8),
        true,
    );
}
fn smart_onscreen(r: Rect) -> bool {
    r.y + r.h > MAIN_TOP && r.y < 1080.
}
/// Focused tiles lift like Apps cards: soft shadow, slight zoom, accent hairline.
fn smart_focus(out: &mut Scene, state: &State, target: &Target, r: Rect) -> (Rect, f32) {
    let level = state.focus_level(target);
    let zoom = if state.preferences.reduced_motion {
        1.
    } else {
        1. + 0.035 * level
    };
    let r = Rect::new(
        r.x - r.w * (zoom - 1.) / 2.,
        r.y - r.h * (zoom - 1.) / 2.,
        r.w * zoom,
        r.h * zoom,
    );
    out.accent_glow(r, SMART_RADIUS, level, state);
    (r, level)
}
/// Smart Home button: the shared glass at rest, the accent fill when focused.
fn smart_button(out: &mut Scene, state: &State, r: Rect, level: f32) -> Color {
    out.glass(r, r.h / 2.);
    if level > 0. {
        out.shadow(r, r.h / 2., 40., 16., 0.4 * level);
        let at = out.draw.len();
        accent_fill(out, r, r.h / 2., state);
        motion::transform_group(&mut out.draw[at..], 0., 0., level);
    }
    movie_view::mix(WHITE, accent_ink(state), level)
}
fn smart_heading(out: &mut Scene, label: &str, y: f32) {
    out.text(
        label,
        Rect::new(SMART_X, y, 800., 40.),
        text(28., 600, -0.015),
        WHITE,
        false,
    );
}
fn smart_label(
    out: &mut Scene,
    assets: &Assets,
    label: &str,
    r: Rect,
    style: TextStyle,
    ink: Color,
) {
    out.text(
        tvos_settings_view::ellipsis_for_smart(assets, label, style, r.w),
        r,
        style,
        ink,
        false,
    );
}
fn smart_value(e: &crate::smart_home::Entity) -> String {
    if let (Some(n), true) = (e.brightness, e.on()) {
        return format!("{n}%");
    }
    match e.state.as_str() {
        "unavailable" | "unknown" => "Unavailable".into(),
        _ if !e.unit.is_empty() => format!("{} {}", e.state, e.unit),
        "on" => "On".into(),
        "off" => "Off".into(),
        other => {
            let mut chars = other.chars();
            chars
                .next()
                .map(|first| first.to_uppercase().chain(chars).collect())
                .unwrap_or_default()
        }
    }
}

fn smart_home_view(out: &mut Scene, state: &State, assets: &Assets) {
    let s = state.scroll;
    let devices = state.ha_tab(0);
    let cameras = state.ha_tab(1);
    let scenes = state.ha_tab(2);
    if devices.is_empty() && cameras.is_empty() && scenes.is_empty() {
        let card = Rect::new(560., 330., 800., 380.);
        let start = out.draw.len();
        out.glass(card, 36.);
        let (title, detail) = if state.ha_entities.is_empty() {
            if state.ha_online {
                (
                    "Loading your home…",
                    "Devices appear as soon as Home Assistant answers.",
                )
            } else {
                (
                    "Can't reach Home Assistant",
                    "Luma keeps retrying. Check that the server is on.",
                )
            }
        } else {
            (
                "Nothing to show yet",
                "Choose devices, cameras and scenes in Settings.",
            )
        };
        out.text(
            title,
            Rect::new(card.x, card.y + 70., card.w, 56.),
            text(42., 600, -0.02),
            WHITE,
            true,
        );
        out.text(
            detail,
            Rect::new(card.x + 40., card.y + 140., card.w - 80., 36.),
            text(25., 500, 0.),
            c(0xffffff, 0.7),
            true,
        );
        let button = Rect::new(card.x + (card.w - 320.) / 2., card.y + 236., 320., 76.);
        let ink = smart_button(out, state, button, state.focus_level(&Target::SmartConnect));
        out.text(
            "Open Settings",
            Rect::new(button.x, button.y + 20., button.w, 36.),
            text(25., 600, 0.),
            ink,
            true,
        );
        out.control(Target::SmartConnect, button, false);
        smart_enter(out, state, start, card);
        return;
    }

    // Summary chips: only categories that exist, over the shared glass.
    let useful: Vec<_> = state.ha_entities.iter().filter(|e| e.useful()).collect();
    let lights: Vec<_> = useful.iter().filter(|e| e.domain == "light").collect();
    let locks: Vec<_> = useful.iter().filter(|e| e.domain == "lock").collect();
    let openings: Vec<_> = useful
        .iter()
        .filter(|e| {
            e.domain == "binary_sensor"
                && matches!(
                    e.device_class.as_str(),
                    "door" | "window" | "garage_door" | "opening"
                )
        })
        .collect();
    let climate = useful
        .iter()
        .find(|e| e.domain == "sensor" && matches!(e.unit.as_str(), "°C" | "°F"));
    let mut chips: Vec<(&str, String, &str)> = Vec::new();
    if !state.ha_online {
        chips.push(("Home Assistant", "Reconnecting…".into(), "refresh"));
    }
    if let Some(e) = climate {
        chips.push(("Climate", format!("{}{}", e.state, e.unit), "thermometer"));
    }
    if let Some(alarm) = state
        .ha_entities
        .iter()
        .find(|e| e.domain == "alarm_control_panel" && !e.hidden)
    {
        let value = match alarm.state.as_str() {
            "disarmed" => "Disarmed",
            "armed_home" => "Armed · Home",
            "armed_away" => "Armed · Away",
            "armed_night" => "Armed · Night",
            "armed_vacation" => "Armed · Vacation",
            "arming" => "Arming…",
            "pending" => "Pending…",
            "triggered" => "Triggered",
            _ => "Unavailable",
        };
        chips.push(("Alarm", value.into(), "smart-lock"));
    }
    if !lights.is_empty() {
        chips.push((
            "Lights",
            format!("{} on", lights.iter().filter(|e| e.on()).count()),
            "smart-light",
        ));
    }
    if !locks.is_empty() {
        let locked = locks.iter().all(|e| e.state == "locked");
        chips.push((
            "Security",
            if locked { "Locked" } else { "Unlocked" }.into(),
            "smart-lock",
        ));
    }
    if !openings.is_empty() {
        let open = openings.iter().filter(|e| e.on()).count();
        chips.push((
            "Doors",
            if open == 0 {
                "All closed".into()
            } else {
                format!("{open} open")
            },
            "smart-door",
        ));
    }
    // Explicit sizes: text() snaps everything under 27 px to the 25 px caption.
    let title_style = TextStyle::new(22.).weight(600).leading(1.2);
    let value_style = TextStyle::new(18.).weight(500).leading(1.2);
    let mut x = SMART_X;
    let chip_y = SMART_REVEAL_TOP + 6. - s;
    for (title, value, icon) in &chips {
        let w = assets
            .width(title, title_style)
            .max(assets.width(value, value_style))
            .ceil()
            + 116.;
        let r = Rect::new(x, chip_y, w, 84.);
        if smart_onscreen(r) {
            let start = out.draw.len();
            out.glass(r, r.h / 2.);
            out.shape(
                Rect::new(r.x + 14., r.y + 14., 56., 56.),
                28.,
                c(0xffffff, 0.16),
            );
            out.icon(
                assets,
                icon,
                Rect::new(r.x + 26., r.y + 26., 32., 32.),
                WHITE,
            );
            out.text(
                *title,
                Rect::new(r.x + 86., r.y + 14., w - 116., 30.),
                title_style,
                WHITE,
                false,
            );
            out.text(
                value.as_str(),
                Rect::new(r.x + 86., r.y + 44., w - 116., 26.),
                value_style,
                c(0xffffff, 0.62),
                false,
            );
            smart_enter(out, state, start, r);
        }
        x += w + 16.;
    }
    let mut y = chip_y + 84. + 40.;

    // Scenes: one row of buttons in the two Luma states (grey, accent when focused).
    if !scenes.is_empty() {
        let mut x = SMART_X;
        let style = text(25., 600, -0.01);
        for e in &scenes {
            let target = Target::SmartScene(e.id.clone());
            let label_w = assets.width(&e.name, style).ceil().min(360.);
            let w = label_w + 104.;
            if x + w > SMART_X + SMART_W {
                break;
            }
            let r = Rect::new(x, y, w, 76.);
            out.control(target.clone(), r, true);
            if smart_onscreen(r) {
                let start = out.draw.len();
                let fired = state.ha_scene_fired.as_ref().is_some_and(|(id, at)| {
                    id == &e.id && at.elapsed().as_secs_f32() < crate::state::SCENE_FIRED
                });
                // Focus fills; a scene that just ran is only outlined.
                let level = state.focus_level(&target);
                let ink = smart_button(out, state, r, level);
                if fired && level < 0.5 {
                    out.accent_ring(r, r.h / 2., state);
                }
                out.icon(
                    assets,
                    if fired { "check" } else { "sparkles" },
                    Rect::new(r.x + 30., r.y + 26., 24., 24.),
                    ink,
                );
                smart_label(
                    out,
                    assets,
                    &e.name,
                    Rect::new(r.x + 72., r.y + 20., label_w + 4., 36.),
                    style,
                    ink,
                );
                smart_enter(out, state, start, r);
            }
            x += w + 16.;
        }
        y += 76. + 48.;
    }

    // Cameras: two columns across the page, picture only.
    if !cameras.is_empty() {
        let at = out.draw.len();
        smart_heading(out, "Cameras", y);
        smart_enter(out, state, at, Rect::new(SMART_X, y, 0., 0.));
        y += 56.;
        let w = (SMART_W - SMART_GAP) / 2.;
        let h = (w * 9. / 16.).round();
        for (i, e) in cameras.iter().enumerate() {
            let col = (i % 2) as f32;
            let row = (i / 2) as f32;
            let base = Rect::new(
                SMART_X + col * (w + SMART_GAP),
                y + row * (h + SMART_GAP),
                w,
                h,
            );
            let target = Target::SmartCamera(e.id.clone());
            out.control(target.clone(), base, true);
            if !smart_onscreen(base) {
                continue;
            }
            let start = out.draw.len();
            let (r, level) = smart_focus(out, state, &target, base);
            out.glass(r, SMART_RADIUS);
            let offline = smart_camera_offline(state, e);
            if state.ha_camera_hidden(&e.id) {
                // Hidden: only the blurred frame, never a sharp one.
                if let Some(path) = state
                    .ha_snapshots
                    .get(&e.id)
                    .filter(|p| p.contains("-blur-"))
                {
                    out.image(path.clone(), r, SMART_RADIUS, false);
                }
                out.shape(r, SMART_RADIUS, c(0x000000, 0.3));
                smart_camera_status(out, assets, r, "Hidden", "eye-off");
            } else if let Some(path) = state.ha_snapshots.get(&e.id) {
                out.image(path.clone(), r, SMART_RADIUS, false);
                if offline {
                    // Last picture stays, dimmed, with a plain label.
                    out.shape(r, SMART_RADIUS, c(0x000000, 0.55));
                    smart_camera_status(out, assets, r, "Camera offline", "alert");
                }
            } else {
                smart_camera_status(
                    out,
                    assets,
                    r,
                    if offline {
                        "Camera offline"
                    } else {
                        "Connecting…"
                    },
                    if offline { "alert" } else { "refresh" },
                );
            }
            if level > 0. {
                out.accent_ring(r, SMART_RADIUS, state);
            }
            smart_enter(out, state, start, base);
        }
        y += cameras.len().div_ceil(2) as f32 * (h + SMART_GAP) + 24.;
    }

    // Devices: four columns of glass tiles; on = accent outline, focus = accent fill.
    if !devices.is_empty() {
        let at = out.draw.len();
        smart_heading(out, "Devices", y);
        smart_enter(out, state, at, Rect::new(SMART_X, y, 0., 0.));
        y += 56.;
        let w = (SMART_W - 3. * SMART_GAP) / 4.;
        let h = 132.;
        for (i, e) in devices.iter().enumerate() {
            let col = (i % 4) as f32;
            let row = (i / 4) as f32;
            let base = Rect::new(
                SMART_X + col * (w + SMART_GAP),
                y + row * (h + SMART_GAP),
                w,
                h,
            );
            let target = Target::SmartEntity(e.id.clone());
            out.control(target.clone(), base, true);
            if !smart_onscreen(base) {
                continue;
            }
            let start = out.draw.len();
            let (r, level) = smart_focus(out, state, &target, base);
            let lit = e.actionable() && e.on() && state.ha_online;
            // Focus fills the tile; a device that is on is only outlined.
            out.glass(r, SMART_RADIUS);
            if level > 0. {
                let at = out.draw.len();
                accent_fill(out, r, SMART_RADIUS, state);
                motion::transform_group(&mut out.draw[at..], 0., 0., level);
            }
            let ink = movie_view::mix(WHITE, accent_ink(state), level);
            let icon = match e.domain.as_str() {
                "light" => "smart-light",
                "lock" => "smart-lock",
                "fan" => "smart-fan",
                "cover" => "smart-door",
                "sensor" | "climate" => "thermometer",
                "binary_sensor" => "eye",
                _ => "power",
            };
            out.shape(
                Rect::new(r.x + 24., r.y + (r.h - 60.) / 2., 60., 60.),
                30.,
                [ink[0], ink[1], ink[2], 0.14],
            );
            out.icon(
                assets,
                icon,
                Rect::new(r.x + 42., r.y + (r.h - 24.) / 2., 24., 24.),
                ink,
            );
            let text_x = r.x + 104.;
            let text_w = r.x + r.w - 24. - text_x;
            smart_label(
                out,
                assets,
                &e.name,
                Rect::new(text_x, r.y + 30., text_w, 36.),
                text(25., 600, -0.01),
                ink,
            );
            smart_label(
                out,
                assets,
                &smart_value(e),
                Rect::new(text_x, r.y + 70., text_w, 32.),
                text(21., 500, 0.),
                [ink[0], ink[1], ink[2], 0.72],
            );
            if lit && level < 0.5 {
                out.accent_ring(r, SMART_RADIUS, state);
            }
            smart_enter(out, state, start, base);
        }
        y += devices.len().div_ceil(4) as f32 * (h + SMART_GAP);
    }
    out.page_bottom = y + s;
}

fn smart_home_detail(out: &mut Scene, state: &State, assets: &Assets) {
    let Some(id) = state.ha_detail.as_ref() else {
        return;
    };
    let Some(e) = state.ha_entities.iter().find(|e| &e.id == id) else {
        return;
    };
    out.controls.clear();
    out.draw.push(Draw::Clip(None));
    out.shape(Rect::new(0., 0., 1920., 1080.), 0., c(0x000000, 0.6));
    let slider = e.domain == "light" && e.brightness.is_some();
    let lit = e.actionable() && e.on() && state.ha_online;
    let area_h = if e.area.is_empty() { 0. } else { 38. };
    let body_h = if slider {
        40. + 320. + 20. + 68.
    } else {
        32. + 56.
    };
    let (pw, ph): (f32, f32) = (
        720.,
        56. + 112. + 28. + 52. + area_h + body_h + 36. + 28. + 52.,
    );
    let panel = Rect::new((1920. - pw) / 2., ((1080. - ph) / 2.).round(), pw, ph);
    let cx = panel.x + pw / 2.;
    out.shadow(panel, 44., 60., 24., 0.45);
    out.glass(panel, 44.);
    let mut y = panel.y + 56.;
    let badge = Rect::new(cx - 56., y, 112., 112.);
    let ink = if lit {
        accent_fill(out, badge, 56., state);
        accent_ink(state)
    } else {
        out.shape(badge, 56., c(0xffffff, 0.14));
        WHITE
    };
    let icon = match e.domain.as_str() {
        "light" => "smart-light",
        "lock" => "smart-lock",
        "fan" => "smart-fan",
        "cover" => "smart-door",
        "sensor" | "climate" => "thermometer",
        "binary_sensor" => "eye",
        _ => "power",
    };
    out.icon(assets, icon, Rect::new(cx - 28., y + 28., 56., 56.), ink);
    y += 112. + 28.;
    let name_style = text(40., 600, -0.02);
    out.text(
        tvos_settings_view::ellipsis_for_smart(assets, &e.name, name_style, pw - 160.),
        Rect::new(panel.x + 80., y, pw - 160., 52.),
        name_style,
        WHITE,
        true,
    );
    y += 52.;
    if !e.area.is_empty() {
        out.text(
            &e.area,
            Rect::new(panel.x + 80., y + 6., pw - 160., 32.),
            text(24., 500, 0.),
            c(0xffffff, 0.7),
            true,
        );
        y += area_h;
    }
    let hint = if slider {
        y += 40.;
        let track = Rect::new(cx - 75., y, 150., 320.);
        out.shape(track, 40., c(0xffffff, 0.14));
        let amount = if e.on() {
            e.brightness.unwrap_or(0) as f32 / 100.
        } else {
            0.
        };
        let fill = (track.h * amount).round();
        if fill > 0. {
            out.draw.push(Draw::Clip(Some(Rect::new(
                track.x,
                track.y + track.h - fill,
                track.w,
                fill,
            ))));
            accent_fill(out, track, 40., state);
            out.draw.push(Draw::Clip(None));
        }
        if state.focus == Target::SmartBrightness(0) {
            out.accent_ring(track, 40., state);
        }
        out.control(Target::SmartBrightness(0), track, false);
        y += 320. + 20.;
        out.text(
            if e.on() {
                format!("{}%", e.brightness.unwrap_or(0))
            } else {
                "Off".into()
            },
            Rect::new(panel.x, y, pw, 68.),
            text(56., 700, -0.02),
            WHITE,
            true,
        );
        y += 68.;
        "▲▼ Brightness · OK On/Off · Back Close"
    } else {
        y += 32.;
        out.text(
            smart_value(e),
            Rect::new(panel.x + 80., y, pw - 160., 56.),
            text(44., 600, -0.02),
            WHITE,
            true,
        );
        out.control(Target::SmartBrightness(0), badge, false);
        y += 56.;
        if e.domain == "light" {
            "OK On/Off · Back Close"
        } else {
            "Back Close"
        }
    };
    out.text(
        hint,
        Rect::new(panel.x + 40., y + 36., pw - 80., 28.),
        text(20., 500, 0.),
        c(0xffffff, 0.6),
        true,
    );
    let close = Rect::new(panel.x + pw - 24. - 64., panel.y + 24., 64., 64.);
    movie_view::close_button(
        out,
        state,
        assets,
        close,
        "close",
        state.focus_level(&Target::SmartClose),
    );
    out.control(Target::SmartClose, close, false);
}
/// Long-press menu of a camera: blur its tile picture, or bring it back.
fn smart_camera_menu(out: &mut Scene, state: &State, assets: &Assets) {
    let Some(id) = state.ha_camera_menu.as_ref() else {
        return;
    };
    let name = state
        .ha_entities
        .iter()
        .find(|e| &e.id == id)
        .map(|e| e.name.as_str())
        .unwrap_or("Camera");
    let hidden = state.ha_camera_hidden(id);
    out.controls.clear();
    out.draw.push(Draw::Clip(None));
    out.shape(Rect::new(0., 0., 1920., 1080.), 0., c(0x000000, 0.6));
    let (pw, ph): (f32, f32) = (720., 56. + 112. + 28. + 52. + 12. + 32. + 40. + 76. + 56.);
    let panel = Rect::new((1920. - pw) / 2., ((1080. - ph) / 2.).round(), pw, ph);
    let cx = panel.x + pw / 2.;
    out.shadow(panel, 44., 60., 24., 0.45);
    out.glass(panel, 44.);
    let mut y = panel.y + 56.;
    out.shape(Rect::new(cx - 56., y, 112., 112.), 56., c(0xffffff, 0.14));
    out.icon(
        assets,
        if hidden { "eye-off" } else { "eye" },
        Rect::new(cx - 28., y + 28., 56., 56.),
        WHITE,
    );
    y += 112. + 28.;
    let name_style = text(40., 600, -0.02);
    out.text(
        tvos_settings_view::ellipsis_for_smart(assets, name, name_style, pw - 160.),
        Rect::new(panel.x + 80., y, pw - 160., 52.),
        name_style,
        WHITE,
        true,
    );
    y += 52. + 12.;
    out.text(
        if hidden {
            "Blurred on its tile. OK still opens it full screen."
        } else {
            "Blur the picture on its tile."
        },
        Rect::new(panel.x + 40., y, pw - 80., 32.),
        TextStyle::new(22.).weight(500).leading(1.2),
        c(0xffffff, 0.7),
        true,
    );
    y += 32. + 40.;
    let label = if hidden { "Show camera" } else { "Hide camera" };
    let style = text(25., 600, -0.01);
    let label_w = assets.width(label, style).ceil();
    let w = label_w + 104.;
    let button = Rect::new(cx - w / 2., y, w, 76.);
    let ink = smart_button(
        out,
        state,
        button,
        state.focus_level(&Target::SmartCameraHide),
    );
    out.icon(
        assets,
        if hidden { "eye" } else { "eye-off" },
        Rect::new(button.x + 30., button.y + 26., 24., 24.),
        ink,
    );
    smart_label(
        out,
        assets,
        label,
        Rect::new(button.x + 72., button.y + 20., label_w + 4., 36.),
        style,
        ink,
    );
    out.control(Target::SmartCameraHide, button, false);
    let close = Rect::new(panel.x + pw - 24. - 64., panel.y + 24., 64., 64.);
    movie_view::close_button(
        out,
        state,
        assets,
        close,
        "close",
        state.focus_level(&Target::SmartClose),
    );
    out.control(Target::SmartClose, close, false);
}
fn smart_home_camera(out: &mut Scene, state: &State, assets: &Assets) {
    let Some(id) = state.ha_camera.as_ref() else {
        return;
    };
    out.controls.clear();
    // Full screen over the tab: outside the page body clip.
    out.draw.push(Draw::Clip(None));
    out.shape(Rect::new(0., 0., 1920., 1080.), 0., c(0x000000, 1.));
    let entity = state.ha_entities.iter().find(|e| &e.id == id);
    let offline = entity.is_some_and(|e| smart_camera_offline(state, e));
    let waited = state.ha_camera_opened.elapsed().as_secs_f32() >= crate::state::CAMERA_WAIT;
    // Full screen is always sharp; a hidden camera's blurred tile frame waits.
    if let Some(path) = state.ha_snapshots.get(id).filter(|p| !p.contains("-blur-")) {
        out.image(path.clone(), Rect::new(0., 0., 1920., 1080.), 0., true);
        if offline {
            let badge = Rect::new(84., 1080. - 60. - 64., 260., 64.);
            out.glass(badge, 32.);
            out.icon(
                assets,
                "alert",
                Rect::new(badge.x + 24., badge.y + 20., 24., 24.),
                WHITE,
            );
            out.text(
                "Camera offline",
                Rect::new(badge.x + 60., badge.y + 18., badge.w - 80., 30.),
                text(22., 600, 0.),
                WHITE,
                false,
            );
        }
    } else if offline || waited {
        out.icon(
            assets,
            "alert",
            Rect::new(960. - 32., 400., 64., 64.),
            c(0xffffff, 0.85),
        );
        out.text(
            "Camera is offline",
            Rect::new(340., 492., 1240., 52.),
            text(40., 600, -0.02),
            WHITE,
            true,
        );
        out.text(
            "Check its power and Wi-Fi, or open it in Home Assistant.",
            Rect::new(340., 556., 1240., 34.),
            text(24., 500, 0.),
            c(0xffffff, 0.7),
            true,
        );
    } else {
        out.text(
            "Connecting to the camera…",
            Rect::new(340., 510., 1240., 60.),
            text(34., 500, -0.02),
            c(0xffffff, 0.82),
            true,
        );
    }
    // Picture only; a single close button for the pointer, Back on the remote.
    let close = Rect::new(1920. - 84. - 64., 60., 64., 64.);
    movie_view::close_button(
        out,
        state,
        assets,
        close,
        "close",
        state.focus_level(&Target::SmartClose),
    );
    out.control(Target::SmartClose, close, false);
}

const AMBIENT_STAGGER: f32 = crate::state::AMBIENT_STAGGER;

/// Clean wallpaper mode: every element slides `dx`/`dy` away and fades at
/// its own place in the order, so blocks leave one after another.
pub(crate) fn ambient_move(
    draws: &mut [Draw],
    state: &State,
    dx: f32,
    dy: f32,
    order: impl Fn(Rect) -> f32,
) {
    if state.ambient.is_none() {
        return;
    }
    for d in draws.iter_mut() {
        let Some(r) = rect_mut(d).map(|r| *r) else {
            continue;
        };
        let k = state.ambient_k(order(r));
        if k != 0. {
            motion::transform(d, dx * k, dy * k, 1. - k.clamp(0., 1.));
        }
    }
}

/// The video caption's press: everything in it shrinks toward its centre.
fn press_scale(draws: &mut [Draw], s: f32) {
    let mut bounds: Option<Rect> = None;
    for r in draws.iter_mut().filter_map(rect_mut) {
        bounds = Some(match bounds {
            None => *r,
            Some(b) => {
                let x = b.x.min(r.x);
                let y = b.y.min(r.y);
                Rect::new(
                    x,
                    y,
                    (b.x + b.w).max(r.x + r.w) - x,
                    (b.y + b.h).max(r.y + r.h) - y,
                )
            }
        });
    }
    let Some(b) = bounds else {
        return;
    };
    let (cx, cy) = (b.x + b.w / 2., b.y + b.h / 2.);
    for d in draws.iter_mut() {
        if let Draw::Text(_, _, style, _, _) = d {
            style.size *= s;
        }
        if let Some(r) = rect_mut(d) {
            *r = Rect::new(cx + (r.x - cx) * s, cy + (r.y - cy) * s, r.w * s, r.h * s);
        }
    }
}

fn rect_mut(d: &mut Draw) -> Option<&mut Rect> {
    match d {
        Draw::Shape(r, ..)
        | Draw::RoundedGradient(r, ..)
        | Draw::DiagonalGradient(r, ..)
        | Draw::HGradient(r, ..)
        | Draw::DiagonalGradientBorder(r, ..)
        | Draw::Shadow(r, ..)
        | Draw::Sheen(r, ..)
        | Draw::Text(_, r, ..)
        | Draw::Image(_, r, ..)
        | Draw::TintedImage(_, r, ..)
        | Draw::AppImage(_, r, _) => Some(r),
        _ => None,
    }
}

#[cfg(test)]
mod tabular_layout_tests {
    use super::*;
    #[test]
    fn all_heading_fonts_have_fixed_width_changing_digits() {
        let mut maps = BTreeMap::new();
        for font in HeadingFont::ALL {
            for weight in [700, 800, 900] {
                maps.insert(
                    format!("{}-heading-{weight}", font.id()),
                    BTreeMap::from([
                        ('0', 0.72),
                        ('1', 0.37),
                        (':', 0.24),
                        ('К', 0.68),
                        ('Д', 0.69),
                    ]),
                );
            }
        }
        let assets = Assets {
            root: PathBuf::new(),
            version: "test".into(),
            metrics: BTreeMap::new(),
            catalog_metrics: maps,
            active_heading: std::cell::Cell::new(crate::typography::HeadingFont::default()),
            active_text: std::cell::Cell::new(crate::typography::TextFont::default()),
            heading_pct: std::cell::Cell::new(100),
            text_pct: std::cell::Cell::new(100),
            artwork: crate::app_artwork::Service::new(false),
        };
        for font in HeadingFont::ALL {
            assets.active_heading.set(font);
            let style = TextStyle::new(108.).weight(800).display(true).tabular();
            let one = assets.width("11:11", style);
            let zero = assets.width("00:00", style);
            assert!((one - zero).abs() < 0.01, "{font:?}: {one} != {zero}");
        }
    }
}
