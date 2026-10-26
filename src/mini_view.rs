//! Home mini-player (mockup 26.10): glass pill under the header circles,
//! right-aligned with them. Cover, song (scrolls while the pill has focus
//! and the title does not fit), artist · album, progress, then Shuffle,
//! Previous, Play/Pause, Next and Repeat. Focus fills a button with the
//! accent; Shuffle and Repeat that are on keep an accent outline.
use super::*;
use crate::music::Status;
use crate::state::mini::ROW;

const TOP: f32 = 160.;
const H: f32 = 80.;
const W: f32 = 780.;
const PAD: f32 = 10.;
const COVER: f32 = 60.;
const BUTTON: f32 = 56.;
const GAP: f32 = 8.;
/// Marquee speed (px/s) and the gap before the title comes round again.
const SPEED: f32 = 60.;
const LOOP_GAP: f32 = 80.;

pub(super) fn mini_player(out: &mut Scene, state: &State, assets: &Assets) {
    let Some(now) = state.music.now.as_ref() else {
        return;
    };
    let pill = Rect::new(crate::state::HOME_STRIP_RIGHT - W, TOP, W, H);
    out.shadow(pill, H / 2., 24., 8., 0.25);
    out.glass(pill, H / 2.);

    // Cover.
    let cover = Rect::new(pill.x + PAD, pill.y + PAD, COVER, COVER);
    let f = state.focus_level(&Target::MiniCover);
    match &now.cover {
        Some(path) => out.image(path.clone(), cover, COVER / 2., false),
        None => out.shape(cover, COVER / 2., c(0xffffff, 0.1)),
    }
    out.focus_ring(cover, COVER / 2., f, state);
    out.control(Target::MiniCover, cover, false);

    // Buttons from the right edge.
    let buttons = ROW.len() - 1;
    let bx = pill.x + pill.w - PAD - 2. - buttons as f32 * BUTTON - (buttons - 1) as f32 * GAP;
    let by = pill.y + (H - BUTTON) / 2.;
    let (accent, _) = state.accent_colors();
    let playing = now.status == Status::Playing;
    for (i, target) in ROW.iter().skip(1).enumerate() {
        let r = Rect::new(bx + i as f32 * (BUTTON + GAP), by, BUTTON, BUTTON);
        let level = state.focus_level(target);
        let on = match target {
            Target::MiniShuffle => now.shuffle,
            Target::MiniRepeat => now.repeat,
            _ => false,
        };
        out.shape(r, BUTTON / 2., c(0xffffff, 0.1 * (1. - level)));
        if level > 0. {
            let at = out.draw.len();
            out.shadow(r, BUTTON / 2., 20., 8., 0.35);
            accent_fill(out, r, BUTTON / 2., state);
            out.fade(at, level, 1.);
        }
        if on && level < 1. {
            out.border(r.inset(1.), BUTTON / 2. - 1., 2., c(accent, 1. - level));
        }
        let rest = if on { c(accent, 1.) } else { c(0xffffff, 0.92) };
        let ink = blend(rest, accent_ink(state), level);
        let icon = match target {
            Target::MiniShuffle => "shuffle",
            Target::MiniPrev => "skip-back",
            Target::MiniPlay if playing || now.status == Status::Loading => "pause",
            Target::MiniPlay => "play",
            Target::MiniNext => "skip-forward",
            _ => "repeat",
        };
        out.icon(assets, icon, r.inset(16.), ink);
        out.control(target.clone(), r, false);
    }

    // Song, artist · album, progress.
    let tx = cover.x + COVER + 16.;
    let tw = bx - 16. - tx;
    let title = now.current().map_or("", |s| s.title.as_str());
    let style = TextStyle::new(22.).weight(600);
    let width = assets.width(title, style);
    let line = Rect::new(tx, pill.y + 9., tw, 30.);
    let focused = ROW.contains(&state.focus);
    if width > tw && focused && !state.preferences.reduced_motion {
        let t = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0., |d| d.as_secs_f64() as f32);
        let span = width + LOOP_GAP;
        let shift = (t * SPEED) % span;
        out.draw.push(Draw::Clip(Some(line)));
        for x in [tx - shift, tx - shift + span] {
            out.text(
                title,
                Rect::new(x, line.y, width + 4., line.h),
                style,
                WHITE,
                false,
            );
        }
        out.draw.push(Draw::Clip(None));
    } else {
        out.text(fit(assets, title, style, tw), line, style, WHITE, false);
    }
    let sub = TextStyle::new(18.).weight(500);
    let caption = match &now.error {
        Some(_) => "Can't play this song".to_owned(),
        None if now.status == Status::Loading => "Loading…".to_owned(),
        None => [
            now.current()
                .map(|s| s.artist.as_str())
                .filter(|a| !a.is_empty())
                .unwrap_or(now.artist.as_str()),
            now.album.as_str(),
        ]
        .iter()
        .filter(|s| !s.is_empty())
        .copied()
        .collect::<Vec<_>>()
        .join(" · "),
    };
    out.text(
        fit(assets, &caption, sub, tw),
        Rect::new(tx, pill.y + 38., tw, 24.),
        sub,
        c(0xffffff, 0.7),
        false,
    );
    let bar = Rect::new(tx, pill.y + 66., tw, 4.);
    out.shape(bar, 2., c(0xffffff, 0.2));
    let filled = bar.w * now.progress();
    if filled > 0.5 {
        accent_fill(
            out,
            Rect::new(bar.x, bar.y, filled.max(4.), bar.h),
            2.,
            state,
        );
    }
}

fn fit(assets: &Assets, value: &str, style: TextStyle, w: f32) -> String {
    if assets.width(value, style) <= w {
        return value.into();
    }
    let mut s = value.to_string();
    while s.chars().count() > 1 && assets.width(&(s.clone() + "…"), style) > w {
        s.pop();
    }
    s.push('…');
    s
}
