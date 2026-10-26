//! Album card popup (mockup "Музыка: альбом", 25.10): the movie card's
//! panel over a blurred copy of the cover; the square cover, title and
//! artist on the left, Play / Shuffle and the songs on the right.
use super::movie_view::{self, Button, PAD, PANEL, RADIUS};
use super::*;
use crate::motion;
use crate::state::album::{AlbumCard, SONGS_VISIBLE};
use crate::state::movie;

const COVER: f32 = 440.;
const RIGHT_X: f32 = 900.;
const RIGHT_W: f32 = 788.;
const ROW_H: f32 = 70.;
const BLOCK_DY: f32 = 22.;

pub(super) fn album_card(out: &mut Scene, state: &State, assets: &Assets) {
    if let Some((card, closed)) = &state.album_closing {
        let t = 1. - motion::progress(closed.elapsed().as_secs_f32(), movie::CLOSE_TIME);
        if t > 0. && !state.preferences.reduced_motion {
            let controls = out.controls.len();
            draw(out, state, assets, card, false, t);
            out.controls.truncate(controls);
        }
    }
    if let Some(card) = state.album.as_ref() {
        draw(out, state, assets, card, true, 1.);
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

fn clock(ms: u64) -> String {
    let s = ms / 1000;
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60)
    } else {
        format!("{}:{:02}", s / 60, s % 60)
    }
}

fn draw(out: &mut Scene, state: &State, assets: &Assets, card: &AlbumCard, live: bool, fade: f32) {
    if live {
        out.controls.clear();
    }
    let reduced = state.preferences.reduced_motion;
    let open = if !live {
        fade
    } else if reduced {
        1.
    } else {
        motion::progress(card.opened.elapsed().as_secs_f32(), movie::OPEN_TIME)
    };
    let stage = |n: f32| match card.shown {
        _ if !live => 1.,
        None => 0.,
        Some(_) if reduced => 1.,
        Some(t) => motion::progress(
            t.elapsed().as_secs_f32() - n * movie::STAGGER,
            movie::BLOCK_TIME,
        ),
    };
    let album = card.album.as_ref();
    let start = out.draw.len();
    let p = PANEL;
    out.shape(Rect::new(0., 0., 1920., 1080.), 0., c(0x000000, 0.82));
    out.shadow(p, RADIUS, 140., 50., 0.65);
    out.shape(p, RADIUS, c(0x0d0e12, 1.));
    out.draw.push(Draw::RoundClip(Some((p, RADIUS))));
    out.draw.push(Draw::Clip(Some(p)));
    // The cover, blurred, fills the panel under a dark veil.
    if let Some(path) = album.and_then(|a| a.backdrop.clone()) {
        let at = out.draw.len();
        out.image(path, p, RADIUS, false);
        let k = match card.backdrop_at {
            Some(t) if live && !reduced => {
                motion::progress(t.elapsed().as_secs_f32(), movie::BACKDROP_TIME)
            }
            _ => 1.,
        };
        movie_view::slide(out, at, k * 0.55, 0., 0.);
    }
    out.shape(p, RADIUS, c(0x0a0b0e, 0.7));

    // Left column: cover, kicker, title, artist, year · songs · minutes.
    let left = p.x + PAD;
    let mut y = p.y + PAD;
    let cover = album
        .and_then(|a| a.cover.clone())
        .or_else(|| card.poster.clone());
    let at = out.draw.len();
    let rect = Rect::new(left, y, COVER, COVER);
    out.shadow(rect, 26., 80., 30., 0.55);
    match cover {
        Some(path) => out.image(path, rect, 26., false),
        None => out.shape(rect, 26., c(0xffffff, 0.08)),
    }
    movie_view::slide(out, at, stage(0.), 0., BLOCK_DY);
    y += COVER + 34.;

    let at = out.draw.len();
    let kick = movie_view::body(assets, 18., 600).tracking(0.08);
    let (accent, _) = state.accent_colors();
    let kw = assets.width("ALBUM", kick).ceil();
    out.text(
        "ALBUM",
        Rect::new(left, y, kw + 4., 23.),
        kick,
        c(accent, 1.),
        false,
    );
    out.shape(
        Rect::new(left + kw + 14., y + 9., 5., 5.),
        2.5,
        c(0xffffff, 0.4),
    );
    out.text(
        "JELLYFIN",
        Rect::new(left + kw + 33., y, 300., 23.),
        kick,
        c(0xffffff, 0.55),
        false,
    );
    y += 23. + 12.;
    let width = RIGHT_X - PAD - 52.;
    let title = album.map_or(card.title.as_str(), |a| a.title.as_str());
    let big = movie_view::fixed(assets, 64., 900, true, 1.04).tracking(-0.02);
    let lines = (assets.lines(title, big, width) as f32).clamp(1., 2.);
    out.text(
        title,
        Rect::new(left, y, width, 64. * 1.04 * lines + 2.),
        big,
        WHITE,
        false,
    );
    y += 64. * 1.04 * lines + 12.;
    if let Some(a) = album {
        let style = movie_view::body(assets, 27., 600);
        if !a.artist.is_empty() {
            out.text(
                fit(assets, &a.artist, style, width),
                Rect::new(left, y, width, 34.),
                style,
                c(0xffffff, 0.85),
                false,
            );
            y += 34. + 6.;
        }
        let style = movie_view::body(assets, 23., 500);
        out.text(
            a.meta(),
            Rect::new(left, y, width, 30.),
            style,
            c(0xffffff, 0.6),
            false,
        );
    }
    movie_view::slide(out, at, stage(1.), 0., BLOCK_DY);

    // Right column: Play and Shuffle, then the songs. While this album is
    // the music queue, Play pauses it and its song keeps an outline.
    let x = p.x + RIGHT_X;
    let y = p.y + PAD;
    let at = out.draw.len();
    let now = state.music.now.as_ref().filter(|n| n.album_id == card.id);
    let playing = now.is_some_and(|n| n.status != crate::music::Status::Paused);
    let (accent, _) = state.accent_colors();
    let buttons = [
        (
            Target::AlbumPlay,
            if playing {
                Button {
                    icon: Some("pause"),
                    label: "Pause",
                    chevron: false,
                    primary: true,
                    size: 27.,
                }
            } else {
                Button {
                    icon: Some("play"),
                    label: "Play",
                    chevron: false,
                    primary: true,
                    size: 27.,
                }
            },
        ),
        (
            Target::AlbumShuffle,
            Button {
                icon: Some("shuffle"),
                label: "Shuffle",
                chevron: false,
                primary: false,
                size: 27.,
            },
        ),
    ];
    let mut bx = x;
    for (target, b) in &buttons {
        let level = state.focus_level(target);
        let r = movie_view::button(out, state, assets, b, bx, y, level);
        if *target == Target::AlbumShuffle && now.is_some_and(|n| n.shuffle) && level < 1. {
            out.border(r.inset(1.), r.h / 2. - 1., 2., c(accent, 1. - level));
        }
        out.control(target.clone(), r, false);
        bx += r.w + 16.;
    }
    movie_view::slide(out, at, stage(1.), 0., BLOCK_DY);

    let at = out.draw.len();
    let mut y = y + movie_view::BUTTON_H + 36.;
    let label = movie_view::body(assets, 18., 600).tracking(0.08);
    out.text(
        "SONGS",
        Rect::new(x + 24., y, 300., 23.),
        label,
        c(0xffffff, 0.55),
        false,
    );
    y += 23. + 10.;
    let songs = album.map(|a| a.songs.as_slice()).unwrap_or(&[]);
    if songs.is_empty() {
        let note = if card.shown.is_some() {
            "No songs"
        } else {
            "Loading…"
        };
        out.text(
            note,
            Rect::new(x + 24., y + 18., RIGHT_W, 34.),
            movie_view::body(assets, 25., 500),
            c(0xffffff, 0.6),
            false,
        );
    }
    let numbers = songs.iter().any(|s| s.number > 0);
    for (n, song) in songs
        .iter()
        .enumerate()
        .skip(card.first)
        .take(SONGS_VISIBLE)
    {
        let row = Rect::new(x, y, RIGHT_W, ROW_H);
        let target = Target::AlbumSong(n);
        let level = state.focus_level(&target);
        // Rows are bare at rest; only the focused one takes the accent.
        if level > 0. {
            let at = out.draw.len();
            out.shadow(row, 18., 30., 12., 0.35);
            accent_fill(out, row, 18., state);
            motion::transform_group(&mut out.draw[at..], 0., 0., level);
        }
        let current = now.is_some_and(|q| q.song() == n);
        if current && level < 1. {
            out.border(row.inset(1.), 17., 2., c(accent, 1. - level));
        }
        let ink = movie_view::mix(c(0xffffff, 0.88), accent_ink(state), level);
        let faint = movie_view::mix(
            if current {
                c(accent, 1.)
            } else {
                c(0xffffff, 0.45)
            },
            accent_ink(state),
            level,
        );
        let style = movie_view::body(assets, 25., if level > 0.5 { 600 } else { 500 });
        let small = movie_view::body(assets, 22., 500);
        let number = if numbers {
            song.number.max(0) as usize
        } else {
            n + 1
        };
        out.text(
            number.to_string(),
            movie_view::line_in(Rect::new(row.x + 24., row.y, 56., ROW_H), 25.),
            style,
            faint,
            false,
        );
        let time = clock(song.ms);
        let tw = 90.;
        let name_w = RIGHT_W - 24. - 56. - tw - 24.;
        out.text(
            fit(assets, &song.title, style, name_w),
            movie_view::line_in(Rect::new(row.x + 80., row.y, name_w, ROW_H), 25.),
            style,
            ink,
            false,
        );
        if song.ms > 0 {
            let w = assets.width(&time, small).ceil();
            out.text(
                &time,
                movie_view::line_in(
                    Rect::new(row.x + RIGHT_W - 24. - w, row.y, w + 2., ROW_H),
                    22.,
                ),
                small,
                faint,
                false,
            );
        }
        out.control(target, row, false);
        y += ROW_H;
    }
    // More below or above: the list fades at that edge.
    if card.first + SONGS_VISIBLE < songs.len() {
        out.text(
            format!("{} more", songs.len() - card.first - SONGS_VISIBLE),
            Rect::new(x + 24., y + 8., RIGHT_W, 30.),
            movie_view::body(assets, 22., 500),
            c(0xffffff, 0.45),
            false,
        );
    }
    movie_view::slide(out, at, stage(2.), 0., BLOCK_DY);

    out.draw.push(Draw::Clip(None));
    out.draw.push(Draw::RoundClip(None));
    let close = Rect::new(p.x + p.w - 24. - 60., p.y + 24., 60., 60.);
    movie_view::close_button(
        out,
        state,
        assets,
        close,
        "close",
        state.focus_level(&Target::AlbumClose),
    );
    out.control(Target::AlbumClose, close, false);
    if live {
        out.modal = Some(p);
    }
    if open < 1. {
        motion::transform_group(&mut out.draw[start..], 0., 24. * (1. - open), open);
    }
}
