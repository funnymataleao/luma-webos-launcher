//! Title card popup (approved mockup "Movie card"): one padding of 88
//! everywhere, round buttons 84 high that size to their content, a close
//! button on the panel and a side panel for subtitles and quality.
use super::{Assets, Scene, WHITE, accent_fill, accent_ink, c};
use crate::motion;
use crate::renderer::{Color, Draw, TextStyle};
use crate::state::movie::{self, MovieCard};
use crate::state::{Rect, State, Target};

pub(super) const PAD: f32 = 88.;
/// One poster for every title page: movies, series (media, Jellyfin,
/// Plex) and games share its size and place. Sized so the
/// series' episode row still clears it.
pub(crate) const POSTER_W: f32 = 360.;
pub(crate) const POSTER_H: f32 = 540.;
pub(crate) const POSTER_RIGHT: f32 = 140.;
pub(crate) const POSTER_RADIUS: f32 = 30.;

/// Where the poster sits on a title page panel `p`, raised by `lift`.
pub(crate) fn poster_slot(p: Rect, lift: f32) -> Rect {
    Rect::new(
        p.x + p.w - POSTER_RIGHT - POSTER_W,
        p.y + PAD - lift,
        POSTER_W,
        POSTER_H,
    )
}
pub(super) const PANEL: Rect = Rect::new(72., 64., 1776., 952.);
pub(super) const RADIUS: f32 = 44.;
pub(super) const BUTTON_H: f32 = 84.;
/// One side padding in every state: focus changes the fill, never the size.
const BUTTON_PAD: f32 = 44.;
const SHADE: u32 = 0x090a0d;
const SIDE_W: f32 = 640.;

/// Fixed pixel size: undo the user's type scale, like heading_fixed.
pub(super) fn fixed(
    assets: &Assets,
    size: f32,
    weight: u16,
    heading: bool,
    line: f32,
) -> TextStyle {
    let pct = if heading {
        assets.heading_pct.get()
    } else {
        assets.text_pct.get()
    } as f32
        / 100.;
    let pct = pct.max(0.5);
    TextStyle::new(size / pct)
        .weight(weight)
        .display(heading)
        .leading(line * pct)
}
pub(super) fn body(assets: &Assets, size: f32, weight: u16) -> TextStyle {
    fixed(assets, size, weight, false, 1.25)
}

/// One text line centred vertically in `rect`.
pub(super) fn line_in(rect: Rect, size: f32) -> Rect {
    let h = size * 1.25;
    Rect::new(rect.x, rect.y + (rect.h - h) / 2., rect.w, h)
}

fn duration(minutes: i64) -> String {
    match (minutes / 60, minutes % 60) {
        (0, m) => format!("{m}m"),
        (h, 0) => format!("{h}h"),
        (h, m) => format!("{h}h {m}m"),
    }
}

/// A round button: optional icon, label, optional chevron; the width follows
/// the content only, so a row never shifts when focus moves.
pub(super) struct Button<'a> {
    pub icon: Option<&'a str>,
    pub label: &'a str,
    pub chevron: bool,
    pub primary: bool,
    pub size: f32,
}
fn button_width(assets: &Assets, b: &Button) -> f32 {
    let style = body(assets, b.size, if b.primary { 700 } else { 600 });
    let icon = if b.icon.is_some() { 26. + 12. } else { 0. };
    let label = if b.label.is_empty() {
        0.
    } else {
        assets.width(b.label, style).ceil()
    };
    let chevron = if b.chevron { 12. + 22. } else { 0. };
    let pad = BUTTON_PAD;
    if b.label.is_empty() {
        return BUTTON_H;
    }
    icon + label + chevron + 2. * pad
}
pub(super) fn button(
    out: &mut Scene,
    state: &State,
    assets: &Assets,
    b: &Button,
    x: f32,
    y: f32,
    level: f32,
) -> Rect {
    let w = button_width(assets, b);
    let rect = Rect::new(x, y, w, BUTTON_H);
    let r = BUTTON_H / 2.;
    let ink = two_state(out, state, rect, r, level);
    let style = body(assets, b.size, if b.primary { 700 } else { 600 });
    let mut cx = x + if b.label.is_empty() {
        (w - 28.) / 2.
    } else {
        BUTTON_PAD
    };
    if let Some(icon) = b.icon {
        let s = if b.label.is_empty() { 28. } else { 26. };
        out.icon(
            assets,
            icon,
            Rect::new(cx, y + (BUTTON_H - s) / 2., s, s),
            ink,
        );
        cx += s + 12.;
    }
    if !b.label.is_empty() {
        let lw = assets.width(b.label, style).ceil();
        out.text(
            b.label,
            line_in(Rect::new(cx, y, lw + 4., BUTTON_H), b.size),
            style,
            ink,
            false,
        );
        cx += lw + 12.;
    }
    if b.chevron {
        let mut faint = ink;
        faint[3] *= 0.6;
        out.icon(
            assets,
            "chevron-down",
            Rect::new(cx, y + (BUTTON_H - 22.) / 2., 22., 22.),
            faint,
        );
    }
    rect
}

/// Every control on the card has two states: grey at rest, the accent
/// gradient with focus. Returns the label colour.
pub(super) fn two_state(out: &mut Scene, state: &State, rect: Rect, r: f32, level: f32) -> Color {
    if level > 0. {
        out.shadow(rect, r, 40., 16., 0.4 * level);
    }
    out.shape(rect, r, c(0xffffff, 0.09));
    out.border(rect, r, 1., c(0xffffff, 0.11 * (1. - level)));
    if level > 0. {
        let at = out.draw.len();
        accent_fill(out, rect, r, state);
        motion::transform_group(&mut out.draw[at..], 0., 0., level);
    }
    mix(WHITE, accent_ink(state), level)
}

pub(super) fn mix(a: Color, b: Color, t: f32) -> Color {
    let t = t.clamp(0., 1.);
    [0, 1, 2, 3].map(|i| a[i] * (1. - t) + b[i] * t)
}
/// Horizontal multi-stop shade across `rect`: (position 0..1, alpha).
/// Each segment is drawn wider than its slot with extrapolated colours and
/// cut back by a scissor, so the anti-aliased quad edges never meet as a seam.
pub(super) fn h_stops(out: &mut Scene, rect: Rect, stops: &[(f32, f32)], restore: Rect) {
    for pair in stops.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        let x0 = (rect.x + rect.w * a.0).round();
        let x1 = (rect.x + rect.w * b.0).round();
        let w = x1 - x0;
        let slope = (b.1 - a.1) / w;
        out.draw
            .push(Draw::Clip(Some(Rect::new(x0, rect.y, w, rect.h))));
        out.draw.push(Draw::HGradient(
            Rect::new(x0 - 8., rect.y - 8., w + 16., rect.h + 16.),
            0.,
            c(SHADE, a.1 - 8. * slope),
            c(SHADE, b.1 + 8. * slope),
        ));
    }
    out.draw.push(Draw::Clip(Some(restore)));
}
/// Bottom-up shade: stops are (distance from the bottom as a share of the
/// height, alpha), bottom first; seamless like h_stops.
pub(super) fn bottom_shade(out: &mut Scene, rect: Rect, stops: &[(f32, f32)], restore: Rect) {
    for pair in stops.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        let top = (rect.y + rect.h * (1. - b.0)).round();
        let bottom = (rect.y + rect.h * (1. - a.0)).round();
        let h = bottom - top;
        let slope = (a.1 - b.1) / h;
        out.draw
            .push(Draw::Clip(Some(Rect::new(rect.x, top, rect.w, h))));
        out.draw.push(Draw::RoundedGradient(
            Rect::new(rect.x - 8., top - 8., rect.w + 16., h + 16.),
            0.,
            c(SHADE, b.1 - 8. * slope),
            c(SHADE, a.1 + 8. * slope),
        ));
    }
    out.draw.push(Draw::Clip(Some(restore)));
}

pub(super) fn close_button(
    out: &mut Scene,
    state: &State,
    assets: &Assets,
    rect: Rect,
    icon: &str,
    level: f32,
) {
    let ink = two_state(out, state, rect, rect.h / 2., level);
    let s = 24.;
    out.icon(
        assets,
        icon,
        Rect::new(rect.x + (rect.w - s) / 2., rect.y + (rect.h - s) / 2., s, s),
        ink,
    );
}

/// The service a title card came from, for the kicker.
fn source_name(id: i64) -> &'static str {
    if crate::jellyfin::owns(id) {
        "JELLYFIN"
    } else if crate::plex::owns(id) {
        "PLEX"
    } else {
        "MEDIA"
    }
}

fn kicker(
    out: &mut Scene,
    state: &State,
    assets: &Assets,
    kind: &str,
    source: &str,
    x: f32,
    y: f32,
) {
    let style = body(assets, 18., 600).tracking(0.08);
    let (accent, _) = state.accent_colors();
    let w = assets.width(kind, style).ceil();
    out.text(
        kind,
        Rect::new(x, y, w + 4., 23.),
        style,
        c(accent, 1.),
        false,
    );
    out.shape(
        Rect::new(x + w + 14., y + 9., 5., 5.),
        2.5,
        c(0xffffff, 0.4),
    );
    out.text(
        source,
        Rect::new(x + w + 33., y, 200., 23.),
        style,
        c(0xffffff, 0.55),
        false,
    );
}

/// IMDb / KP chips and quality tags; returns the right edge.
fn ratings(out: &mut Scene, assets: &Assets, card: &MovieCard, x: f32, y: f32, tags: bool) -> f32 {
    let Some(d) = &card.detail else {
        return x;
    };
    let value = body(assets, 21., 600);
    let mut x = x;
    let mut chip = |out: &mut Scene, badge: &str, fill: Option<u32>, ink: u32, score: f64| {
        let badge_style = body(assets, 17., 800);
        let bw = assets.width(badge, badge_style).ceil();
        let label = format!("{score:.1}");
        let lw = assets.width(&label, value).ceil();
        let badge_w = if fill.is_some() { bw + 14. } else { bw };
        let w = 14. + badge_w + 10. + lw + 14.;
        out.shape(Rect::new(x, y, w, 40.), 12., c(0xffffff, 0.08));
        let bx = x + 14.;
        if let Some(fill) = fill {
            out.shape(Rect::new(bx, y + 8., badge_w, 24.), 6., c(fill, 1.));
            out.text(
                badge,
                line_in(Rect::new(bx + 7., y, bw + 2., 40.), 17.),
                badge_style,
                c(ink, 1.),
                false,
            );
        } else {
            out.text(
                badge,
                line_in(Rect::new(bx, y, bw + 2., 40.), 17.),
                badge_style,
                c(ink, 1.),
                false,
            );
        }
        out.text(
            label,
            line_in(Rect::new(bx + badge_w + 10., y, lw + 4., 40.), 21.),
            value,
            WHITE,
            false,
        );
        x += w + 12.;
    };
    if let Some(score) = d.imdb {
        chip(out, "IMDb", Some(0xf5c518), 0x000000, score);
    }
    if let Some(score) = d.kinopoisk {
        chip(out, "КП", None, 0xff7a1a, score);
    }
    if tags {
        if d.imdb.is_some() || d.kinopoisk.is_some() {
            x += 10.;
        }
        let style = body(assets, 18., 700).tracking(0.04);
        for tag in &d.tags {
            let w = assets.width(tag, style).ceil() + 20.;
            let rect = Rect::new(x, y + 3., w, 34.);
            out.border(rect, 9., 1.5, c(0xffffff, 0.4));
            out.text(
                tag,
                line_in(Rect::new(x, y + 3., w, 34.), 18.),
                style,
                c(0xffffff, 0.85),
                true,
            );
            x += w + 12.;
        }
    }
    x
}

/// Title: one line at 96 when it fits, else up to two lines at 68.
/// Returns (height, two lines).
pub(super) fn title(
    out: &mut Scene,
    assets: &Assets,
    value: &str,
    x: f32,
    y: f32,
    width: f32,
) -> (f32, bool) {
    let big = fixed(assets, 96., 900, true, 1.08).tracking(-0.02);
    if assets.width(value, big) <= width {
        out.text(value, Rect::new(x, y, width, 104.), big, WHITE, false);
        return (104., false);
    }
    let small = fixed(assets, 68., 900, true, 1.06).tracking(-0.02);
    let lines = assets.lines(value, small, width).min(2) as f32;
    let h = 68. * 1.06 * lines;
    out.text(value, Rect::new(x, y, width, h + 2.), small, WHITE, false);
    (h, lines > 1.)
}

/// Entrance of one block, `delay` after the content started to come in:
/// 0 hidden, 1 in place. Nothing shows while the details are on their way,
/// so the stand-in stack title never flashes.
fn reveal(state: &State, card: &MovieCard, live: bool, delay: f32) -> f32 {
    if !live {
        return 1.;
    }
    entrance(state, card.detail_at, live, delay)
}
/// The title alone may come in before the details, as the stack named it.
fn reveal_title(state: &State, card: &MovieCard, live: bool) -> f32 {
    entrance(state, card.shown, live, 0.)
}
fn entrance(state: &State, from: Option<std::time::Instant>, live: bool, delay: f32) -> f32 {
    if !live {
        return 1.;
    }
    let Some(from) = from else {
        return 0.;
    };
    if state.preferences.reduced_motion {
        return 1.;
    }
    motion::progress(from.elapsed().as_secs_f32() - delay, movie::BLOCK_TIME)
}
/// Fades and slides everything drawn since `at` by the not-yet-done part.
pub(super) fn slide(out: &mut Scene, at: usize, k: f32, dx: f32, dy: f32) {
    if k < 1. {
        motion::transform_group(&mut out.draw[at..], dx * (1. - k), dy * (1. - k), k);
    }
}
const BLOCK_DY: f32 = 22.;
const CARD_DX: f32 = 160.;

fn meta_line(card: &MovieCard) -> String {
    let Some(d) = &card.detail else {
        return "Loading…".into();
    };
    let mut parts = vec![];
    if d.year > 0 {
        parts.push(d.year.to_string());
    }
    if d.serial {
        let n = d.seasons.len();
        parts.push(if n == 1 {
            "1 season".into()
        } else {
            format!("{n} seasons")
        });
    } else if d.minutes > 0 {
        parts.push(duration(d.minutes));
    }
    for s in [&d.genres, &d.countries] {
        if !s.is_empty() {
            parts.push(s.clone());
        }
    }
    parts.join("  ·  ")
}

/// Decode the focused media card's banner and poster ahead of OK: drawn
/// fully transparent, they are already on the GPU when the card opens.
pub fn warm(out: &mut Scene, state: &State) {
    let Target::StackCard(row, index) = state.focus else {
        return;
    };
    let Some(detail) = crate::stacks::row_stack(state, row)
        .and_then(|stack| state.stack_items.get(stack)?.get(index))
        .and_then(|item| state.movie_cache.get(&item.id))
    else {
        return;
    };
    for path in [&detail.backdrop, &detail.poster].into_iter().flatten() {
        out.draw.push(Draw::TintedImage(
            path.clone(),
            Rect::new(0., 0., 1., 1.),
            0.,
            false,
            [1., 1., 1., 0.],
        ));
    }
}

pub fn movie_card(out: &mut Scene, state: &State, assets: &Assets) {
    if let Some((card, closed)) = &state.movie_closing {
        let t = 1. - motion::progress(closed.elapsed().as_secs_f32(), movie::CLOSE_TIME);
        if t > 0. {
            draw_card(out, state, assets, card, false, t, 12. * (1. - t));
        }
    }
    let Some(card) = &state.movie else {
        return;
    };
    let t = if state.preferences.reduced_motion {
        1.
    } else {
        motion::progress(card.opened.elapsed().as_secs_f32(), movie::OPEN_TIME)
    };
    out.controls.clear();
    draw_card(out, state, assets, card, true, t, 24. * (1. - t));
}

fn draw_card(
    out: &mut Scene,
    state: &State,
    assets: &Assets,
    card: &MovieCard,
    live: bool,
    appear: f32,
    dy: f32,
) {
    let start = out.draw.len();
    let level = |t: Target| if live { state.focus_level(&t) } else { 0. };
    out.shape(Rect::new(0., 0., 1920., 1080.), 0., c(0x000000, 0.82));
    let p = Rect::new(PANEL.x, PANEL.y + dy, PANEL.w, PANEL.h);
    out.shadow(p, RADIUS, 140., 50., 0.65);
    // Until the banner is in, the panel is dark glass over the wallpaper,
    // never a stretched poster; the banner then fades in once.
    out.glass(p, RADIUS);
    out.shape(p, RADIUS, c(0x0b0c10, 0.6));
    out.draw.push(Draw::RoundClip(Some((p, RADIUS))));
    // Text is not masked by RoundClip; the scissor keeps lifted text inside.
    out.draw.push(Draw::Clip(Some(p)));
    let detail = card.detail.as_ref();
    let serial = card.serial();
    // Only a real wide backdrop fills the panel; a portrait poster stretched
    // to it looked broken while the details were still loading.
    let backdrop = detail.and_then(|d| d.backdrop.clone());
    if let Some(path) = backdrop {
        let at = out.draw.len();
        if let Some(under) = card.backdrop_under.clone() {
            out.image(under, p, RADIUS, false);
        }
        out.image(path, p, RADIUS, false);
        let k = match card.backdrop_at {
            Some(t) if live && !state.preferences.reduced_motion => {
                motion::progress(t.elapsed().as_secs_f32(), movie::BACKDROP_TIME)
            }
            _ => 1.,
        };
        slide(out, at, k, 0., 0.);
    }
    if serial {
        h_stops(
            out,
            p,
            &[(0., 0.97), (0.42, 0.88), (0.74, 0.3), (1., 0.55)],
            p,
        );
        bottom_shade(out, p, &[(0., 0.98), (0.34, 0.9), (0.6, 0.)], p);
    } else {
        h_stops(
            out,
            p,
            &[(0., 0.97), (0.4, 0.9), (0.72, 0.35), (1., 0.55)],
            p,
        );
        bottom_shade(out, p, &[(0., 0.96), (0.38, 0.)], p);
    }
    let lift = if serial { 0. } else { card.lift.0 };
    let left = p.x + PAD;
    let mut y = p.y + PAD - lift;
    let stage = |i: f32| reveal(state, card, live, i * movie::STAGGER);
    let at = out.draw.len();
    kicker(
        out,
        state,
        assets,
        if serial { "SERIES" } else { "MOVIE" },
        source_name(card.id),
        left,
        y,
    );
    slide(out, at, stage(0.), 0., BLOCK_DY);
    let at = out.draw.len();
    y += 23. + 16.;
    let text_w = if serial { 1040. } else { 1000. };
    let (h, two) = title(out, assets, &card.title, left, y, text_w);
    y += h + 16.;
    slide(out, at, reveal_title(state, card, live), 0., BLOCK_DY);
    let at = out.draw.len();
    let meta_style = body(assets, if serial { 25. } else { 26. }, 400);
    let meta = meta_line(card);
    let mw = assets.width(&meta, meta_style).ceil();
    out.text(
        &meta,
        Rect::new(left, y, mw.min(text_w) + 4., 34.),
        meta_style,
        c(0xffffff, 0.75),
        false,
    );
    if serial {
        ratings(
            out,
            assets,
            card,
            left + mw.min(text_w) + 22.,
            y - 3.,
            false,
        );
        y += 34. + 20.;
    } else {
        y += 34. + 20.;
        if detail.is_some_and(|d| d.imdb.is_some() || d.kinopoisk.is_some() || !d.tags.is_empty()) {
            ratings(out, assets, card, left, y, true);
            y += 40. + 24.;
        }
    }
    slide(out, at, stage(1.), 0., BLOCK_DY);
    let at = out.draw.len();
    if let Some(d) = detail.filter(|d| !d.plot.is_empty()) {
        let size = if serial { 26. } else { 27. };
        let lines = if serial || two { 2. } else { 3. };
        let style = fixed(assets, size, 400, false, 1.42);
        let used = (assets.lines(&d.plot, style, 960.) as f32).min(lines);
        out.text(
            &d.plot,
            Rect::new(left, y, 960., size * 1.42 * lines + 1.),
            style,
            c(0xffffff, 0.84),
            false,
        );
        y += size * 1.42 * used;
    }
    if !serial && let Some(d) = detail {
        y += 16.;
        let label = body(assets, 23., 400);
        for (name, value) in [("Director", &d.director), ("Starring", &d.cast)] {
            if value.is_empty() {
                continue;
            }
            let lw = assets.width(name, label).ceil();
            out.text(
                name,
                Rect::new(left, y, lw + 4., 30.),
                label,
                c(0xffffff, 0.42),
                false,
            );
            out.text(
                value,
                Rect::new(left + lw + 14., y, text_w - lw - 14., 30.),
                label,
                c(0xffffff, 0.62),
                false,
            );
            y += 36.;
        }
    }
    slide(out, at, stage(2.), 0., BLOCK_DY);
    let at = out.draw.len();
    // Buttons.
    let pills = card.pills();
    let labels: Vec<String> = pills
        .iter()
        .map(|&k| clip_label(assets, &card.pill_label(k)))
        .collect();
    let play_label = card.play_label();
    let trailer = detail.is_some_and(|d| d.trailer.is_some());
    // The buttons keep to the text block's width: left to
    // right, wrapping onto a new line only when the next one would not fit,
    // so nothing ever reaches under the poster.
    let limit = left + text_w;
    let row = |out: &mut Scene, y: f32, items: Vec<(Target, Button)>, gaps: &[f32]| -> f32 {
        let mut x = left;
        let mut y = y;
        for (i, (target, b)) in items.into_iter().enumerate() {
            let before = out.draw.len();
            let mut rect = button(out, state, assets, &b, x, y, level(target.clone()));
            if rect.x + rect.w > limit && x > left {
                out.draw.truncate(before);
                x = left;
                y += BUTTON_H + 18.;
                rect = button(out, state, assets, &b, x, y, level(target.clone()));
            }
            if live {
                out.control(target, rect, false);
            }
            x += rect.w + gaps.get(i).copied().unwrap_or(14.);
        }
        y
    };
    let play = Button {
        icon: Some("play"),
        label: &play_label,
        chevron: false,
        primary: true,
        size: 27.,
    };
    if serial {
        y += 30.;
        let mut items = vec![(Target::MoviePlay, play)];
        // A round icon button: the series row has no room for a label.
        if trailer {
            let round = Button {
                label: "",
                ..trailer_button()
            };
            items.push((Target::MovieTrailer, round));
        }
        for (&k, label) in pills.iter().zip(&labels) {
            items.push((Target::MoviePill(k), pill(k, label)));
        }
        row(out, y, items, &[14., 22.]);
    } else {
        // One row, like the series card: Play, Trailer, then the pickers
        // (a lone picker stacked over Play looked broken).
        y += 34.;
        let mut items = vec![(Target::MoviePlay, play)];
        if trailer {
            items.push((Target::MovieTrailer, trailer_button()));
        }
        for (&k, label) in pills.iter().zip(&labels) {
            items.push((Target::MoviePill(k), pill(k, label)));
        }
        row(out, y, items, &[]);
    }
    slide(out, at, stage(3.), 0., BLOCK_DY);
    // Poster.
    let at = out.draw.len();
    let poster = detail
        .and_then(|d| d.poster.clone())
        .or_else(|| card.poster.clone());
    let poster_rect = poster_slot(p, lift);
    out.shadow(poster_rect, 30., 100., 40., 0.65);
    let corner = POSTER_RADIUS;
    // The stack's small poster stays underneath until the big one has
    // decoded and faded in over it, so the slot is never empty.
    if let Some(small) = card
        .poster
        .clone()
        .filter(|small| poster.as_ref() != Some(small))
    {
        out.image(small, poster_rect, corner, false);
    }
    match poster {
        Some(path) => out.image(path, poster_rect, corner, false),
        None => out.shape(poster_rect, 30., c(0x23272e, 1.)),
    }
    slide(out, at, stage(1.), 0., 0.);
    if serial {
        seasons_and_episodes(out, state, assets, card, p, live);
    } else {
        similar(out, state, assets, card, p, lift, live);
    }
    out.draw.push(Draw::Clip(None));
    out.draw.push(Draw::RoundClip(None));
    // Close.
    let close = Rect::new(p.x + p.w - 24. - 60., p.y + 24., 60., 60.);
    close_button(
        out,
        state,
        assets,
        close,
        "close",
        level(Target::MovieClose),
    );
    if live {
        out.control(Target::MovieClose, close, false);
        out.modal = Some(p);
    }
    if let Some((kind, opened)) = card.panel {
        let t = if state.preferences.reduced_motion {
            1.
        } else {
            motion::progress(opened.elapsed().as_secs_f32(), movie::PANEL_TIME)
        };
        if live {
            // Only the side panel reacts while it is open.
            out.controls.clear();
        }
        side_panel(out, state, assets, card, kind, p, t, live);
    }
    border(out, p);
    if appear < 1. {
        motion::transform_group(&mut out.draw[start..], 0., 0., appear);
    }
}

fn border(out: &mut Scene, p: Rect) {
    out.border(p, RADIUS, 1., c(0xffffff, 0.08));
}

/// Pill labels stay on one line; very long track names are shortened.
fn clip_label(assets: &Assets, value: &str) -> String {
    let style = body(assets, 25., 600);
    if assets.width(value, style) <= 300. {
        return value.to_owned();
    }
    let mut out = String::new();
    for ch in value.chars() {
        out.push(ch);
        if assets.width(&format!("{out}…"), style) > 300. {
            out.pop();
            break;
        }
    }
    format!("{}…", out.trim_end())
}

fn similar(
    out: &mut Scene,
    state: &State,
    assets: &Assets,
    card: &MovieCard,
    p: Rect,
    lift: f32,
    live: bool,
) {
    let items = card.similar();
    if items.is_empty() {
        return;
    }
    let left = p.x + PAD;
    let top = p.y + 842. - lift;
    let at = out.draw.len();
    out.text(
        "More like this",
        Rect::new(left, top, 600., 36.),
        body(assets, 28., 700),
        WHITE,
        false,
    );
    slide(
        out,
        at,
        reveal(state, card, live, movie::ROW_DELAY),
        0.,
        BLOCK_DY,
    );
    let first = (card.similar_x.0 / movie::SIMILAR_PITCH).floor().max(0.) as usize;
    let y = top + 50.;
    // The same cards as the Home stack rows: size, zoom, shadow, light sweep.
    let (w, h) = crate::stacks::CardShape::Poster.size();
    let radius = super::TILE_RADIUS;
    out.draw
        .push(Draw::Clip(Some(Rect::new(p.x, p.y, p.w, p.h))));
    for (i, item) in items.iter().enumerate() {
        let x = left + i as f32 * movie::SIMILAR_PITCH - card.similar_x.0;
        if x > p.x + p.w || x + w < p.x {
            continue;
        }
        let rect = Rect::new(x, y, w, h);
        let target = Target::MovieSimilar(i);
        let f = if live { state.focus_level(&target) } else { 0. };
        // Exactly the Home stack card: the focused poster grows around its
        // centre (the row itself never shifts) and wears the accent ring.
        // The light sweep waits for the row to rest: running it on every card
        // the pointer passes while the row slides cost frames on the TV.
        let resting = (card.similar_x.0 - card.similar_x.1).abs() < 0.5;
        let (pop, sheen) = if live {
            state.focus_pop(&target)
        } else {
            (0., None)
        };
        let sheen = sheen.filter(|_| resting);
        let zoom = 1. + 0.06 * pop;
        let r = Rect::new(
            rect.x - rect.w * (zoom - 1.) / 2.,
            rect.y - rect.h * (zoom - 1.) / 2.,
            rect.w * zoom,
            rect.h * zoom,
        );
        let at = out.draw.len();
        let rank = i.saturating_sub(first) as f32;
        out.shadow(r, radius, 26. + 14. * f, 8. + 4. * f, 0.16 + 0.1 * f);
        out.accent_glow(r, radius, f, state);
        match &item.poster {
            Some(path) => out
                .draw
                .push(Draw::TintedImage(path.clone(), r, radius, false, [1.; 4])),
            None => {
                out.shape(r, radius, c(0x3a3d44, 1.));
                out.text(
                    &item.title,
                    Rect::new(r.x + 20., r.y + 20., r.w - 40., 120.),
                    body(assets, 23., 600),
                    WHITE,
                    false,
                );
            }
        }
        out.border(r, radius, 1., c(0xffffff, 0.06));
        out.focus_ring(r, radius, f, state);
        if let Some(progress) = sheen {
            out.draw
                .push(Draw::Sheen(r, radius, progress, c(0xffffff, 0.14)));
        }
        let k = reveal(
            state,
            card,
            live,
            movie::ROW_DELAY + 0.04 + rank * movie::CARD_STAGGER,
        );
        slide(out, at, k, CARD_DX, 0.);
        if live {
            out.control(target, rect, false);
        }
    }
    out.draw.push(Draw::Clip(Some(p)));
}

fn seasons_and_episodes(
    out: &mut Scene,
    state: &State,
    assets: &Assets,
    card: &MovieCard,
    p: Rect,
    live: bool,
) {
    let Some(d) = &card.detail else {
        return;
    };
    let left = p.x + PAD;
    // Season tabs.
    let tab_style = body(assets, 24., 600);
    let labels: Vec<String> = d.seasons.iter().map(|s| format!("Season {s}")).collect();
    let widths: Vec<f32> = labels
        .iter()
        .map(|l| assets.width(l, tab_style).ceil() + 52.)
        .collect();
    let total = widths.iter().sum::<f32>() + 6. * (widths.len().saturating_sub(1)) as f32 + 12.;
    let bar = Rect::new(left, p.y + 584., total.min(p.w - 2. * PAD), 68.);
    let tabs_at = out.draw.len();
    out.shape(bar, 34., c(0xffffff, 0.06));
    let mut x = bar.x + 6.;
    for (i, (label, w)) in labels.iter().zip(&widths).enumerate() {
        let rect = Rect::new(x, bar.y + 6., *w, 56.);
        let target = Target::MovieSeason(i);
        let focus = if live { state.focus_level(&target) } else { 0. };
        let ink = if focus > 0.5 {
            accent_ink(state)
        } else if i == card.season {
            WHITE
        } else {
            c(0xffffff, 0.62)
        };
        if i == card.season && focus < 0.5 {
            super::accent_outline(out, state, rect, 28.);
        }
        if focus > 0. {
            let at = out.draw.len();
            accent_fill(out, rect, 28., state);
            motion::transform_group(&mut out.draw[at..], 0., 0., focus);
        }
        out.text(label, line_in(rect, 24.), tab_style, ink, true);
        if live {
            out.control(target, rect, false);
        }
        x += w + 6.;
    }
    slide(
        out,
        tabs_at,
        reveal(state, card, live, movie::ROW_DELAY),
        0.,
        BLOCK_DY,
    );
    // Episodes of the shown season.
    let top = p.y + 680.;
    let first = (card.episodes_x.0 / movie::EPISODE_PITCH).floor().max(0.) as usize;
    out.draw.push(Draw::Clip(Some(Rect::new(
        left - 8.,
        top - 30.,
        p.w - PAD + 8.,
        p.h,
    ))));
    let title_style = body(assets, 23., 600);
    let meta_style = body(assets, 19., 400);
    for (pos, index) in card.season_episodes().into_iter().enumerate() {
        let e = &d.episodes[index];
        let x = left + pos as f32 * movie::EPISODE_PITCH - card.episodes_x.0;
        if x > p.x + p.w || x + 352. < p.x {
            continue;
        }
        let thumb = Rect::new(x, top, 352., 198.);
        let at = out.draw.len();
        let rank = pos.saturating_sub(first) as f32;
        let target = Target::MovieEpisode(index);
        let level = if live { state.focus_level(&target) } else { 0. };
        // Focus like a Home stack card: the still grows around its centre and
        // wears the accent ring (no flat yellow frame).
        let pop = if live { state.focus_pop(&target).0 } else { 0. };
        let zoom = 1. + 0.06 * pop;
        let z = Rect::new(
            thumb.x - thumb.w * (zoom - 1.) / 2.,
            thumb.y - thumb.h * (zoom - 1.) / 2.,
            thumb.w * zoom,
            thumb.h * zoom,
        );
        out.shadow(z, 20., 34., 14., 0.45);
        out.accent_glow(z, 20., level, state);
        match &e.thumb {
            Some(path) => out.image(path.clone(), z, 20., false),
            None => out.shape(z, 20., c(0x23272e, 1.)),
        }
        let bar = Rect::new(
            z.x + 16. * zoom,
            z.y + z.h - 20. * zoom,
            z.w - 32. * zoom,
            6.,
        );
        if e.watched {
            out.shape(bar, 3., c(0xffffff, 0.9));
        } else if e.progress > 0.01 {
            out.shape(bar, 3., c(0xffffff, 0.25));
            let (a, b) = state.accent_colors();
            out.draw.push(Draw::HGradient(
                Rect::new(bar.x, bar.y, (bar.w * e.progress).max(6.), 6.),
                3.,
                c(a, 1.),
                c(b, 1.),
            ));
        }
        out.border(z, 20., 1., c(0xffffff, 0.06));
        out.focus_ring(z, 20., level, state);
        let name = if e.title.is_empty() {
            format!("Episode {}", e.number)
        } else {
            format!("{} · {}", e.number, e.title)
        };
        out.text(
            name,
            Rect::new(x, top + 198. + 12., 352., 30.),
            title_style,
            WHITE,
            false,
        );
        let meta = if e.watched {
            format!("Watched · {}", duration(e.minutes))
        } else if e.progress > 0.01 {
            format!(
                "{} left",
                duration(((1. - e.progress) * e.minutes as f32).round() as i64)
            )
        } else if e.minutes > 0 {
            duration(e.minutes)
        } else {
            String::new()
        };
        out.text(
            meta,
            Rect::new(x, top + 198. + 46., 352., 26.),
            meta_style,
            c(0xffffff, 0.5),
            false,
        );
        let k = reveal(
            state,
            card,
            live,
            movie::ROW_DELAY + 0.04 + rank * movie::CARD_STAGGER,
        );
        slide(out, at, k, CARD_DX, 0.);
        if live {
            out.control(target, thumb, false);
        }
    }
    out.draw.push(Draw::Clip(Some(p)));
}

#[allow(clippy::too_many_arguments)]
fn side_panel(
    out: &mut Scene,
    state: &State,
    assets: &Assets,
    card: &MovieCard,
    kind: usize,
    p: Rect,
    t: f32,
    live: bool,
) {
    let start = out.draw.len();
    // The card behind steps back.
    out.shape(p, RADIUS, c(0x000000, 0.5 * t));
    let x = p.x + p.w - SIDE_W + 60. * (1. - t);
    let aside = Rect::new(x, p.y, SIDE_W, p.h);
    out.draw.push(Draw::RoundClip(Some((p, RADIUS))));
    out.shadow(aside, 0., 90., 0., 0.45);
    out.shape(aside, 0., c(0x111318, 0.97));
    out.shape(Rect::new(x, p.y, 1., p.h), 0., c(0xffffff, 0.08));
    let level = |t: Target| if live { state.focus_level(&t) } else { 0. };
    let back = Rect::new(x + 48., p.y + 24., 60., 60.);
    close_button(
        out,
        state,
        assets,
        back,
        "chevron-left",
        level(Target::MoviePanelBack),
    );
    if live {
        out.control(Target::MoviePanelBack, back, false);
    }
    let heading = match kind {
        movie::AUDIO => "Audio",
        movie::SUBTITLES => "Subtitles",
        _ => "Quality",
    };
    out.text(
        heading,
        line_in(Rect::new(back.x + 60. + 18., back.y - 4., 320., 68.), 44.),
        fixed(assets, 44., 900, true, 1.25),
        WHITE,
        false,
    );
    let options = card.options(kind);
    let note = match kind {
        movie::AUDIO => "Switch in the TV player".to_owned(),
        movie::SUBTITLES => format!("{} tracks", options.len().saturating_sub(1)),
        _ => String::new(),
    };
    let mut note_style = body(assets, 20., 500);
    note_style.align_right = true;
    out.text(
        note,
        line_in(Rect::new(x + SIDE_W - 48. - 260., back.y, 260., 60.), 20.),
        note_style,
        c(0xffffff, 0.45),
        false,
    );
    let list_top = p.y + 120.;
    let list = Rect::new(x, list_top, SIDE_W, p.y + p.h - list_top);
    out.draw.push(Draw::Clip(Some(list)));
    let chosen = card.chosen(kind);
    let row_style = body(assets, 26., 500);
    let row_focus_style = body(assets, 26., 700);
    for (i, label) in options.iter().enumerate() {
        let y = list_top + 8. + i as f32 * movie::OPTION_PITCH - card.options_y.0;
        if y + 68. < list_top || y > p.y + p.h {
            continue;
        }
        let rect = Rect::new(x + 24., y, SIDE_W - 48., 68.);
        let target = Target::MovieOption(i);
        let focus = level(target.clone());
        let ink = if focus > 0.5 {
            accent_ink(state)
        } else {
            c(0xffffff, 0.9)
        };
        let picked = i == chosen && kind != movie::AUDIO || kind == movie::AUDIO && i == 0;
        if picked && focus < 0.5 {
            super::accent_outline(out, state, rect, 18.);
        }
        if focus > 0. {
            out.shadow(rect, 18., 30., 12., 0.35 * focus);
            let at = out.draw.len();
            accent_fill(out, rect, 18., state);
            motion::transform_group(&mut out.draw[at..], 0., 0., focus);
        }
        let style = if focus > 0.5 {
            row_focus_style
        } else {
            row_style
        };
        out.text(
            label,
            line_in(Rect::new(rect.x + 24., y, rect.w - 90., 68.), 26.),
            style,
            ink,
            false,
        );
        if picked {
            out.icon(
                assets,
                "check",
                Rect::new(rect.x + rect.w - 24. - 28., y + 20., 28., 28.),
                ink,
            );
        }
        if live {
            out.control(target, rect, false);
        }
    }
    out.draw.push(Draw::Clip(None));
    // Fade the list into the panel at the bottom.
    let fade = Rect::new(x, p.y + p.h - 120., SIDE_W, 120.);
    out.draw.push(Draw::RoundedGradient(
        fade,
        0.,
        c(0x111318, 0.),
        c(0x111318, 1.),
    ));
    out.draw.push(Draw::RoundClip(None));
    if t < 1. {
        motion::transform_group(&mut out.draw[start..], 0., 0., t);
    }
    if live {
        out.modal = Some(aside);
    }
}

fn pill(kind: usize, label: &str) -> Button<'_> {
    Button {
        icon: Some(match kind {
            movie::AUDIO => "volume",
            movie::SUBTITLES => "captions",
            _ => "quality",
        }),
        label,
        chevron: true,
        primary: false,
        size: 25.,
    }
}

fn trailer_button() -> Button<'static> {
    Button {
        icon: Some("trailer"),
        label: "Trailer",
        chevron: false,
        primary: false,
        size: 27.,
    }
}
