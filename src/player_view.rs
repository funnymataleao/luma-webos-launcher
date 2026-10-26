//! Full-screen cinema controls (approved mockups Player / PlayerTracks /
//! PlayerChapters): one padding of 88, right edge at 1832, round buttons 84
//! high that share the movie card's two states, alpha gradients only.
use super::movie_view::{body, bottom_shade, close_button, fixed, line_in, two_state};
use super::{Assets, Scene, accent_fill, accent_ink, c};
use crate::player::{self, Panel, Session, Source};
use crate::renderer::{Color, Draw, TextStyle, WHITE};
use crate::state::{Rect, State, Target};

const PAD: f32 = 88.;
const RIGHT: f32 = 1920. - PAD;
const WIDE: f32 = RIGHT - PAD;
const SCREEN: Rect = Rect::new(0., 0., 1920., 1080.);
const BAR_Y: f32 = 846.;
const ROW_Y: f32 = 928.;
const BUTTON_H: f32 = 84.;
const BUTTON_PAD: f32 = 40.;
const ICON: f32 = 26.;
const GAP: f32 = 14.;
const PANEL_BG: u32 = 0x111318;

fn faint(alpha: f32) -> Color {
    c(0xffffff, alpha)
}
fn plain(style: TextStyle) -> TextStyle {
    let mut style = style;
    style.tabular_nums = false;
    style
}
fn focused(state: &State, target: Target) -> f32 {
    if state.focus == target { 1. } else { 0. }
}

/// A round button: icon before or after the label, sized to its content.
struct Button<'a> {
    icon: &'a str,
    label: &'a str,
    trailing: bool,
}
fn button_width(assets: &Assets, b: &Button) -> f32 {
    ICON + 12. + assets.width(b.label, body(assets, 25., 600)).ceil() + 2. * BUTTON_PAD
}
fn button(out: &mut Scene, state: &State, assets: &Assets, b: &Button, x: f32, level: f32) -> Rect {
    let rect = Rect::new(x, ROW_Y, button_width(assets, b), BUTTON_H);
    let ink = two_state(out, state, rect, BUTTON_H / 2., level);
    let style = body(assets, 25., 600);
    let lw = assets.width(b.label, style).ceil();
    let icon_y = ROW_Y + (BUTTON_H - ICON) / 2.;
    let mut cx = x + BUTTON_PAD;
    if !b.trailing {
        out.icon(assets, b.icon, Rect::new(cx, icon_y, ICON, ICON), ink);
        cx += ICON + 12.;
    }
    out.text(
        b.label,
        line_in(Rect::new(cx, ROW_Y, lw + 4., BUTTON_H), 25.),
        style,
        ink,
        false,
    );
    if b.trailing {
        out.icon(
            assets,
            b.icon,
            Rect::new(cx + lw + 12., icon_y, ICON, ICON),
            ink,
        );
    }
    rect
}

/// Uppercase source kicker: accent kind, dot, service.
fn kicker(out: &mut Scene, state: &State, assets: &Assets, source: Source) {
    let (kind, service) = match source {
        Source::Movie => ("MOVIE", "MEDIA"),
        Source::Series => ("SERIES", "MEDIA"),
        Source::Youtube => ("VIDEO", "YOUTUBE"),
        Source::Local => ("VIDEO", "LOCAL FILE"),
    };
    let style = plain(body(assets, 18., 600).tracking(0.08));
    let w = assets.width(kind, style).ceil();
    out.text(
        kind,
        Rect::new(PAD, 72., w + 4., 23.),
        style,
        accent_at(state, 0.55),
        false,
    );
    out.shape(Rect::new(PAD + w + 14., 81., 5., 5.), 2.5, faint(0.5));
    out.text(
        service,
        Rect::new(PAD + w + 33., 72., 400., 23.),
        style,
        faint(0.6),
        false,
    );
}

/// Title in the brand heading font; shrinks once before it would clip.
fn title(out: &mut Scene, assets: &Assets, value: &str, width: f32) -> f32 {
    let mut style = plain(fixed(assets, 52., 800, true, 1.05));
    let mut size = 52.;
    if assets.width(value, style) > width {
        size = 42.;
        style = plain(fixed(assets, size, 800, true, 1.05));
    }
    out.text(
        value,
        Rect::new(PAD, 105., width, size * 1.05 + 8.),
        style,
        WHITE,
        false,
    );
    105. + size * 1.05
}

fn header(out: &mut Scene, state: &State, assets: &Assets, p: &Session) {
    kicker(out, state, assets, p.request.source);
    let bottom = title(out, assets, &p.request.title, 1300.);
    if let Some(line) = p.request.episode_line.as_deref() {
        let style = plain(body(assets, 25., 600));
        out.text(
            line,
            Rect::new(PAD, bottom + 10., 1300., 32.),
            style,
            faint(0.8),
            false,
        );
    }
    let clock =
        time::OffsetDateTime::now_local().unwrap_or_else(|_| time::OffsetDateTime::now_utc());
    let now = format!("{}:{:02}", clock.hour(), clock.minute());
    out.text(
        now,
        Rect::new(RIGHT - 400., 72., 400., 56.),
        plain(body(assets, 44., 700)).right(),
        WHITE,
        false,
    );
    if p.duration_ms > 0 {
        let end = clock
            + time::Duration::milliseconds(p.duration_ms.saturating_sub(p.position_ms) as i64);
        out.text(
            format!("Ends at {}:{:02}", end.hour(), end.minute()),
            Rect::new(RIGHT - 400., 134., 400., 28.),
            plain(body(assets, 22., 500)).right(),
            faint(0.7),
            false,
        );
    }
    let close = Rect::new(RIGHT - 60., 170., 60., 60.);
    close_button(
        out,
        state,
        assets,
        close,
        "close",
        focused(state, Target::PlayerClose),
    );
    out.control(Target::PlayerClose, close, false);
}

/// Accent colour at `t` (0..1) along the gradient, so split segments line up.
fn accent_at(state: &State, t: f32) -> Color {
    let (a, b) = state.accent_colors();
    let (a, b) = (c(a, 1.), c(b, 1.));
    super::movie_view::mix(a, b, t)
}

fn timeline(out: &mut Scene, state: &State, assets: &Assets, p: &Session) {
    let total = p.duration_ms.max(1) as f32;
    let share = |ms: u64| (ms as f32 / total).clamp(0., 1.);
    let filled = share(p.position_ms);
    let buffered = share(p.buffered_ms).max(filled);
    // Chapters split the bar into segments with 3px gaps; no chapters = one segment.
    let mut cuts: Vec<f32> = p
        .request
        .chapters
        .iter()
        .map(|c| share(c.start_ms))
        .filter(|s| *s > 0.)
        .collect();
    cuts.insert(0, 0.);
    cuts.push(1.);
    for pair in cuts.windows(2) {
        let (s0, s1) = (pair[0], pair[1]);
        let gap = if s1 < 1. { 3. } else { 0. };
        let x0 = PAD + WIDE * s0;
        let w = (WIDE * (s1 - s0) - gap).max(0.);
        if w <= 0. {
            continue;
        }
        let seg = Rect::new(x0, BAR_Y, w, 10.);
        out.shape(seg, 5., faint(0.25));
        if buffered > s0 {
            let bw = (WIDE * (buffered.min(s1) - s0)).min(w);
            out.shape(Rect::new(x0, BAR_Y, bw, 10.), 5., faint(0.4));
        }
        if filled > s0 {
            let fw = (WIDE * (filled.min(s1) - s0)).min(w);
            let t1 = if filled > 0. {
                (s0 + fw / WIDE) / filled
            } else {
                1.
            };
            out.draw.push(Draw::HGradient(
                Rect::new(x0, BAR_Y, fw, 10.),
                5.,
                accent_at(state, s0 / filled.max(0.001)),
                accent_at(state, t1.min(1.)),
            ));
        }
    }
    let thumb_x = PAD + WIDE * filled;
    let seeking =
        matches!(state.focus, Target::PlayerToggle | Target::PlayerSeek) && !state.pointer;
    let accent = accent_at(state, 0.55);
    let r = if seeking { 17. } else { 14. };
    out.shadow(
        Rect::new(thumb_x - r, BAR_Y + 5. - r, 2. * r, 2. * r),
        r,
        18.,
        6.,
        0.5,
    );
    out.shape(
        Rect::new(
            thumb_x - r - 6.,
            BAR_Y + 5. - r - 6.,
            2. * r + 12.,
            2. * r + 12.,
        ),
        r + 6.,
        [accent[0], accent[1], accent[2], 0.25],
    );
    out.shape(
        Rect::new(thumb_x - r, BAR_Y + 5. - r, 2. * r, 2. * r),
        r,
        accent,
    );
    out.control(
        Target::PlayerSeek,
        Rect::new(PAD, BAR_Y - 26., WIDE, 62.),
        false,
    );

    // Chapter name rides above the thumb.
    if let Some(chapter) = p.current_chapter().filter(|_| p.hover_seek_ms.is_none()) {
        let x = (thumb_x - 200.).clamp(PAD, RIGHT - 400.);
        out.text(
            chapter.title.clone(),
            Rect::new(x, BAR_Y - 54., 400., 30.),
            plain(body(assets, 23., 700)),
            WHITE,
            true,
        );
    }
    // Pointer hover: time chip (and chapter) over the hovered point. media
    // streams carry no storyboard frames, so there is no picture to show.
    if let Some(hover) = p.hover_seek_ms {
        let x = PAD + WIDE * share(hover);
        let label = player::clock(hover);
        let style = body(assets, 23., 700);
        let w = assets.width(&label, style).ceil() + 28.;
        let chip = Rect::new((x - w / 2.).clamp(PAD, RIGHT - w), BAR_Y - 54., w, 38.);
        out.shape(chip, 12., c(0x000000, 0.7));
        out.text(label, line_in(chip, 23.), style, WHITE, true);
        if let Some(chapter) = p
            .request
            .chapters
            .iter()
            .rev()
            .find(|c| c.start_ms <= hover)
        {
            let cx = (x - 200.).clamp(PAD, RIGHT - 400.);
            out.text(
                chapter.title.clone(),
                Rect::new(cx, BAR_Y - 92., 400., 30.),
                plain(body(assets, 23., 700)),
                WHITE,
                true,
            );
        }
    }
    let times = body(assets, 25., 600);
    out.text(
        player::clock(p.position_ms),
        Rect::new(PAD, BAR_Y + 28., 400., 32.),
        times,
        WHITE,
        false,
    );
    let remains = p.duration_ms.saturating_sub(p.position_ms);
    out.text(
        format!("−{}", player::clock(remains)),
        Rect::new(RIGHT - 400., BAR_Y + 28., 400., 32.),
        times.right(),
        faint(0.7),
        false,
    );
}

fn bottom_row(out: &mut Scene, state: &State, assets: &Assets, p: &Session) {
    let mut x = PAD;
    for (i, pill) in p.pills().iter().enumerate() {
        let name = p.pill_name(*pill);
        let icon = match pill {
            Panel::Audio => "volume",
            Panel::Subtitles => "captions",
            Panel::Quality => "quality",
            Panel::Chapters => "list",
        };
        // One highlighted control only: while chapters are open, the cards own it.
        let level = if p.panel.is_none() {
            focused(state, Target::PlayerPill(i))
        } else {
            0.
        };
        let r = button(
            out,
            state,
            assets,
            &Button {
                icon,
                label: &name,
                trailing: false,
            },
            x,
            level,
        );
        out.control(Target::PlayerPill(i), r, false);
        x = r.x + r.w + GAP;
    }
    // Right-hand group, laid out from the right edge.
    let mut group: Vec<(Target, String, &str, bool)> = vec![];
    if p.request.source == Source::Series {
        if let Some((_, n)) = p.request.prev_episode {
            group.push((
                Target::PlayerPreviousEpisode,
                format!("Episode {n}"),
                "skip-back",
                false,
            ));
        }
        if let Some((_, n)) = p.request.next_episode {
            group.push((
                Target::PlayerNextEpisode,
                format!("Episode {n}"),
                "skip-forward",
                true,
            ));
        }
    } else {
        group.push((Target::PlayerRestart, "From start".into(), "refresh", false));
    }
    let widths: Vec<f32> = group
        .iter()
        .map(|(_, label, icon, trailing)| {
            button_width(
                assets,
                &Button {
                    icon,
                    label,
                    trailing: *trailing,
                },
            )
        })
        .collect();
    let mut gx = RIGHT - widths.iter().sum::<f32>() - GAP * group.len().saturating_sub(1) as f32;
    for ((target, label, icon, trailing), w) in group.into_iter().zip(widths) {
        let level = if p.panel.is_none() {
            focused(state, target.clone())
        } else {
            0.
        };
        let r = button(
            out,
            state,
            assets,
            &Button {
                icon,
                label: &label,
                trailing,
            },
            gx,
            level,
        );
        out.control(target, r, false);
        gx += w + GAP;
    }
}

fn centered_note(out: &mut Scene, assets: &Assets, line: &str, sub: Option<&str>) {
    let style = plain(fixed(assets, 44., 800, true, 1.1));
    out.text(line, Rect::new(PAD, 490., WIDE, 56.), style, WHITE, true);
    if let Some(sub) = sub {
        out.text(
            sub,
            Rect::new(PAD, 560., WIDE, 32.),
            plain(body(assets, 25., 500)),
            faint(0.7),
            true,
        );
    }
}

pub fn draw(out: &mut Scene, state: &State, assets: &Assets) {
    let Some(p) = state.playback.as_ref() else {
        return;
    };
    out.draw.clear();
    out.controls.clear();
    out.modal = None;
    // Everything here is drawn over the transparent frame that lets the
    // uMediaServer video plane show; nothing may be opaque full-screen.
    if !p.ready && p.error.is_none() {
        if let Some(poster) = p.request.poster.as_deref() {
            out.image(assets.path(poster), SCREEN, 0., false);
        }
        out.shape(SCREEN, 0., c(0x07090e, 0.6));
        centered_note(out, assets, "Loading…", None);
    }
    if let Some(error) = p.error.as_deref() {
        out.shape(SCREEN, 0., c(0x07090e, 0.75));
        centered_note(out, assets, "Can't play this video", Some(error));
    }
    let controls = p.controls_visible();
    if let Some(subtitle) = p.current_caption() {
        let y = if controls { 660. } else { 860. };
        let style = plain(body(assets, 38., 700));
        let lines = assets.lines(subtitle, style, 1380.).clamp(1, 3) as f32;
        let h = lines * 38. * 1.25 + 28.;
        let w = subtitle
            .lines()
            .map(|l| assets.width(l, style))
            .fold(0., f32::max)
            .min(1380.)
            + 56.;
        let box_ = Rect::new((1920. - w) / 2., y + 110. - h, w, h);
        out.shape(box_, 20., c(0x000000, 0.6));
        out.text(
            subtitle,
            Rect::new(box_.x + 28., box_.y + 14., w - 56., h - 28.),
            style,
            WHITE,
            true,
        );
    }
    if !controls {
        return;
    }
    let tracks_open = matches!(
        p.panel,
        Some(Panel::Audio | Panel::Subtitles | Panel::Quality)
    );
    if tracks_open {
        tracks(out, state, assets, p);
        return;
    }
    // Alpha scrims: 0.55 at the top fading out by 26 %, 0.82 at the bottom from 52 %.
    bottom_shade(
        out,
        Rect::new(0., 0., 1920., 281.),
        &[(0., 0.), (1., 0.55)],
        SCREEN,
    );
    let chapters_open = p.panel == Some(Panel::Chapters);
    let from = if chapters_open { 0.22 } else { 0.48 };
    bottom_shade(
        out,
        Rect::new(0., 1080. * (1. - from) - 1., 1920., 1080. * from + 1.),
        &[(0., 0.85), (1., 0.)],
        SCREEN,
    );
    out.draw.push(Draw::Clip(None));
    header(out, state, assets, p);
    // Whole picture toggles play/pause with the pointer.
    if chapters_open {
        chapters(out, state, assets, p);
    } else {
        out.control(
            Target::PlayerToggle,
            Rect::new(PAD, 250., WIDE, 520.),
            false,
        );
    }
    timeline(out, state, assets, p);
    bottom_row(out, state, assets, p);
    if p.request.source == Source::Series
        && p.request.next_episode.is_some()
        && p.duration_ms > 0
        && p.duration_ms.saturating_sub(p.position_ms) <= 30_000
    {
        let secs = p.duration_ms.saturating_sub(p.position_ms) / 1000;
        out.text(
            format!("Next episode in {secs}s"),
            Rect::new(RIGHT - 600., BAR_Y - 54., 600., 30.),
            plain(body(assets, 23., 700)).right(),
            WHITE,
            false,
        );
    }
}

fn chapters(out: &mut Scene, state: &State, assets: &Assets, p: &Session) {
    let rows = &p.request.chapters;
    let current = p
        .current_chapter()
        .and_then(|cur| rows.iter().position(|c| c.start_ms == cur.start_ms));
    out.text(
        "Chapters",
        Rect::new(PAD, 400., 600., 38.),
        plain(body(assets, 30., 700)),
        WHITE,
        false,
    );
    out.text(
        format!("{} of {}", current.map_or(1, |i| i + 1), rows.len()),
        Rect::new(RIGHT - 400., 406., 400., 30.),
        plain(body(assets, 22., 500)).right(),
        faint(0.6),
        false,
    );
    const SHOWN: usize = 5;
    let w = (WIDE - 28. * (SHOWN - 1) as f32) / SHOWN as f32;
    let h = (w * 9. / 16.).round();
    let first = p
        .selected_row
        .saturating_sub(2)
        .min(rows.len().saturating_sub(SHOWN));
    for (slot, i) in (first..rows.len().min(first + SHOWN)).enumerate() {
        let x = PAD + slot as f32 * (w + 28.);
        let card = Rect::new(x, 460., w, h);
        let on = state.focus == Target::PlayerChapter(i);
        if on {
            let (a, b) = state.accent_colors();
            out.shadow(card, 20., 40., 16., 0.5);
            if state.preferences.accent.kind == "gradient" {
                out.draw.push(Draw::DiagonalGradientBorder(
                    card.inset(-8.),
                    28.,
                    4.,
                    c(a, 1.),
                    c(b, 1.),
                ));
            } else {
                out.border(card.inset(-8.), 28., 4., c(a, 1.));
            }
        }
        out.shape(card, 20., c(0x262b32, 0.9));
        if let Some(thumb) = rows[i].thumbnail.as_deref() {
            out.draw.push(Draw::TintedImage(
                assets.path(thumb),
                card,
                20.,
                false,
                [1., 1., 1., if on { 1. } else { 0.75 }],
            ));
        }
        let style = plain(body(assets, 23., 600));
        out.text(
            rows[i].title.clone(),
            Rect::new(x, 460. + h + 12., w, 30.),
            style,
            WHITE,
            false,
        );
        let meta = if Some(i) == current {
            format!("{} · now", player::clock(rows[i].start_ms))
        } else {
            player::clock(rows[i].start_ms)
        };
        out.text(
            meta,
            Rect::new(x, 460. + h + 46., w, 26.),
            body(assets, 20., 500),
            faint(0.55),
            false,
        );
        out.control(
            Target::PlayerChapter(i),
            Rect::new(x, 460., w, h + 72.),
            false,
        );
    }
    out.modal = Some(Rect::new(PAD, 390., WIDE, h + 150.));
}

/// Centred audio/subtitle (or quality) panel over a dimmed picture.
fn tracks(out: &mut Scene, state: &State, assets: &Assets, p: &Session) {
    out.shape(SCREEN, 0., c(0x000000, 0.45));
    let panel = Rect::new(400., 96., 1120., 760.);
    out.shadow(panel, 44., 120., 40., 0.6);
    out.shape(panel, 44., c(PANEL_BG, 0.88));
    out.border(panel, 44., 1., faint(0.1));
    let quality = p.panel == Some(Panel::Quality);
    let heading = if quality {
        "Quality"
    } else {
        "Audio and subtitles"
    };
    out.text(
        heading,
        Rect::new(448., 136., 900., 60.),
        plain(fixed(assets, 44., 800, true, 1.1)),
        WHITE,
        false,
    );
    let close = Rect::new(panel.x + panel.w - 48. - 60., 136., 60., 60.);
    close_button(
        out,
        state,
        assets,
        close,
        "close",
        focused(state, Target::PlayerPanelClose),
    );
    out.control(Target::PlayerPanelClose, close, false);
    let inner = 1120. - 96.;
    let cols = if quality {
        vec![(Panel::Quality, 448., inner)]
    } else {
        let w = (inner - 40.) / 2.;
        vec![
            (Panel::Audio, 448., w),
            (Panel::Subtitles, 448. + w + 40., w),
        ]
    };
    const ROW: f32 = 68.;
    const VISIBLE: usize = 8;
    for (kind, x, w) in cols {
        let rows = p.panel_rows(kind);
        let count = if kind == Panel::Subtitles {
            rows.len().saturating_sub(1)
        } else {
            rows.len()
        };
        let label = match kind {
            Panel::Audio => format!("AUDIO · {count}"),
            Panel::Subtitles => format!("SUBTITLES · {count}"),
            _ => format!("QUALITY · {count}"),
        };
        out.text(
            label,
            Rect::new(x + 24., 226., w - 24., 23.),
            plain(body(assets, 18., 600).tracking(0.08)),
            faint(0.55),
            false,
        );
        let chosen = match kind {
            Panel::Audio => p.selected_audio,
            Panel::Subtitles => p.selected_subtitle.map_or(0, |i| i + 1),
            _ => p.selected_quality,
        };
        let anchor = if p.panel == Some(kind) {
            p.selected_row
        } else {
            chosen
        };
        let first = anchor
            .saturating_sub(VISIBLE - 1)
            .min(rows.len().saturating_sub(VISIBLE));
        for (slot, i) in (first..rows.len().min(first + VISIBLE)).enumerate() {
            let y = 259. + slot as f32 * ROW;
            let r = Rect::new(x, y, w, ROW);
            let target = match kind {
                Panel::Audio => Target::PlayerAudioOption(i),
                Panel::Subtitles => Target::PlayerSubtitleOption(i),
                _ => Target::PlayerOption(i),
            };
            let on = state.focus == target;
            if on {
                out.shadow(r, 18., 30., 12., 0.35);
                accent_fill(out, r, 18., state);
            }
            let ink = if on { accent_ink(state) } else { faint(0.9) };
            let style = plain(body(assets, 26., if on { 700 } else { 500 }));
            out.text(
                rows[i].clone(),
                line_in(Rect::new(x + 24., y, w - 48. - 40., ROW), 26.),
                style,
                ink,
                false,
            );
            if i == chosen && !on {
                out.icon(
                    assets,
                    "check",
                    Rect::new(x + w - 24. - 26., y + (ROW - 26.) / 2., 26., 26.),
                    accent_at(state, 0.55),
                );
            }
            out.control(target, r, true);
        }
    }
    out.modal = Some(panel);
    // Playback keeps going underneath: small badge centred at the bottom.
    let time = player::clock(p.position_ms);
    let t_style = body(assets, 25., 600);
    let n_style = plain(body(assets, 22., 500));
    let tw = assets.width(&time, t_style).ceil();
    let nw = assets.width("keeps playing", n_style).ceil();
    let bw = 40. + 24. + 14. + tw + 14. + nw + 40.;
    let badge = Rect::new((1920. - bw) / 2., 1080. - PAD - BUTTON_H, bw, BUTTON_H);
    out.shadow(badge, 42., 60., 20., 0.5);
    out.shape(badge, 42., c(PANEL_BG, 0.88));
    out.border(badge, 42., 1., faint(0.1));
    let mut bx = badge.x + 40.;
    out.icon(
        assets,
        if p.paused { "pause" } else { "play" },
        Rect::new(bx, badge.y + 30., 24., 24.),
        WHITE,
    );
    bx += 24. + 14.;
    out.text(
        time,
        line_in(Rect::new(bx, badge.y, tw + 4., BUTTON_H), 25.),
        t_style,
        WHITE,
        false,
    );
    bx += tw + 14.;
    out.text(
        "keeps playing",
        line_in(Rect::new(bx, badge.y, nw + 4., BUTTON_H), 22.),
        n_style,
        faint(0.7),
        false,
    );
}
