//! Home widgets on screen: the blocks and the text line right of the
//! headline, the full-screen pages they open, and edit mode. Geometry follows
//! the approved mockup (Home widgets v32): 322 px blocks on the Home grid,
//! one 28 px plate radius, nested plates concentric (outer − padding).
use super::*;
use crate::{
    smart_home::Entity,
    weather::Reading,
    weather_sky::Mood,
    widgets::{self as w, Block, Detail, Kind, RADIUS, Size},
    widgets_state::{
        hdmi_inputs, input_live, input_name, smart_camera, smart_devices, smart_sensor,
    },
};

const GREEN: u32 = 0x8fe3a8;
const RED: u32 = 0xffa98f;
/// Block top before scroll: 2 px under the old weather card, same bottom edge.
const BLOCK_TOP: f32 = WEATHER_TOP + 2.;
/// Index for previews: never focused, never a real block.
pub(super) const PREVIEW: usize = usize::MAX;

fn ts(assets: &Assets, size: f32, weight: u16) -> TextStyle {
    weather_type(assets, size, weight, false)
}
fn td(assets: &Assets, size: f32) -> TextStyle {
    weather_type(assets, size, 700, true)
}
fn trend(pct: f64) -> Color {
    c(if pct >= 0. { GREEN } else { RED }, 1.)
}
/// Text with a soft dark copy under it, so light type reads on any sky.
fn lifted(out: &mut Scene, value: &str, r: Rect, style: TextStyle, color: Color, center: bool) {
    out.text(
        value,
        Rect::new(r.x, r.y + 2., r.w, r.h),
        style,
        c(0x071325, 0.3 * color[3]),
        center,
    );
    out.text(value, r, style, color, center);
}
fn width(assets: &Assets, value: &str, style: TextStyle) -> f32 {
    // ▲ and ▼ render from a fallback font the metrics table does not know.
    let arrows = value.chars().filter(|c| matches!(c, '▲' | '▼')).count() as f32;
    assets.width(value, style).ceil() + 2. + arrows * style.size * 0.7
}

// ------------------------------------------------------------------ layout

pub(super) fn available(state: &State, kind: Kind) -> bool {
    match kind {
        Kind::Weather => state.preferences.show_weather,
        Kind::Crypto => true,
        Kind::Hdmi => !state.inventory.inputs.is_empty(),
        Kind::Smart => !state.preferences.smart_home.server_url.is_empty(),
    }
}
pub(super) struct Zone {
    /// (index in preferences, block, screen rect).
    pub blocks: Vec<(usize, Block, Rect)>,
    pub text: Vec<(usize, Kind)>,
    pub left: f32,
    pub top: f32,
}
pub(super) fn zone(state: &State) -> Zone {
    let prefs = &state.preferences.widgets;
    let blocks: Vec<(usize, Block)> = prefs
        .blocks
        .iter()
        .copied()
        .enumerate()
        .filter(|(_, b)| available(state, b.kind))
        .collect();
    let text: Vec<(usize, Kind)> = prefs
        .text
        .iter()
        .copied()
        .enumerate()
        .filter(|(_, k)| available(state, *k))
        .collect();
    let only: Vec<Block> = blocks.iter().map(|b| b.1).collect();
    let layout = w::layout(
        &only,
        !text.is_empty(),
        HOME_TILE_PITCH,
        HOME_TILE_WIDTH,
        crate::state::HOME_STRIP_RIGHT,
    );
    let top =
        BLOCK_TOP + HOME_DROP - state.scroll - state.bounce_at(crate::state::BounceAxis::Page);
    Zone {
        blocks: blocks
            .into_iter()
            .zip(layout.blocks)
            .map(|((i, b), (x, wd))| (i, b, Rect::new(x, top, wd, w::BLOCK_H)))
            .collect(),
        text,
        left: layout.left,
        top,
    }
}
/// Headline wrap width: it stops 44 px before the widgets.
pub(super) fn phrase_width(state: &State) -> f32 {
    (zone(state).left - 44. - crate::state::HOME_STRIP_LEFT).clamp(520., PHRASE_W)
}
/// Bottom of the blocks before scroll, for the Favorites offset.
pub(super) fn bottom() -> f32 {
    BLOCK_TOP + w::BLOCK_H
}

// --------------------------------------------------------------- Home zone

pub(super) fn home(out: &mut Scene, state: &State, assets: &Assets) {
    let z = zone(state);
    let edit = state.widget_edit;
    if edit.is_some() {
        // Edit mode: everything but the widgets steps back.
        out.shape(Rect::new(0., 0., 1920., 1080.), 0., c(0x000000, 0.5));
    }
    if !z.text.is_empty() {
        let start = out.draw.len();
        text_line(out, state, assets, &z);
        out.enter(start, state, 0.21, 29.);
    }
    let grab = edit.and_then(|e| e.grab);
    for (n, (i, b, r)) in z.blocks.iter().enumerate() {
        if grab.is_some_and(|g| g.index == *i) {
            // Where the carried widget lands.
            out.shape(*r, RADIUS, c(0xffffff, 0.05));
            out.border(*r, RADIUS, 3., c(0xffffff, 0.45));
            continue;
        }
        let start = out.draw.len();
        block(out, state, assets, *i, *b, *r);
        out.enter(start, state, 0.21 + n as f32 * motion::STAGGER, 29.);
    }
    if edit.is_some() {
        edit_chrome(out, state, assets, &z);
    }
    if let Some(g) = grab
        && let Some((i, b, r)) = z.blocks.iter().find(|(i, _, _)| *i == g.index)
    {
        let at = Rect::new(g.at.0 - g.offset.0, g.at.1 - g.offset.1, r.w, r.h);
        let start = out.draw.len();
        out.shadow(at, RADIUS, 80., 34., 0.6);
        block(out, state, assets, *i, *b, at);
        out.focus_ring(at, RADIUS, 1., state);
        let (cx, cy) = at.center();
        scale_group(&mut out.draw[start..], cx, cy, 1.03);
        // The carried widget has no hit box of its own.
        out.controls.retain(|c| c.target != Target::Widget(*i));
        out.control(Target::Widget(*i), *r, true);
    }
}

/// One block at `r`. `index` is its place in preferences; PREVIEW draws it
/// without focus or controls.
pub(super) fn block(
    out: &mut Scene,
    state: &State,
    assets: &Assets,
    index: usize,
    b: Block,
    r: Rect,
) {
    let target = Target::Widget(index);
    let level = if index == PREVIEW {
        0.
    } else {
        state.focus_level(&target)
    };
    let controls = out.controls.len();
    let whole = match b.kind {
        Kind::Weather => {
            weather_block(out, state, assets, b.size, r);
            true
        }
        Kind::Crypto => {
            crypto_block(out, state, assets, b.size, r);
            true
        }
        Kind::Hdmi => hdmi_block(out, state, assets, index, b.size, r),
        Kind::Smart => smart_block(out, state, assets, index, b.size, r),
    };
    if whole {
        out.focus_ring(r, RADIUS, level, state);
    }
    if index == PREVIEW {
        out.controls.truncate(controls);
    } else if whole {
        out.controls.insert(
            controls,
            Control {
                target,
                rect: r,
                scrollable: true,
            },
        );
    }
}

// ----------------------------------------------------------------- weather

fn synthetic(code: u8, is_day: bool, temperature: f64) -> Reading {
    Reading {
        temperature,
        code,
        is_day,
        wind_speed: 0.,
        temperature_3h_ago: None,
        hours: [None; 5],
    }
}
/// The sky mood and reading Home shows; desktop QA can force a mood with
/// LUMA_WEATHER_PREVIEW_MOOD.
pub(super) fn weather_now(state: &State) -> (Mood, Option<Reading>) {
    let real = state
        .preferences
        .weather_location
        .as_ref()
        .and_then(|place| state.weather.reading(place));
    #[cfg(debug_assertions)]
    let preview = std::env::var("LUMA_WEATHER_PREVIEW_MOOD")
        .ok()
        .and_then(|name| Mood::from_name(&name));
    #[cfg(not(debug_assertions))]
    let preview: Option<Mood> = None;
    let mood = preview
        .or_else(|| real.map(Mood::from_reading))
        .unwrap_or(Mood::Clear);
    let fallback = preview.map(|m| {
        let code = match m {
            Mood::Clear | Mood::Wind | Mood::Night => 0,
            Mood::Partly | Mood::NightCloudy => 2,
            Mood::Cloudy => 3,
            Mood::Fog => 45,
            Mood::Rain => 61,
            Mood::Storm => 95,
            Mood::Snow => 71,
        };
        let mut r = synthetic(code, !matches!(m, Mood::Night | Mood::NightCloudy), 22.);
        for (i, h) in r.hours.iter_mut().enumerate() {
            *h = Some(crate::weather::HourForecast {
                hour: (14 + i as u8) % 24,
                temperature: 23. + i as f64,
                code,
                is_day: true,
            });
        }
        r
    });
    (mood, real.or(fallback))
}
fn forecast(state: &State) -> Option<&w::Forecast> {
    let key = state.preferences.weather_location.as_ref()?.key();
    state.widget_data.forecast.as_ref().filter(|f| f.key == key)
}
fn temp(state: &State, value: f64) -> String {
    weather::format_temperature(value, state.preferences.temperature_unit)
}
/// Compact temperature for rows and columns: "14°".
fn deg(state: &State, value: f64) -> String {
    temp(state, value).trim_end_matches(['C', 'F']).to_owned()
}
fn condition_text(state: &State, r: Reading) -> String {
    state
        .preferences
        .language
        .t(weather::condition(r).key)
        .to_owned()
}
fn high_low(state: &State) -> Option<String> {
    let d = forecast(state)?.days.first()?;
    Some(format!("H {}  L {}", deg(state, d.max), deg(state, d.min)))
}
fn city(state: &State) -> String {
    state
        .preferences
        .weather_location
        .as_ref()
        .map_or_else(|| "Set weather location".to_owned(), |l| l.name.clone())
}
fn hero_icon(mood: Mood, r: Reading) -> &'static str {
    match mood {
        Mood::Night => "weather-moon",
        Mood::NightCloudy | Mood::Wind => "weather-cloud",
        _ => condition_icon(weather::condition(r).icon),
    }
}
/// The condition icon with the pulsing sun glow, centered on (cx, cy).
fn hero(
    out: &mut Scene,
    state: &State,
    assets: &Assets,
    mood: Mood,
    r: Reading,
    at: (f32, f32),
    d: f32,
) {
    if matches!(mood, Mood::Clear | Mood::Partly) {
        let pulse = 1.
            + if state.preferences.reduced_motion {
                0.
            } else {
                0.06 * (state.page_started.elapsed().as_secs_f32() * 1.2).sin()
            };
        let g = d * 1.45 * pulse;
        out.draw.push(Draw::TintedImage(
            assets.path("weather-sky/glow.png"),
            Rect::new(at.0 - g / 2., at.1 - g / 2., g, g),
            0.,
            false,
            [1., 1., 1., 0.7],
        ));
    }
    out.icon(
        assets,
        hero_icon(mood, r),
        Rect::new(at.0 - d / 2., at.1 - d / 2., d, d),
        c(0xfff8ef, 1.),
    );
}
fn city_line(out: &mut Scene, state: &State, assets: &Assets, x: f32, y: f32, max: f32, size: f32) {
    let style = ts(assets, size, 500);
    let icon = size;
    out.icon(
        assets,
        "location",
        Rect::new(x, y + (size * 1.36 - icon) / 2., icon, icon),
        c(0xffffff, 0.92),
    );
    lifted(
        out,
        &clip_weather_line(&city(state), max - icon - 8., style, assets),
        Rect::new(x + icon + 8., y, max - icon - 8., size * 1.4),
        style,
        c(0xffffff, 0.92),
        false,
    );
}
fn sky_plate(out: &mut Scene, state: &State, assets: &Assets, r: Rect, radius: f32, mood: Mood) {
    out.shadow(r, radius, 24., 8., 0.16);
    // The sky is translucent: frost under it so the wallpaper never shows through sharp.
    out.frost(r, radius);
    crate::weather_sky::draw(
        &mut out.draw,
        &assets.root,
        r,
        radius,
        mood,
        state.page_started.elapsed().as_secs_f32(),
        state.preferences.reduced_motion,
    );
    out.border(r, radius, 1., c(0xffffff, 0.14));
}

fn weather_block(out: &mut Scene, state: &State, assets: &Assets, size: Size, r: Rect) {
    let (mood, reading) = weather_now(state);
    sky_plate(out, state, assets, r, RADIUS, mood);
    let Some(now) = reading else {
        let message = if state.weather.loading {
            "Updating forecast"
        } else if state.preferences.weather_location.is_none() {
            "Choose a city in Settings"
        } else {
            "Forecast unavailable"
        };
        let style = ts(assets, 22., 600);
        out.text(
            clip_weather_line(message, r.w - 44., style, assets),
            Rect::new(r.x + 22., r.y + r.h / 2. - 16., r.w - 44., 32.),
            style,
            WHITE,
            size != Size::L,
        );
        return;
    };
    match size {
        Size::S => weather_s(out, state, assets, r, mood, now),
        Size::L => weather_l(out, state, assets, r, mood, now),
        _ => weather_m(out, state, assets, r, mood, now),
    }
}
fn weather_s(out: &mut Scene, state: &State, assets: &Assets, r: Rect, mood: Mood, now: Reading) {
    // Under the temperature, right: a wide reading never runs into it.
    hero(
        out,
        state,
        assets,
        mood,
        now,
        (r.x + r.w - 66., r.y + 186.),
        84.,
    );
    city_line(out, state, assets, r.x + 22., r.y + 20., 150., 18.);
    lifted(
        out,
        &temp(state, now.temperature),
        Rect::new(r.x + 22., r.y + 49., r.w - 44., 80.),
        td(assets, 68.),
        WHITE,
        false,
    );
    let mut y = r.y + r.h - 20.;
    if let Some(hl) = high_low(state) {
        y -= 25.;
        lifted(
            out,
            &hl,
            Rect::new(r.x + 22., y, r.w - 44., 25.),
            ts(assets, 18., 500),
            c(0xffffff, 0.85),
            false,
        );
    }
    let style = ts(assets, 20., 500);
    lifted(
        out,
        &clip_weather_line(&condition_text(state, now), r.w - 44., style, assets),
        Rect::new(r.x + 22., y - 28., r.w - 44., 28.),
        style,
        WHITE,
        false,
    );
}
fn weather_m(out: &mut Scene, state: &State, assets: &Assets, r: Rect, mood: Mood, now: Reading) {
    hero(
        out,
        state,
        assets,
        mood,
        now,
        (r.x + r.w - 92., r.y + 92.),
        108.,
    );
    city_line(out, state, assets, r.x + 20., r.y + 22., r.w - 200., 20.);
    let t = temp(state, now.temperature);
    let big = td(assets, 72.);
    lifted(
        out,
        &t,
        Rect::new(r.x + 20., r.y + 50., 300., 86.),
        big,
        WHITE,
        false,
    );
    let x = r.x + 20. + width(assets, &t, big) + 16.;
    let side = (r.x + r.w - 170. - x).max(80.);
    let style = ts(assets, 22., 500);
    lifted(
        out,
        &clip_weather_line(&condition_text(state, now), side, style, assets),
        Rect::new(x, r.y + 63., side, 30.),
        style,
        WHITE,
        false,
    );
    if let Some(hl) = high_low(state) {
        lifted(
            out,
            &hl,
            Rect::new(x, r.y + 95., side, 25.),
            ts(assets, 18., 500),
            c(0xffffff, 0.85),
            false,
        );
    }
    // Six-hour strip, concentric with the plate: inset 20, radius 28 − 20.
    let strip = Rect::new(r.x + 20., r.y + r.h - 20. - 76., r.w - 40., 76.);
    out.shape(strip, w::inner_radius(RADIUS, 20.), c(0xffffff, 0.14));
    let cell = (strip.w - 12.) / 6.;
    for i in 0..6usize {
        let x = strip.x + 6. + i as f32 * cell;
        let (label, value) = if i == 0 {
            ("Now".to_owned(), deg(state, now.temperature))
        } else if let Some(h) = now.hours[i - 1] {
            (format!("{:02}:00", h.hour), deg(state, h.temperature))
        } else {
            ("--".into(), "--".into())
        };
        let delay = 0.1 + i as f32 * 0.05;
        let alpha = if state.preferences.reduced_motion {
            1.
        } else {
            motion::progress(state.page_started.elapsed().as_secs_f32() - delay, 0.4)
        };
        out.text(
            label,
            Rect::new(x, strip.y + 10., cell, 25.),
            ts(assets, 18., 500),
            c(0xffffff, 0.85 * alpha),
            true,
        );
        out.text(
            value,
            Rect::new(x, strip.y + 36., cell, 30.),
            ts(assets, 22., 600),
            c(0xffffff, alpha),
            true,
        );
    }
}
fn weather_l(out: &mut Scene, state: &State, assets: &Assets, r: Rect, mood: Mood, now: Reading) {
    let left = Rect::new(r.x + 26., r.y + 22., 330., r.h - 44.);
    city_line(out, state, assets, left.x, left.y, left.w, 20.);
    lifted(
        out,
        &temp(state, now.temperature),
        Rect::new(left.x, left.y + 31., left.w, 86.),
        td(assets, 72.),
        WHITE,
        false,
    );
    let style = ts(assets, 22., 500);
    lifted(
        out,
        &clip_weather_line(&condition_text(state, now), left.w, style, assets),
        Rect::new(left.x, left.y + 113., left.w, 30.),
        style,
        WHITE,
        false,
    );
    if let Some(hl) = high_low(state) {
        lifted(
            out,
            &hl,
            Rect::new(left.x, left.y + 143., left.w, 25.),
            ts(assets, 18., 500),
            c(0xffffff, 0.85),
            false,
        );
    }
    if let Some(f) = forecast(state) {
        let line = format!(
            "Wind {:.0} m/s · Humidity {:.0}% · UV {:.0}",
            f.wind_ms, f.humidity, f.uv
        );
        let style = ts(assets, 18., 500);
        lifted(
            out,
            &clip_weather_line(&line, left.w, style, assets),
            Rect::new(left.x, left.y + left.h - 25., left.w, 25.),
            style,
            c(0xffffff, 0.85),
            false,
        );
    }
    // Five days on a panel inset 16, radius 28 − 16.
    let panel = Rect::new(left.x + left.w + 28., r.y + 16., 0., r.h - 32.);
    let panel = Rect::new(panel.x, panel.y, r.x + r.w - 16. - panel.x, panel.h);
    out.shape(panel, w::inner_radius(RADIUS, 16.), c(0xffffff, 0.14));
    let Some(f) = forecast(state) else {
        hero(out, state, assets, mood, now, panel.center(), 108.);
        return;
    };
    days(
        out,
        state,
        assets,
        &f.days,
        5,
        Rect::new(panel.x + 20., panel.y + 12., panel.w - 40., panel.h - 24.),
        DayScale {
            size: 20.,
            icon: 28.,
        },
    );
}
/// Text size and icon side of the day rows.
struct DayScale {
    size: f32,
    icon: f32,
}
/// Day rows: label, icon, low, range bar, high. Rows share `r` evenly.
fn days(
    out: &mut Scene,
    state: &State,
    assets: &Assets,
    list: &[w::Day],
    n: usize,
    r: Rect,
    scale: DayScale,
) {
    let DayScale { size, icon } = scale;
    let shown = &list[..list.len().min(n)];
    if shown.is_empty() {
        return;
    }
    let lo = shown.iter().map(|d| d.min).fold(f64::MAX, f64::min);
    let hi = shown.iter().map(|d| d.max).fold(f64::MIN, f64::max);
    let span = (hi - lo).max(1.);
    let row = r.h / n as f32;
    let style = ts(assets, size, 500);
    let label_w = size * 3.6;
    let num_w = size * 2.6;
    for (i, d) in shown.iter().enumerate() {
        let y = r.y + i as f32 * row;
        let mid = y + row / 2.;
        if i > 0 && size > 20. {
            out.shape(Rect::new(r.x, y, r.w, 1.), 0., c(0xffffff, 0.10));
        }
        let label = if i == 0 {
            "Today"
        } else {
            w::WEEKDAYS[d.weekday as usize % 7]
        };
        let th = size * 1.4;
        out.text(
            label,
            Rect::new(r.x, mid - th / 2., label_w, th),
            style,
            WHITE,
            false,
        );
        let ix = r.x + label_w + 4.;
        out.icon(
            assets,
            condition_icon(weather::condition(synthetic(d.code, true, d.max)).icon),
            Rect::new(ix, mid - icon / 2., icon, icon),
            WHITE,
        );
        let low_x = ix + icon + 14.;
        out.text(
            deg(state, d.min),
            Rect::new(low_x, mid - th / 2., num_w, th),
            style.right(),
            c(0xffffff, 0.72),
            false,
        );
        let bar_x = low_x + num_w + 14.;
        let bar_w = r.x + r.w - num_w - 14. - bar_x;
        let bar = Rect::new(bar_x, mid - 3., bar_w.max(20.), 6.);
        out.shape(bar, 3., c(0xffffff, 0.18));
        let a = ((d.min - lo) / span) as f32;
        let b = ((d.max - lo) / span) as f32;
        let seg = Rect::new(bar.x + bar.w * a, bar.y, (bar.w * (b - a)).max(6.), 6.);
        out.draw
            .push(Draw::HGradient(seg, 3., c(0x9fe3f5, 1.), c(0xffd36b, 1.)));
        out.text(
            deg(state, d.max),
            Rect::new(r.x + r.w - num_w, mid - th / 2., num_w, th),
            style.right(),
            WHITE,
            false,
        );
    }
}

// ------------------------------------------------------------------ crypto

fn coin_disc(out: &mut Scene, assets: &Assets, symbol: &str, x: f32, y: f32, d: f32) {
    let (_, color, glyph) = w::coin(symbol);
    let r = Rect::new(x, y, d, d);
    out.shape(r, d / 2., c(color, 1.));
    // Never under the 18 px TV minimum, even in the 26 px text-line disc.
    let g = (d * 0.6).max(18.);
    let style = ts(assets, g, 600);
    out.text(
        glyph,
        Rect::new(x, y + (d - g * 1.36) / 2., d, g * 1.36),
        style,
        WHITE,
        true,
    );
}
fn pair_quote(prefs: &w::WidgetPrefs) -> &str {
    match (prefs.exchange, prefs.quote.as_str()) {
        (w::Exchange::Binance | w::Exchange::Bybit | w::Exchange::Okx, "USD") => "USDT",
        (_, q) => q,
    }
}
fn price(state: &State, value: f64) -> String {
    w::format_price(value, &state.preferences.widgets.quote)
}
/// A line through `series` (oldest first) with an optional soft area below.
/// The renderer has no line primitive: the line is a chain of small discs,
/// the area a run of 2 px vertical gradients.
pub(super) fn sparkline(
    out: &mut Scene,
    series: &[f64],
    r: Rect,
    color: u32,
    line: f32,
    area: bool,
) {
    if series.len() < 2 || r.w < 4. {
        return;
    }
    let lo = series.iter().copied().fold(f64::MAX, f64::min);
    let hi = series.iter().copied().fold(f64::MIN, f64::max);
    let span = (hi - lo).max(hi.abs() * 1e-6).max(1e-12);
    let value = |u: f32| {
        let p = u * (series.len() - 1) as f32;
        let i = (p.floor() as usize).min(series.len() - 2);
        let k = (p - i as f32) as f64;
        series[i] * (1. - k) + series[i + 1] * k
    };
    let y_of = |v: f64| r.y + r.h - ((v - lo) / span) as f32 * r.h;
    let steps = (r.w / 2.).ceil() as usize;
    if area {
        for k in 0..steps {
            let x = r.x + k as f32 * 2.;
            let y = y_of(value((k as f32 * 2. / r.w).min(1.)));
            out.draw.push(Draw::RoundedGradient(
                Rect::new(x, y, 2.4, r.y + r.h - y),
                0.,
                c(color, 0.26),
                c(color, 0.),
            ));
        }
    }
    let half = line / 2.;
    let mut prev: Option<(f32, f32)> = None;
    for k in 0..=steps {
        let u = (k as f32 * 2. / r.w).min(1.);
        let p = (r.x + u * r.w, y_of(value(u)));
        let from = prev.unwrap_or(p);
        let dist = ((p.0 - from.0).powi(2) + (p.1 - from.1).powi(2)).sqrt();
        let n = (dist / (line * 0.45)).ceil().max(1.) as usize;
        for j in 1..=n {
            let t = j as f32 / n as f32;
            let (x, y) = (from.0 + (p.0 - from.0) * t, from.1 + (p.1 - from.1) * t);
            out.shape(
                Rect::new(x - half, y - half, line, line),
                half,
                c(color, 1.),
            );
        }
        prev = Some(p);
    }
}
fn crypto_status(state: &State) -> &'static str {
    if state.widget_data.quotes_failed {
        "Prices unavailable"
    } else {
        "Loading prices"
    }
}
fn crypto_block(out: &mut Scene, state: &State, assets: &Assets, size: Size, r: Rect) {
    out.glass(r, RADIUS);
    let prefs = &state.preferences.widgets;
    let coins = prefs.ordered_coins();
    match size {
        Size::S => {
            let symbol = &coins[0];
            coin_disc(out, assets, symbol, r.x + 22., r.y + 22., 34.);
            out.text(
                symbol,
                Rect::new(r.x + 66., r.y + 24., 100., 30.),
                ts(assets, 22., 600),
                WHITE,
                false,
            );
            out.text(
                prefs.exchange.short(),
                Rect::new(r.x + r.w - 22. - 120., r.y + 26., 120., 26.),
                ts(assets, 18., 500).right(),
                c(0xffffff, 0.7),
                false,
            );
            let Some(q) = state.widget_data.quote(symbol) else {
                out.text(
                    crypto_status(state),
                    Rect::new(r.x + 22., r.y + 80., r.w - 44., 30.),
                    ts(assets, 20., 500),
                    c(0xffffff, 0.7),
                    false,
                );
                return;
            };
            let style = td(assets, 40.);
            out.text(
                clip_weather_line(&price(state, q.price), r.w - 44., style, assets),
                Rect::new(r.x + 22., r.y + 68., r.w - 44., 48.),
                style,
                WHITE,
                false,
            );
            out.text(
                format!(
                    "{} · {}",
                    w::format_change(q.change_pct),
                    prefs.period.label().to_lowercase()
                ),
                Rect::new(r.x + 22., r.y + 118., r.w - 44., 28.),
                ts(assets, 20., 500),
                trend(q.change_pct),
                false,
            );
            sparkline(
                out,
                &q.series,
                Rect::new(r.x + 22., r.y + r.h - 18. - 110., r.w - 44., 104.),
                if q.change_pct >= 0. { GREEN } else { RED },
                3.,
                true,
            );
        }
        Size::M => {
            let inner = Rect::new(r.x + 24., r.y + 18., r.w - 48., r.h - 36.);
            let row = 70.;
            let gap = (inner.h - 3. * row) / 2.;
            for (i, symbol) in coins.iter().take(3).enumerate() {
                let y = inner.y + i as f32 * (row + gap);
                if i > 0 {
                    out.shape(
                        Rect::new(inner.x, y - gap / 2., inner.w, 1.),
                        0.,
                        c(0xffffff, 0.08),
                    );
                }
                coin_row(
                    out,
                    state,
                    assets,
                    symbol,
                    Rect::new(inner.x, y, inner.w, row),
                    true,
                );
            }
        }
        _ => {
            let symbol = &coins[0];
            let left = Rect::new(r.x + 24., r.y + 22., 520., r.h - 44.);
            coin_disc(out, assets, symbol, left.x, left.y, 36.);
            let name = w::coin(symbol).0;
            let ns = ts(assets, 22., 600);
            let name = if name.is_empty() {
                symbol.as_str()
            } else {
                name
            };
            out.text(
                name,
                Rect::new(left.x + 48., left.y + 3., 200., 30.),
                ns,
                WHITE,
                false,
            );
            out.text(
                format!(
                    "{symbol}/{} · {}",
                    pair_quote(prefs),
                    prefs.exchange.short()
                ),
                Rect::new(
                    left.x + 48. + width(assets, name, ns) + 12.,
                    left.y + 6.,
                    300.,
                    26.,
                ),
                ts(assets, 18., 500),
                c(0xffffff, 0.7),
                false,
            );
            if let Some(q) = state.widget_data.quote(symbol) {
                let big = td(assets, 48.);
                let p = price(state, q.price);
                out.text(
                    &p,
                    Rect::new(left.x, left.y + 44., 360., 60.),
                    big,
                    WHITE,
                    false,
                );
                out.text(
                    format!(
                        "{} · {}",
                        w::format_change(q.change_pct),
                        prefs.period.label().to_lowercase()
                    ),
                    Rect::new(
                        left.x + width(assets, &p, big) + 14.,
                        left.y + 66.,
                        220.,
                        28.,
                    ),
                    ts(assets, 20., 500),
                    trend(q.change_pct),
                    false,
                );
                sparkline(
                    out,
                    &q.series,
                    Rect::new(left.x, left.y + 124., left.w, left.h - 128.),
                    if q.change_pct >= 0. { GREEN } else { RED },
                    3.,
                    true,
                );
            } else {
                out.text(
                    crypto_status(state),
                    Rect::new(left.x, left.y + 60., left.w, 30.),
                    ts(assets, 20., 500),
                    c(0xffffff, 0.7),
                    false,
                );
            }
            let x = left.x + left.w + 24.;
            out.shape(
                Rect::new(x, r.y + 22., 1., r.h - 44.),
                0.,
                c(0xffffff, 0.08),
            );
            let list = Rect::new(x + 24., r.y + 22., r.x + r.w - 24. - x - 24., r.h - 44.);
            let rest: Vec<&String> = coins.iter().skip(1).take(3).collect();
            let row = 56.;
            let gap = (list.h - 3. * row) / 2.;
            for (i, symbol) in rest.into_iter().enumerate() {
                let y = list.y + i as f32 * (row + gap);
                coin_row(
                    out,
                    state,
                    assets,
                    symbol,
                    Rect::new(list.x, y, list.w, row),
                    false,
                );
            }
        }
    }
}
/// Coin row of the M and L widgets: disc, symbol (and name), price and change.
fn coin_row(out: &mut Scene, state: &State, assets: &Assets, symbol: &str, r: Rect, full: bool) {
    let d = if full { 40. } else { 32. };
    let mid = r.y + r.h / 2.;
    coin_disc(out, assets, symbol, r.x, mid - d / 2., d);
    let x = r.x + d + if full { 14. } else { 12. };
    if full {
        out.text(
            symbol,
            Rect::new(x, mid - 29., 120., 30.),
            ts(assets, 22., 600),
            WHITE,
            false,
        );
        out.text(
            w::coin(symbol).0,
            Rect::new(x, mid + 1., 120., 25.),
            ts(assets, 18., 500),
            c(0xffffff, 0.65),
            false,
        );
    } else {
        out.text(
            symbol,
            Rect::new(x, mid - 15., 100., 30.),
            ts(assets, 22., 600),
            WHITE,
            false,
        );
    }
    let Some(q) = state.widget_data.quote(symbol) else {
        out.text(
            "—",
            Rect::new(r.x + r.w - 140., mid - 15., 140., 30.),
            ts(assets, 22., 600).right(),
            c(0xffffff, 0.5),
            false,
        );
        return;
    };
    let ps = ts(assets, if full { 24. } else { 22. }, 600);
    out.text(
        price(state, q.price),
        Rect::new(r.x + r.w - 160., mid - 30., 160., 32.),
        ps.right(),
        WHITE,
        false,
    );
    out.text(
        w::format_change(q.change_pct),
        Rect::new(r.x + r.w - 160., mid + 2., 160., 25.),
        ts(assets, 18., 500).right(),
        trend(q.change_pct),
        false,
    );
    if full {
        let spark = Rect::new(x + 130., mid - 18., r.w - (x + 130. - r.x) - 180., 36.);
        sparkline(
            out,
            &q.series,
            spark,
            if q.change_pct >= 0. { GREEN } else { RED },
            2.5,
            false,
        );
    }
}

// -------------------------------------------------------------------- HDMI

fn device_colors(icon: &str) -> (u32, u32) {
    match icon {
        "playstation" => (0x2a4cae, 0x1c2f6a),
        "computer" => (0x4f3496, 0x2f205e),
        "console" => (0x2f7d4a, 0x1d4b2e),
        "streamer" => (0x8a3b2c, 0x52231a),
        _ => (0x41464f, 0x2c3037),
    }
}
fn input_tile(
    out: &mut Scene,
    state: &State,
    assets: &Assets,
    input: &Input,
    r: Rect,
    radius: f32,
    big: bool,
) {
    let custom = state
        .preferences
        .input_names
        .get(&input.id)
        .map(String::as_str);
    let icon = input_icon(input, custom);
    let (a, b) = device_colors(icon);
    out.draw
        .push(Draw::RoundedGradient(r, radius, c(a, 1.), c(b, 1.)));
    let live = input_live(input);
    let d = if big { 132. } else { 104. };
    out.icon(
        assets,
        &format!("input-{icon}"),
        Rect::new(
            r.x + (r.w - d) / 2.,
            r.y + (r.h - d) / 2. - if big { 6. } else { 18. },
            d,
            d,
        ),
        c(0xffffff, if live { 1. } else { 0.55 }),
    );
    let name = input_name(state, input);
    let style = ts(assets, 26., 600);
    out.text(
        clip_weather_line(&name, r.w - 36., style, assets),
        Rect::new(r.x + 18., r.y + r.h - 16. - 36., r.w - 36., 36.),
        style,
        WHITE,
        false,
    );
    if big {
        // Port and signal chips, top left.
        let mut x = r.x + 26.;
        for label in [
            input.port.clone(),
            if live {
                "Signal".to_owned()
            } else {
                "No signal".to_owned()
            },
        ] {
            if label.is_empty() {
                continue;
            }
            let cs = ts(assets, 18., 500);
            let cw = width(assets, &label, cs) + 28.;
            let chip = Rect::new(x, r.y + 24., cw, 36.);
            out.shape(chip, 18., c(0x000000, 0.45));
            out.text(
                label,
                Rect::new(chip.x, chip.y + 5., chip.w, 26.),
                cs,
                WHITE,
                true,
            );
            x += cw + 8.;
        }
    }
}
/// Returns whether the whole block is one control (M) or tiles (L).
fn hdmi_block(
    out: &mut Scene,
    state: &State,
    assets: &Assets,
    index: usize,
    size: Size,
    r: Rect,
) -> bool {
    let inputs = hdmi_inputs(state);
    if size != Size::L {
        out.shadow(r, RADIUS, 24., 8., 0.16);
        match inputs.first() {
            Some(input) => input_tile(out, state, assets, input, r, RADIUS, true),
            None => out.glass(r, RADIUS),
        }
        out.border(r, RADIUS, 1., c(0xffffff, 0.08));
        return true;
    }
    out.glass(r, RADIUS);
    let pad = 16.;
    let gap = 14.;
    let tw = (r.w - 2. * pad - 2. * gap) / 3.;
    for (k, input) in inputs.iter().take(3).enumerate() {
        let t = Rect::new(
            r.x + pad + k as f32 * (tw + gap),
            r.y + pad,
            tw,
            r.h - 2. * pad,
        );
        let radius = w::inner_radius(RADIUS, pad);
        input_tile(out, state, assets, input, t, radius, false);
        if index != PREVIEW {
            let target = Target::WidgetTile(index, k);
            out.focus_ring(t, radius, state.focus_level(&target), state);
            out.control(target, t, true);
        }
    }
    false
}

// -------------------------------------------------------------- Smart Home

fn entity_icon(e: &Entity) -> &'static str {
    match e.domain.as_str() {
        "light" => "smart-light",
        "lock" => "smart-lock",
        "fan" => "smart-fan",
        "cover" => "smart-door",
        "sensor" | "climate" => "thermometer",
        "binary_sensor" => "eye",
        _ => "power",
    }
}
fn camera_picture(
    out: &mut Scene,
    state: &State,
    assets: &Assets,
    cam: Option<&Entity>,
    r: Rect,
    radius: f32,
    big: bool,
) {
    out.shape(r, radius, c(0x1b1f25, 1.));
    let snapshot = cam.and_then(|e| state.ha_snapshots.get(&e.id));
    if let Some(path) = snapshot {
        out.image(path.clone(), r, radius, false);
    } else {
        out.text(
            if cam.is_some() {
                "Waiting for the camera"
            } else {
                "No camera selected"
            },
            Rect::new(r.x + 16., r.y + r.h / 2. - 14., r.w - 32., 28.),
            ts(assets, 18., 500),
            c(0xffffff, 0.45),
            true,
        );
    }
    // LIVE chip.
    let inset = if big { 20. } else { 14. };
    let chip = Rect::new(r.x + inset, r.y + inset, 92., if big { 36. } else { 32. });
    out.shape(chip, chip.h / 2., c(0x181a1e, 0.6));
    out.shape(
        Rect::new(chip.x + 14., chip.y + chip.h / 2. - 5., 10., 10.),
        5.,
        c(0xff6b5e, 1.),
    );
    out.text(
        "LIVE",
        Rect::new(chip.x + 32., chip.y + (chip.h - 25.) / 2., 56., 25.),
        ts(assets, 18., 600),
        WHITE,
        false,
    );
    if let Some(e) = cam {
        if big {
            out.draw.push(Draw::RoundedGradient(
                Rect::new(r.x, r.y + r.h - 110., r.w, 110.),
                radius,
                c(0x000000, 0.),
                c(0x000000, 0.7),
            ));
        }
        let style = ts(assets, if big { 26. } else { 22. }, 600);
        let bottom = if big { 20. } else { 14. };
        let detail = if big && !e.area.is_empty() { 25. } else { 0. };
        lifted(
            out,
            &clip_weather_line(&e.name, r.w - 2. * inset, style, assets),
            Rect::new(
                r.x + inset + 4.,
                r.y + r.h - bottom - detail - style.size * 1.36,
                r.w - 2. * inset,
                style.size * 1.4,
            ),
            style,
            WHITE,
            false,
        );
        if detail > 0. {
            out.text(
                &e.area,
                Rect::new(
                    r.x + inset + 4.,
                    r.y + r.h - bottom - 25.,
                    r.w - 2. * inset,
                    25.,
                ),
                ts(assets, 18., 500),
                c(0xffffff, 0.75),
                false,
            );
        }
    }
}
fn device_tile(out: &mut Scene, state: &State, assets: &Assets, e: &Entity, r: Rect, radius: f32) {
    let on = e.on();
    // A device that is on is "chosen": accent outline; the fill is only focus.
    out.shape(r, radius, c(0xffffff, 0.10));
    choice(out, state, r, radius, on, 0.);
    let ink = WHITE;
    out.icon(
        assets,
        entity_icon(e),
        Rect::new(r.x + 16., r.y + 14., 30., 30.),
        ink,
    );
    let style = ts(assets, 18., 500);
    out.text(
        clip_weather_line(&e.name, r.w - 32., style, assets),
        Rect::new(r.x + 16., r.y + r.h - 14. - 28. - 25., r.w - 32., 25.),
        style,
        [ink[0], ink[1], ink[2], if on { 1. } else { 0.75 }],
        false,
    );
    out.text(
        smart_value(e),
        Rect::new(r.x + 16., r.y + r.h - 14. - 28., r.w - 32., 28.),
        ts(assets, 20., 600),
        ink,
        false,
    );
}
fn smart_block(
    out: &mut Scene,
    state: &State,
    assets: &Assets,
    index: usize,
    size: Size,
    r: Rect,
) -> bool {
    match size {
        Size::S => {
            out.glass(r, RADIUS);
            let Some(e) = smart_devices(state).first().copied() else {
                out.text(
                    "No devices yet",
                    Rect::new(r.x + 22., r.y + r.h / 2. - 14., r.w - 44., 28.),
                    ts(assets, 20., 500),
                    c(0xffffff, 0.7),
                    false,
                );
                return true;
            };
            let disc = Rect::new(r.x + 22., r.y + 22., 64., 64.);
            out.shape(disc, 32., c(0xffffff, 0.14));
            choice(out, state, disc, 32., e.on(), 0.);
            let ink = WHITE;
            out.icon(
                assets,
                entity_icon(e),
                Rect::new(disc.x + 16., disc.y + 16., 32., 32.),
                ink,
            );
            let x = r.x + 22.;
            let inner = r.w - 44.;
            out.text(
                &e.area,
                Rect::new(x, r.y + r.h - 22. - 28. - 36. - 25., inner, 25.),
                ts(assets, 18., 500),
                c(0xffffff, 0.7),
                false,
            );
            let ns = ts(assets, 26., 600);
            out.text(
                clip_weather_line(&e.name, inner, ns, assets),
                Rect::new(x, r.y + r.h - 22. - 28. - 36., inner, 36.),
                ns,
                WHITE,
                false,
            );
            out.text(
                smart_value(e),
                Rect::new(x, r.y + r.h - 22. - 28., inner, 28.),
                ts(assets, 20., 500),
                c(0xffffff, 0.9),
                false,
            );
            true
        }
        Size::M => {
            out.shadow(r, RADIUS, 24., 8., 0.16);
            camera_picture(out, state, assets, smart_camera(state), r, RADIUS, true);
            out.border(r, RADIUS, 1., c(0xffffff, 0.08));
            true
        }
        _ => {
            out.glass(r, RADIUS);
            let pad = 16.;
            let radius = w::inner_radius(RADIUS, pad);
            let cam = Rect::new(r.x + pad, r.y + pad, 400., r.h - 2. * pad);
            camera_picture(out, state, assets, smart_camera(state), cam, radius, false);
            if index != PREVIEW {
                let target = Target::Widget(index);
                out.focus_ring(cam, radius, state.focus_level(&target), state);
                out.control(target, cam, true);
            }
            let grid = Rect::new(
                cam.x + cam.w + 16.,
                r.y + pad,
                r.x + r.w - pad - cam.x - cam.w - 16.,
                r.h - 2. * pad,
            );
            let gap = 12.;
            let tw = (grid.w - gap) / 2.;
            let th = (grid.h - gap) / 2.;
            for (k, e) in smart_devices(state).into_iter().take(4).enumerate() {
                let t = Rect::new(
                    grid.x + (k % 2) as f32 * (tw + gap),
                    grid.y + (k / 2) as f32 * (th + gap),
                    tw,
                    th,
                );
                device_tile(out, state, assets, e, t, radius);
                if index != PREVIEW {
                    let target = Target::WidgetTile(index, k);
                    out.focus_ring(t, radius, state.focus_level(&target), state);
                    out.control(target, t, true);
                }
            }
            false
        }
    }
}

// --------------------------------------------------------------- text line

/// The parts of one text item: icon (or coin), text runs with colours.
fn text_item(state: &State, kind: Kind) -> (Option<String>, Option<String>, Vec<(String, Color)>) {
    match kind {
        Kind::Weather => {
            let (mood, reading) = weather_now(state);
            let Some(r) = reading else {
                return (
                    Some("weather-sun".into()),
                    None,
                    vec![("Weather".into(), WHITE)],
                );
            };
            let wind = forecast(state).map_or(r.wind_speed / 3.6, |f| f.wind_ms);
            (
                Some(hero_icon(mood, r).into()),
                None,
                vec![(
                    format!("{} · Wind {:.0} m/s", temp(state, r.temperature), wind),
                    WHITE,
                )],
            )
        }
        Kind::Crypto => {
            let symbol = state.preferences.widgets.main_coin.clone();
            match state.widget_data.quote(&symbol) {
                Some(q) => (
                    None,
                    Some(symbol),
                    vec![
                        (price(state, q.price), WHITE),
                        (
                            format!(
                                "{} {:.1}%",
                                if q.change_pct >= 0. { "▲" } else { "▼" },
                                q.change_pct.abs()
                            ),
                            trend(q.change_pct),
                        ),
                    ],
                ),
                None => (
                    None,
                    Some(symbol),
                    vec![(crypto_status(state).into(), c(0xffffff, 0.72))],
                ),
            }
        }
        Kind::Smart => {
            let lights = state
                .ha_entities
                .iter()
                .filter(|e| e.domain == "light" && e.on() && !e.hidden)
                .count();
            let mut line = smart_sensor(state)
                .map(|e| match e.state.parse::<f64>() {
                    Ok(v) if e.unit.contains('°') || e.device_class == "temperature" => {
                        format!("{} {v:.1}°", e.name)
                    }
                    _ => format!("{} {}", e.name, smart_value(e)),
                })
                .unwrap_or_default();
            if !line.is_empty() {
                line.push_str(" · ");
            }
            line.push_str(&match lights {
                0 => "Lights off".to_owned(),
                1 => "1 light on".to_owned(),
                n => format!("{n} lights on"),
            });
            (Some("thermometer".into()), None, vec![(line, WHITE)])
        }
        Kind::Hdmi => (None, None, vec![]),
    }
}
/// Width of a text item, for layout and hit boxes.
pub(super) fn text_item_width(assets: &Assets, state: &State, kind: Kind) -> f32 {
    let (icon, coin, runs) = text_item(state, kind);
    let style = ts(assets, 22., 500);
    let lead = if icon.is_some() || coin.is_some() {
        36.
    } else {
        0.
    };
    lead + runs
        .iter()
        .map(|(t, _)| width(assets, t, style))
        .sum::<f32>()
        + 10. * (runs.len().saturating_sub(1)) as f32
}
/// Text items that fit the line: (index, kind, content rect).
fn text_rects(state: &State, assets: &Assets, z: &Zone) -> Vec<(usize, Kind, Rect)> {
    let mut x = z.left;
    let mut out = Vec::new();
    for (i, kind) in &z.text {
        let wd = text_item_width(assets, state, *kind);
        if x + wd > crate::state::HOME_STRIP_RIGHT + 1. {
            break;
        }
        out.push((*i, *kind, Rect::new(x, z.top - 60., wd, 44.)));
        x += wd + 40.;
    }
    out
}
/// The pill a text item is focused and edited in: one shape for both.
fn text_pill(r: Rect) -> Rect {
    Rect::new(r.x - 14., r.y - 8., r.w + 28., r.h + 16.)
}
pub(super) fn text_line(out: &mut Scene, state: &State, assets: &Assets, z: &Zone) {
    let style = ts(assets, 22., 500);
    for (i, kind, r) in text_rects(state, assets, z) {
        let (x, y) = (r.x, r.y);
        let target = Target::WidgetText(i);
        let level = state.focus_level(&target);
        let pill = text_pill(r);
        if level > 0. {
            out.focus_ring(pill, pill.h / 2., level, state);
        }
        let (icon, coin, runs) = text_item(state, kind);
        let mut cx = x;
        if let Some(icon) = icon {
            out.icon(assets, &icon, Rect::new(cx, y + 9., 26., 26.), WHITE);
            cx += 36.;
        }
        if let Some(symbol) = coin {
            coin_disc(out, assets, &symbol, cx, y + 9., 26.);
            cx += 36.;
        }
        for (t, color) in runs {
            let tw = width(assets, &t, style);
            // Spare width: symbols such as ▲ may come from a fallback font.
            lifted(
                out,
                &t,
                Rect::new(cx, y + 7., tw + 40., 30.),
                style,
                color,
                false,
            );
            cx += tw + 10.;
        }
        out.control(target, pill, true);
    }
}

// --------------------------------------------------------------- edit mode

fn round_button(out: &mut Scene, state: &State, r: Rect, target: &Target) -> Color {
    let level = state.focus_level(target);
    out.shape(r, r.h / 2., c(0x3c3e44, 0.92));
    out.border(r, r.h / 2., 1., c(0xffffff, 0.18));
    if level > 0. {
        let at = out.draw.len();
        accent_fill(out, r, r.h / 2., state);
        motion::transform_group(&mut out.draw[at..], 0., 0., level);
    }
    blend(WHITE, accent_ink(state), level)
}
fn edit_chrome(out: &mut Scene, state: &State, assets: &Assets, z: &Zone) {
    let right = crate::state::HOME_STRIP_RIGHT;
    // Every widget, text or block, gets its own outline and its own «−» on
    // its top-left corner, so a button never sits closer to a neighbour.
    for (i, _, r) in text_rects(state, assets, z) {
        let pill = text_pill(r);
        out.border(pill, pill.h / 2., 1.5, c(0xffffff, 0.35));
        let b = Rect::new(pill.x - 16., pill.y - 16., 36., 36.);
        let target = Target::WidgetTextRemove(i);
        let ink = round_button(out, state, b, &target);
        out.shape(Rect::new(b.x + 11., b.y + 16.5, 14., 3.), 1.5, ink);
        out.control(target, b, true);
    }
    for (i, _, r) in &z.blocks {
        let lift = state.focus_level(&Target::Widget(*i));
        if lift > 0. {
            out.shadow(*r, RADIUS, 80., 34., 0.5 * lift);
        }
        let b = Rect::new(r.x - 18., r.y - 18., 44., 44.);
        let target = Target::WidgetRemove(*i);
        let ink = round_button(out, state, b, &target);
        out.shape(Rect::new(b.x + 14., b.y + 20.5, 16., 3.), 1.5, ink);
        out.control(target, b, true);
    }
    // Add widget and the hint, right-aligned under the blocks.
    let y = z.top + w::BLOCK_H + 22.;
    let hint = "Hold the wheel and drag  ·  Arrows move it  ·  Back finishes";
    let hs = ts(assets, 22., 500);
    let hw = width(assets, hint, hs) + 48.;
    let hint_r = Rect::new(right - hw, y, hw, 56.);
    out.glass(hint_r, 28.);
    out.text(hint, Rect::new(hint_r.x, y + 12., hw, 32.), hs, WHITE, true);
    let label = "Add widget";
    let ls = ts(assets, 22., 600);
    let aw = 18. + 20. + 8. + width(assets, label, ls) + 24.;
    let add = Rect::new(hint_r.x - 16. - aw, y, aw, 56.);
    let target = Target::WidgetAdd;
    let level = state.focus_level(&target);
    out.shape(add, 28., c(0xffffff, 0.14));
    if level > 0. {
        let at = out.draw.len();
        accent_fill(out, add, 28., state);
        motion::transform_group(&mut out.draw[at..], 0., 0., level);
    }
    let ink = blend(WHITE, accent_ink(state), level);
    out.shape(Rect::new(add.x + 18., add.y + 26.5, 20., 3.), 1.5, ink);
    out.shape(Rect::new(add.x + 26.5, add.y + 18., 3., 20.), 1.5, ink);
    out.text(
        label,
        Rect::new(add.x + 46., y + 12., aw - 46., 32.),
        ls,
        ink,
        false,
    );
    out.control(target, add, true);
}

/// Scale a composed group about (ox, oy): rects, radii, strokes, blurs and type.
pub(super) fn scale_group(draw: &mut [Draw], ox: f32, oy: f32, k: f32) {
    let sr = |r: &mut Rect| {
        r.x = ox + (r.x - ox) * k;
        r.y = oy + (r.y - oy) * k;
        r.w *= k;
        r.h *= k;
    };
    for d in draw {
        match d {
            Draw::Shape(r, radius, stroke, _)
            | Draw::DiagonalGradientBorder(r, radius, stroke, _, _) => {
                sr(r);
                *radius *= k;
                *stroke *= k;
            }
            Draw::RoundedGradient(r, radius, _, _)
            | Draw::DiagonalGradient(r, radius, _, _)
            | Draw::HGradient(r, radius, _, _)
            | Draw::Sheen(r, radius, _, _)
            | Draw::Image(_, r, radius, _)
            | Draw::TintedImage(_, r, radius, _, _) => {
                sr(r);
                *radius *= k;
            }
            Draw::Shadow(r, radius, blur, _) => {
                sr(r);
                *radius *= k;
                *blur *= k;
            }
            Draw::Text(_, r, style, _, _) => {
                sr(r);
                style.size *= k;
            }
            Draw::AppImage(_, r, _) => sr(r),
            Draw::Clip(Some(r)) => sr(r),
            Draw::RoundClip(Some((r, radius))) => {
                sr(r);
                *radius *= k;
            }
            _ => {}
        }
    }
}

// ---------------------------------------------------------- detail pages

pub(super) fn detail(out: &mut Scene, state: &State, assets: &Assets) {
    let Some(d) = state.widget_detail else {
        return;
    };
    out.controls.clear();
    out.draw.push(Draw::Clip(None));
    let start = out.draw.len();
    out.shape(Rect::new(0., 0., 1920., 1080.), 0., c(0x050608, 0.62));
    match d {
        Detail::Weather { tab } => weather_detail(out, state, assets, tab),
        Detail::Crypto { coin } => crypto_detail(out, state, assets, coin),
    }
    if let Some(opened) = state.widget_detail_started
        && !state.preferences.reduced_motion
    {
        let t = motion::progress(opened.elapsed().as_secs_f32(), motion::PANEL_IN);
        motion::transform_group(&mut out.draw[start..], 0., 16. * (1. - t), t);
    }
}
/// One filled element per screen: focus fills with the accent, the chosen
/// item without focus keeps only an accent outline (like `view::sub_tab`).
/// Returns the ink level: 1 on the fill, 0 on the outline.
pub(super) fn choice(
    out: &mut Scene,
    state: &State,
    r: Rect,
    radius: f32,
    selected: bool,
    focus: f32,
) -> f32 {
    if selected && focus < 1. {
        const W: f32 = 2.5;
        let (a, b) = state.accent_colors();
        let b = if state.preferences.accent.kind == "gradient" {
            b
        } else {
            a
        };
        let alpha = 1. - focus;
        out.draw.push(Draw::DiagonalGradientBorder(
            r.inset(W / 2.),
            radius - W / 2.,
            W,
            c(a, alpha),
            c(b, alpha),
        ));
    }
    if focus > 0. {
        let at = out.draw.len();
        accent_fill(out, r, radius, state);
        motion::transform_group(&mut out.draw[at..], 0., 0., focus);
    }
    focus
}
fn tabs(
    out: &mut Scene,
    state: &State,
    assets: &Assets,
    labels: &[&str],
    selected: usize,
    x: f32,
    y: f32,
) {
    let style = ts(assets, 22., 500);
    let mut x = x;
    for (i, label) in labels.iter().enumerate() {
        let wd = width(assets, label, style) + 60.;
        let r = Rect::new(x, y, wd, 64.);
        let target = Target::WidgetTab(i);
        out.shape(r, 32., c(0xffffff, 0.14));
        let on = choice(
            out,
            state,
            r,
            32.,
            i == selected,
            state.focus_level(&target),
        );
        out.text(
            *label,
            Rect::new(r.x, y + 16., wd, 32.),
            style,
            blend(WHITE, accent_ink(state), on),
            true,
        );
        out.control(target, r, false);
        x += wd + 16.;
    }
}
fn close(out: &mut Scene, state: &State, assets: &Assets, x: f32, y: f32) {
    let r = Rect::new(x, y, 64., 64.);
    movie_view::close_button(
        out,
        state,
        assets,
        r,
        "close",
        state.focus_level(&Target::WidgetClose),
    );
    out.control(Target::WidgetClose, r, false);
}
fn tile_head(out: &mut Scene, assets: &Assets, icon: &str, label: &str, x: f32, y: f32) {
    out.icon(
        assets,
        icon,
        Rect::new(x, y + 2., 20., 20.),
        c(0xffffff, 0.72),
    );
    out.text(
        label,
        Rect::new(x + 28., y, 400., 25.),
        ts(assets, 18., 500).tracking(0.03),
        c(0xffffff, 0.72),
        false,
    );
}
fn compass(deg: f64) -> &'static str {
    const NAMES: [&str; 8] = [
        "north",
        "northeast",
        "east",
        "southeast",
        "south",
        "southwest",
        "west",
        "northwest",
    ];
    NAMES[(((deg % 360. + 360.) % 360. + 22.5) / 45.) as usize % 8]
}
fn uv_word(uv: f64) -> &'static str {
    match uv {
        u if u < 3. => "Low",
        u if u < 6. => "Moderate",
        u if u < 8. => "High",
        u if u < 11. => "Very high",
        _ => "Extreme",
    }
}
fn weather_detail(out: &mut Scene, state: &State, assets: &Assets, tab: usize) {
    let (mood, reading) = weather_now(state);
    let panel = Rect::new(48., 48., 1824., 984.);
    out.shadow(panel, 40., 80., 30., 0.5);
    sky_plate(out, state, assets, panel, 40., mood);
    out.draw.push(Draw::RoundedGradient(
        Rect::new(panel.x, panel.y + panel.h * 0.3, panel.w, panel.h * 0.7),
        40.,
        c(0x0a0e18, 0.),
        c(0x0a0e18, 0.55),
    ));
    close(out, state, assets, 1744., 112.);
    tabs(
        out,
        state,
        assets,
        &["Today", "Tomorrow", "10 days"],
        tab,
        112.,
        420.,
    );
    let f = forecast(state);
    city_line(out, state, assets, 112., 104., 560., 24.);
    let Some(now) = reading else {
        out.text(
            "Forecast unavailable",
            Rect::new(112., 200., 560., 46.),
            ts(assets, 34., 500),
            WHITE,
            false,
        );
        return;
    };
    lifted(
        out,
        &temp(state, now.temperature),
        Rect::new(112., 140., 560., 170.),
        td(assets, 150.),
        WHITE,
        false,
    );
    lifted(
        out,
        &condition_text(state, now),
        Rect::new(112., 305., 560., 46.),
        ts(assets, 34., 500),
        WHITE,
        false,
    );
    let mut line = high_low(state).unwrap_or_default();
    if let Some(f) = f
        && f.feels_like.is_finite()
    {
        line = format!("{line}  ·  Feels like {}", temp(state, f.feels_like));
    }
    lifted(
        out,
        &line,
        Rect::new(112., 357., 560., 30.),
        ts(assets, 22., 500),
        c(0xffffff, 0.85),
        false,
    );

    let plate = |out: &mut Scene, r: Rect| {
        out.shape(r, RADIUS, c(0xffffff, 0.13));
        out.border(r, RADIUS, 1., c(0xffffff, 0.10));
    };
    // Hourly (or ten days) across the top right.
    let hourly = Rect::new(712., 196., 1096., 292.);
    plate(out, hourly);
    let Some(f) = f else {
        out.text(
            "Updating forecast",
            Rect::new(hourly.x + 28., hourly.y + 120., 600., 34.),
            ts(assets, 24., 500),
            c(0xffffff, 0.8),
            false,
        );
        return;
    };
    tile_head(
        out,
        assets,
        "clock",
        if tab == 2 {
            "10-DAY FORECAST"
        } else {
            "HOURLY FORECAST"
        },
        hourly.x + 28.,
        hourly.y + 20.,
    );
    let summary = {
        let rainy = f
            .hours
            .iter()
            .find(|h| h.day == if tab == 1 { 1 } else { 0 } && h.rain_chance >= 50.);
        match rainy {
            Some(h) => format!("Rain likely from {:02}:00", h.hour),
            None => "No rain expected".to_owned(),
        }
    };
    if tab != 2 {
        out.text(
            &summary,
            Rect::new(hourly.x + hourly.w - 28. - 600., hourly.y + 18., 600., 28.),
            ts(assets, 20., 500).right(),
            c(0xffffff, 0.8),
            false,
        );
    }
    // (label, icon, extra, value, highlighted)
    let cols: Vec<(String, &str, String, String, bool)> = match tab {
        2 => f
            .days
            .iter()
            .take(10)
            .enumerate()
            .map(|(i, d)| {
                (
                    if i == 0 {
                        "Today".into()
                    } else {
                        w::WEEKDAYS[d.weekday as usize % 7].into()
                    },
                    condition_icon(weather::condition(synthetic(d.code, true, d.max)).icon),
                    if d.rain_chance >= 20. {
                        format!("{:.0}%", d.rain_chance)
                    } else {
                        String::new()
                    },
                    deg(state, d.max),
                    i == 0,
                )
            })
            .collect(),
        _ => {
            let list: Vec<&w::Hour> = if tab == 1 {
                f.hours
                    .iter()
                    .filter(|h| h.day == 1 && h.hour >= 8)
                    .take(12)
                    .collect()
            } else {
                f.hours.iter().take(12).collect()
            };
            list.iter()
                .enumerate()
                .map(|(i, h)| {
                    (
                        if i == 0 && tab == 0 {
                            "Now".into()
                        } else {
                            format!("{:02}", h.hour)
                        },
                        condition_icon(
                            weather::condition(synthetic(h.code, h.is_day, h.temperature)).icon,
                        ),
                        if h.rain_chance >= 20. {
                            format!("{:.0}%", h.rain_chance)
                        } else {
                            String::new()
                        },
                        deg(state, h.temperature),
                        i == 0 && tab == 0,
                    )
                })
                .collect()
        }
    };
    let n = if tab == 2 { 10 } else { 12 };
    let grid = Rect::new(
        hourly.x + 28.,
        hourly.y + 60.,
        hourly.w - 56.,
        hourly.h - 80.,
    );
    let cw = grid.w / n as f32;
    for (i, (label, icon, extra, value, hi)) in cols.iter().enumerate() {
        let x = grid.x + i as f32 * cw;
        if *hi {
            out.shape(Rect::new(x, grid.y, cw, grid.h), 16., c(0xffffff, 0.14));
        }
        out.text(
            label,
            Rect::new(x, grid.y + 14., cw, 30.),
            ts(assets, 22., 500),
            c(0xffffff, if *hi { 1. } else { 0.78 }),
            true,
        );
        out.icon(
            assets,
            icon,
            Rect::new(x + cw / 2. - 18., grid.y + 62., 36., 36.),
            WHITE,
        );
        out.text(
            extra,
            Rect::new(x, grid.y + 102., cw, 25.),
            ts(assets, 18., 500),
            c(0x9fdcff, 1.),
            true,
        );
        out.text(
            value,
            Rect::new(x, grid.y + grid.h - 14. - 41., cw, 41.),
            ts(assets, 30., 600),
            WHITE,
            true,
        );
    }
    // Seven days, left.
    let week = Rect::new(112., 504., 560., 464.);
    plate(out, week);
    tile_head(
        out,
        assets,
        "list",
        "7-DAY FORECAST",
        week.x + 26.,
        week.y + 22.,
    );
    days(
        out,
        state,
        assets,
        &f.days,
        7,
        Rect::new(week.x + 26., week.y + 58., week.w - 52., 7. * 52.),
        DayScale {
            size: 22.,
            icon: 30.,
        },
    );
    // Six conditions tiles.
    let today = f.days.first();
    let tiles: [(&str, &str, String, String); 6] = [
        (
            "WIND",
            "weather-cloud",
            format!("{:.0} m/s", f.wind_ms),
            format!(
                "Gusts up to {:.0} m/s\nfrom the {}",
                f.gust_ms,
                compass(f.wind_dir)
            ),
        ),
        (
            "HUMIDITY",
            "weather-drizzle",
            format!("{:.0}%", f.humidity),
            if f.dew_point.is_finite() {
                format!("Dew point {}", temp(state, f.dew_point))
            } else {
                String::new()
            },
        ),
        (
            "UV INDEX",
            "weather-sun",
            format!("{:.0}", f.uv),
            uv_word(f.uv).to_owned(),
        ),
        (
            "SUNSET",
            "weather-moon",
            f.sunset.clone(),
            format!("Sunrise {}", f.sunrise),
        ),
        (
            "RAIN",
            "weather-rain",
            format!("{:.0} mm", today.map_or(0., |d| d.rain_mm)),
            format!("Chance today {:.0}%", today.map_or(0., |d| d.rain_chance)),
        ),
        (
            "PRESSURE",
            "clock",
            if f.pressure.is_finite() {
                format!("{:.0} hPa", f.pressure)
            } else {
                "—".into()
            },
            if f.visibility_km.is_finite() {
                format!("Visibility {:.0} km", f.visibility_km)
            } else {
                String::new()
            },
        ),
    ];
    for (k, (label, icon, value, sub)) in tiles.iter().enumerate() {
        let r = Rect::new(
            712. + (k % 3) as f32 * 373.,
            504. + (k / 3) as f32 * 242.,
            349.,
            222.,
        );
        plate(out, r);
        tile_head(out, assets, icon, label, r.x + 24., r.y + 22.);
        out.text(
            value,
            Rect::new(r.x + 24., r.y + 54., r.w - 48., 56.),
            td(assets, 44.),
            WHITE,
            false,
        );
        if k == 2 {
            let bar = Rect::new(r.x + 24., r.y + 122., r.w - 48., 8.);
            let stops = [0x6fdc8c, 0xf5e050, 0xffa04d, 0xff5d5d, 0xb46bff];
            let seg = bar.w / 4.;
            for s in 0..4 {
                let sr = Rect::new(
                    bar.x + s as f32 * seg,
                    bar.y,
                    seg + if s < 3 { 1. } else { 0. },
                    bar.h,
                );
                out.draw.push(Draw::HGradient(
                    sr,
                    if s == 0 || s == 3 { 4. } else { 0. },
                    c(stops[s], 1.),
                    c(stops[s + 1], 1.),
                ));
            }
            let mx = bar.x + bar.w * (f.uv / 11.).clamp(0., 1.) as f32;
            out.shape(
                Rect::new(mx - 10., bar.y - 6., 20., 20.),
                10.,
                c(0x000000, 0.25),
            );
            out.shape(Rect::new(mx - 7., bar.y - 3., 14., 14.), 7., WHITE);
        }
        let lines: Vec<&str> = sub.lines().collect();
        for (j, l) in lines.iter().enumerate() {
            let y = r.y + r.h - 22. - (lines.len() - j) as f32 * 27.;
            out.text(
                *l,
                Rect::new(r.x + 24., y, r.w - 48., 27.),
                ts(assets, 20., 500),
                c(0xffffff, 0.8),
                false,
            );
        }
    }
}
fn crypto_detail(out: &mut Scene, state: &State, assets: &Assets, coin: usize) {
    let prefs = &state.preferences.widgets;
    let coins = prefs.ordered_coins();
    let symbol = coins.get(coin).unwrap_or(&coins[0]).clone();
    let panel = Rect::new(48., 48., 1824., 984.);
    out.shadow(panel, 40., 80., 30., 0.5);
    out.glass(panel, 40.);
    close(out, state, assets, 1744., 96.);
    let period = period_index(prefs.period);
    tabs(
        out,
        state,
        assets,
        &["1H", "24H", "7D", "1Y"],
        period,
        1392.,
        196.,
    );
    coin_disc(out, assets, &symbol, 112., 104., 56.);
    let name = w::coin(&symbol).0;
    let name = if name.is_empty() {
        symbol.as_str()
    } else {
        name
    };
    let ns = ts(assets, 32., 600);
    out.text(name, Rect::new(184., 110., 400., 44.), ns, WHITE, false);
    out.text(
        format!(
            "{symbol} / {} · {}",
            pair_quote(prefs),
            prefs.exchange.short()
        ),
        Rect::new(184. + width(assets, name, ns) + 16., 118., 500., 30.),
        ts(assets, 22., 500),
        c(0xffffff, 0.65),
        false,
    );
    let chart = Rect::new(152., 320., 1080., 350.);
    if let Some(q) = state.widget_data.quote(&symbol) {
        let big = td(assets, 96.);
        let p = price(state, q.price);
        out.text(&p, Rect::new(112., 168., 1100., 110.), big, WHITE, false);
        let first = q.series.first().copied().unwrap_or(q.price);
        out.text(
            format!(
                "{} {} · {:.2}%",
                if q.change_pct >= 0. { "▲" } else { "▼" },
                price(state, (q.price - first).abs()),
                q.change_pct.abs()
            ),
            Rect::new(112. + width(assets, &p, big) + 22., 214., 520., 40.),
            ts(assets, 28., 500),
            trend(q.change_pct),
            false,
        );
        // Grid lines and labels, then the line.
        for k in 0..4 {
            let v = q.high - (q.high - q.low) * k as f64 / 3.;
            let y = chart.y + chart.h * k as f32 / 3.;
            out.shape(Rect::new(chart.x, y, chart.w, 1.), 0., c(0xffffff, 0.08));
            out.text(
                price(state, v),
                Rect::new(chart.x + chart.w + 20., y - 13., 160., 26.),
                ts(assets, 18., 500),
                c(0xffffff, 0.6),
                false,
            );
        }
        let marks = match prefs.period {
            w::Period::Hour => ["−60 min", "−45 min", "−30 min", "−15 min", "Now"],
            w::Period::Day => ["−24 h", "−18 h", "−12 h", "−6 h", "Now"],
            w::Period::Week => ["−7 d", "−5 d", "−3 d", "−1 d", "Now"],
            w::Period::Year => ["−12 mo", "−9 mo", "−6 mo", "−3 mo", "Now"],
        };
        for (k, m) in marks.iter().enumerate() {
            let x = chart.x + chart.w * k as f32 / 4.;
            out.text(
                *m,
                Rect::new(x - 60., chart.y + chart.h + 22., 120., 26.),
                ts(assets, 18., 500),
                c(0xffffff, 0.6),
                true,
            );
        }
        sparkline(
            out,
            &q.series,
            chart,
            if q.change_pct >= 0. { GREEN } else { RED },
            4.,
            true,
        );
        let label = prefs.period.label();
        let stats = [
            (format!("{label} HIGH"), price(state, q.high)),
            (format!("{label} LOW"), price(state, q.low)),
            (format!("{label} OPEN"), price(state, first)),
            ("PRICES FROM".to_owned(), prefs.exchange.short().to_owned()),
        ];
        for (k, (l, v)) in stats.iter().enumerate() {
            let r = Rect::new(112. + k as f32 * 274., 818., 250., 150.);
            out.shape(r, RADIUS, c(0xffffff, 0.07));
            out.border(r, RADIUS, 1., c(0xffffff, 0.08));
            out.text(
                l,
                Rect::new(r.x + 24., r.y + 22., r.w - 48., 25.),
                ts(assets, 18., 500).tracking(0.03),
                c(0xffffff, 0.65),
                false,
            );
            out.text(
                v,
                Rect::new(r.x + 24., r.y + 56., r.w - 48., 44.),
                ts(assets, 32., 600),
                WHITE,
                false,
            );
        }
    } else {
        out.text(
            crypto_status(state),
            Rect::new(112., 190., 1000., 60.),
            ts(assets, 40., 600),
            c(0xffffff, 0.8),
            false,
        );
    }
    for (k, s) in coins.iter().take(4).enumerate() {
        let r = Rect::new(1392., 300. + k as f32 * 112., 416., 96.);
        let target = Target::WidgetCoin(k);
        out.shape(r, 48., c(0xffffff, 0.07));
        let t = choice(out, state, r, 48., k == coin, state.focus_level(&target));
        let ink = blend(WHITE, accent_ink(state), t);
        coin_disc(out, assets, s, r.x + 18., r.y + 20., 56.);
        out.text(
            s,
            Rect::new(r.x + 90., r.y + 16., 140., 32.),
            ts(assets, 24., 600),
            ink,
            false,
        );
        out.text(
            w::coin(s).0,
            Rect::new(r.x + 90., r.y + 50., 160., 25.),
            ts(assets, 18., 500),
            [ink[0], ink[1], ink[2], 0.7],
            false,
        );
        if let Some(q) = state.widget_data.quote(s) {
            out.text(
                price(state, q.price),
                Rect::new(r.x + r.w - 28. - 180., r.y + 16., 180., 32.),
                ts(assets, 24., 600).right(),
                ink,
                false,
            );
            let up = q.change_pct >= 0.;
            let tc = blend(
                trend(q.change_pct),
                c(if up { 0x1d5a2f } else { 0x7a2a1a }, 1.),
                t,
            );
            out.text(
                w::format_change(q.change_pct),
                Rect::new(r.x + r.w - 28. - 180., r.y + 50., 180., 25.),
                ts(assets, 18., 500).right(),
                tc,
                false,
            );
        }
        out.control(target, r, false);
    }
    out.text(
        "Up and down switch the coin.\nPick coins in Settings › Widgets.",
        Rect::new(1392., 760., 416., 64.),
        ts(assets, 20., 500).leading(1.4),
        c(0xffffff, 0.65),
        false,
    );
}
fn period_index(p: w::Period) -> usize {
    w::Period::ALL.iter().position(|x| *x == p).unwrap_or(1)
}
