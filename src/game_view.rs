//! Game card popup (mockup "Game page", 25.10): the movie card's panel,
//! padding and two-state round buttons, with details from Steam when the
//! title is found there. Missing fields are left out, never shown empty.
use super::movie_view::{self, Button, PAD, PANEL, RADIUS};
use super::*;
use crate::motion;
use crate::state::movie;

pub(super) fn game_card(out: &mut Scene, state: &State, assets: &Assets) {
    if let Some((card, closed)) = &state.game_closing {
        let t = 1. - motion::progress(closed.elapsed().as_secs_f32(), movie::CLOSE_TIME);
        if t > 0. && !state.preferences.reduced_motion {
            // A closing card is only a picture: it takes no focus or clicks.
            let controls = out.controls.len();
            draw_game(out, state, assets, card, false, t);
            out.controls.truncate(controls);
        }
    }
    if let Some(game) = state.game.as_ref() {
        draw_game(out, state, assets, game, true, 1.);
    }
}

fn draw_game(
    out: &mut Scene,
    state: &State,
    assets: &Assets,
    game: &crate::state::GameCard,
    live: bool,
    fade: f32,
) {
    if live {
        out.controls.clear();
    }
    let info = state.game_info.get(&game.id).filter(|i| i.steam_id > 0);
    let reduced = state.preferences.reduced_motion;
    let open = if !live {
        fade
    } else if reduced {
        1.
    } else {
        motion::progress(game.opened.elapsed().as_secs_f32(), movie::OPEN_TIME)
    };
    // Only the panel until Steam answered (or GAME_WAIT ran out), then the
    // blocks come in one by one, as on the movie card.
    let stage = |n: f32| match game.shown {
        None => 0.,
        Some(_) if reduced => 1.,
        Some(t) => motion::progress(
            t.elapsed().as_secs_f32() - n * movie::STAGGER,
            movie::BLOCK_TIME,
        ),
    };
    let start = out.draw.len();
    let p = PANEL;
    out.shape(Rect::new(0., 0., 1920., 1080.), 0., c(0x000000, 0.82));
    out.shadow(p, RADIUS, 140., 50., 0.65);
    out.shape(p, RADIUS, c(0x0b0c10, 1.));
    out.draw.push(Draw::RoundClip(Some((p, RADIUS))));
    out.draw.push(Draw::Clip(Some(p)));
    // Wide Steam art fills the panel; the portrait cover never stretches.
    let hero = info.and_then(|i| i.hero.clone());
    if let Some(path) = hero.as_ref() {
        let at = out.draw.len();
        out.image(path.clone(), p, RADIUS, false);
        let k = match game.backdrop_at {
            Some(t) if !reduced => {
                motion::progress(t.elapsed().as_secs_f32(), movie::BACKDROP_TIME)
            }
            Some(_) => 1.,
            None => 0.,
        };
        movie_view::slide(out, at, k, 0., 0.);
    }
    // The movie card's shading, unchanged: dark on the left and at the bottom.
    movie_view::h_stops(
        out,
        p,
        &[(0., 0.97), (0.4, 0.9), (0.72, 0.35), (1., 0.55)],
        p,
    );
    movie_view::bottom_shade(out, p, &[(0., 0.96), (0.38, 0.)], p);

    let left = p.x + PAD;
    let text_w = 1000.;
    let mut y = p.y + PAD;
    let kick = movie_view::body(assets, 18., 600).tracking(0.08);
    let (accent, _) = state.accent_colors();
    let source = if info.is_some() {
        format!("{} · STEAM", game.host.to_uppercase())
    } else {
        format!("{} · MOONLIGHT", game.host.to_uppercase())
    };
    let at = out.draw.len();
    let kw = assets.width("GAME", kick).ceil();
    out.text(
        "GAME",
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
        &source,
        Rect::new(left + kw + 33., y, 600., 23.),
        kick,
        c(0xffffff, 0.55),
        false,
    );
    y += 23. + 16.;
    let (h, _) = movie_view::title(out, assets, &game.title, left, y, text_w);
    y += h + 16.;
    movie_view::slide(out, at, stage(0.), 0., BLOCK_DY);

    if let Some(i) = info {
        let at = out.draw.len();
        let mut parts: Vec<String> = Vec::new();
        if !i.year.is_empty() {
            parts.push(i.year.clone());
        }
        if !i.genres.is_empty() {
            parts.push(i.genres.join(", "));
        }
        if !i.developer.is_empty() {
            parts.push(i.developer.trim_end_matches(" A/S").to_owned());
        }
        let meta = parts.join("  ·  ");
        let style = movie_view::body(assets, 26., 400);
        let mw = assets.width(&meta, style).ceil().min(text_w - 90.);
        out.text(
            &meta,
            Rect::new(left, y, mw + 4., 34.),
            style,
            c(0xffffff, 0.75),
            false,
        );
        if i.metacritic > 0 {
            let chip = Rect::new(left + mw + 18., y - 3., 58., 40.);
            let colour = if i.metacritic >= 75 {
                0x66cc33
            } else if i.metacritic >= 50 {
                0xffcc33
            } else {
                0xff5a4f
            };
            out.shape(chip, 12., c(colour, 1.));
            out.text(
                i.metacritic.to_string(),
                movie_view::line_in(chip, 24.),
                movie_view::body(assets, 24., 700),
                c(0x0b0d10, 1.),
                true,
            );
        }
        y += 34. + 24.;
        movie_view::slide(out, at, stage(1.), 0., BLOCK_DY);
        let at = out.draw.len();
        if !i.description.is_empty() {
            let style = movie_view::fixed(assets, 27., 400, false, 1.42);
            let used = (assets.lines(&i.description, style, 960.) as f32).min(3.);
            out.text(
                &i.description,
                Rect::new(left, y, 960., 27. * 1.42 * 3. + 1.),
                style,
                c(0xffffff, 0.84),
                false,
            );
            y += 27. * 1.42 * used;
        }
        movie_view::slide(out, at, stage(2.), 0., BLOCK_DY);
        y += 32.;
    } else {
        y += 16.;
    }

    // Feature pills: real data only, text centred in the pill.
    let mut pills: Vec<(&str, &str)> = Vec::new();
    if game.hdr {
        pills.push(("weather-sun", "HDR"));
    }
    if info.is_some_and(|i| i.controller) {
        pills.push(("gaming", "Full controller support"));
    }
    let at = out.draw.len();
    if !pills.is_empty() {
        let style = movie_view::body(assets, 24., 600);
        let mut x = left;
        for (icon, label) in pills {
            let w = 28. + 26. + 12. + assets.width(label, style).ceil() + 28.;
            let r = Rect::new(x, y, w, 64.);
            out.shape(r, 32., c(0xffffff, 0.08));
            out.border(r, 32., 1., c(0xffffff, 0.10));
            out.icon(assets, icon, Rect::new(x + 28., y + 19., 26., 26.), WHITE);
            out.text(
                label,
                movie_view::line_in(Rect::new(x + 66., y, w - 66., 64.), 24.),
                style,
                WHITE,
                false,
            );
            x += w + 16.;
        }
        y += 64. + 40.;
    }
    movie_view::slide(out, at, stage(3.), 0., BLOCK_DY);

    // Buttons: two states only (grey / accent gradient), sized to content.
    let resume = format!("Resume on {}", game.host);
    let play_label = format!("Play on {}", game.host);
    let mut buttons: Vec<(Target, Button)> = vec![(
        Target::GamePlay,
        Button {
            icon: Some("play"),
            label: if game.running { &resume } else { &play_label },
            chevron: false,
            primary: true,
            size: 28.,
        },
    )];
    if game.running {
        buttons.push((
            Target::GameQuit,
            Button {
                icon: Some("close"),
                label: "Close game",
                chevron: false,
                primary: false,
                size: 28.,
            },
        ));
    }
    if !game.online {
        buttons.push((
            Target::GameWake,
            Button {
                icon: Some("power"),
                label: "Wake PC",
                chevron: false,
                primary: false,
                size: 28.,
            },
        ));
    }
    if info.is_some_and(|i| !i.trailer.is_empty()) {
        buttons.push((
            Target::GameTrailer,
            Button {
                icon: Some("trailer"),
                label: "Trailer",
                chevron: false,
                primary: false,
                size: 28.,
            },
        ));
    }
    let favorite = state.preferences.games.favorites.contains(&game.id);
    buttons.push((
        Target::GameFavorite,
        Button {
            icon: Some(if favorite { "heart-filled" } else { "heart" }),
            label: "",
            chevron: false,
            primary: false,
            size: 28.,
        },
    ));
    let at = out.draw.len();
    let mut x = left;
    for (target, b) in &buttons {
        let r = movie_view::button(out, state, assets, b, x, y, state.focus_level(target));
        out.control(target.clone(), r, false);
        x += r.w + 20.;
    }
    movie_view::slide(out, at, stage(4.), 0., BLOCK_DY);
    let below_buttons = y + 84. + 48.;

    // Screenshots along the bottom; without Steam, the upright Apollo cover.
    let shots = info.map(|i| i.screenshots.as_slice()).unwrap_or(&[]);
    // 16:9 thumbnails in the space left under the buttons, never over them.
    let sh = (p.y + p.h - 80. - below_buttons).min(216.);
    if !shots.is_empty() && sh >= 120. {
        let sw = (sh * 16. / 9.).round();
        let sy = p.y + p.h - 80. - sh;
        // The right column holds the exit hint and the host chip.
        let limit = p.x + p.w - PAD - 640.;
        for (n, shot) in shots.iter().take(3).enumerate() {
            let r = Rect::new(left + n as f32 * (sw + 24.), sy, sw, sh);
            if r.x + r.w > limit {
                break;
            }
            let at = out.draw.len();
            let target = Target::GameShot(n);
            let t = state.focus_level(&target);
            out.image(shot.clone(), r, 24., false);
            // The Home focus frame: 3 px of the chosen accent (or its gradient).
            out.border(r, 24., 1., c(0xffffff, 0.1 * (1. - t)));
            out.focus_ring(r, 24., t, state);
            out.control(target, r, false);
            let delay = (movie::ROW_DELAY + n as f32 * movie::CARD_STAGGER) / movie::STAGGER;
            movie_view::slide(out, at, stage(delay), CARD_DX, 0.);
        }
    }
    // The upright poster, where and as large as on a movie card: Steam's
    // library art, else the Apollo cover. Decided once the card shows, never
    // swapped mid-entrance for art that is still on its way.
    if game.shown.is_some() {
        let poster = info
            .and_then(|i| i.portrait.clone())
            .or_else(|| game.poster.clone());
        if let Some(path) = poster {
            let at = out.draw.len();
            // The same poster slot as the movie and series pages.
            let rect = movie_view::poster_slot(p, 0.);
            out.shadow(rect, 30., 100., 40., 0.65);
            out.image(path, rect, movie_view::POSTER_RADIUS, false);
            movie_view::slide(out, at, stage(1.), 40., 0.);
        }
    }

    // Host status chip, bottom right.
    let status = if game.running {
        format!("Playing on {}", game.host)
    } else if game.online {
        format!("{} is online · ready to stream", game.host)
    } else {
        format!("{} is offline", game.host)
    };
    let style = movie_view::body(assets, 22., 600);
    let cw = assets.width(&status, style).ceil() + 24. + 12. + 12. + 24.;
    let chip = Rect::new(p.x + p.w - PAD - cw, p.y + p.h - 80. - 56., cw, 56.);
    let at = out.draw.len();
    out.shape(chip, 28., c(0x0e1015, 0.6));
    out.border(chip, 28., 1., c(0xffffff, 0.1));
    out.shape(
        Rect::new(chip.x + 24., chip.y + 22., 12., 12.),
        6.,
        if game.online {
            c(0x7fe3b0, 1.)
        } else {
            c(0xbbbdc1, 1.)
        },
    );
    out.text(
        &status,
        movie_view::line_in(Rect::new(chip.x + 48., chip.y, cw - 48., 56.), 22.),
        style,
        c(0xffffff, 0.85),
        false,
    );
    // How to leave the stream: Moonlight has the gamepad, not Luma. With root,
    // pad_watch closes the game on a double Start+Select.
    let hint = if state.root.is_some() {
        "To exit the game, press Start + Select twice"
    } else {
        "To exit, hold Start + Select + LB + RB, then Quit"
    };
    let hint_w = assets.width(hint, style).ceil();
    out.text(
        hint,
        Rect::new(p.x + p.w - PAD - hint_w, chip.y - 48., hint_w, 30.),
        style,
        c(0xffffff, 0.7),
        false,
    );
    movie_view::slide(out, at, stage(5.), 0., BLOCK_DY);
    if let Some(note) = state
        .moonlight_status
        .as_ref()
        .filter(|s| state.moonlight_pending.is_some() || s.starts_with("Closing"))
    {
        out.text(
            note,
            Rect::new(p.x + p.w - PAD - 600., chip.y - 92., 600., 30.),
            style,
            c(0xffffff, 0.7),
            false,
        );
    }
    out.draw.push(Draw::Clip(None));
    out.draw.push(Draw::RoundClip(None));
    let close = Rect::new(p.x + p.w - 24. - 60., p.y + 24., 60., 60.);
    movie_view::close_button(
        out,
        state,
        assets,
        close,
        "close",
        state.focus_level(&Target::GameClose),
    );
    out.control(Target::GameClose, close, false);
    if live {
        out.modal = Some(p);
    }
    if open < 1. {
        motion::transform_group(&mut out.draw[start..], 0., 24. * (1. - open), open);
    }
    if let (true, Some(n), Some(i)) = (live, game.shot, info) {
        shot_viewer(out, state, assets, &i.screenshots_full, n);
    }
}

/// Full-screen screenshot: ◀ ▶ page through, Back or OK closes.
fn shot_viewer(out: &mut Scene, state: &State, assets: &Assets, shots: &[String], n: usize) {
    let Some(path) = shots.get(n) else {
        return;
    };
    out.controls.clear();
    let screen = Rect::new(0., 0., 1920., 1080.);
    out.shape(screen, 0., c(0x000000, 0.96));
    out.image(path.clone(), screen, 0., true);
    let label = format!("{} / {}", n + 1, shots.len());
    let style = movie_view::body(assets, 24., 600);
    let w = assets.width(&label, style).ceil() + 48.;
    let chip = Rect::new((1920. - w) / 2., 1080. - 48. - 56., w, 56.);
    out.shape(chip, 28., c(0x0e1015, 0.7));
    out.text(
        &label,
        movie_view::line_in(chip, 24.),
        style,
        c(0xffffff, 0.9),
        true,
    );
    let close = Rect::new(1920. - 48. - 60., 48., 60., 60.);
    movie_view::close_button(out, state, assets, close, "close", 1.);
    out.control(Target::GameShot(n), close, false);
}
const BLOCK_DY: f32 = 22.;
const CARD_DX: f32 = 160.;
