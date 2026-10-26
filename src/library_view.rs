//! Library page (mockup 25.10): a fixed header (Back, title, tabs, search)
//! over the Apps grid of Home-size posters. Only rows on screen are drawn.
use super::*;
use crate::library::{CARD_W, CLOSE_TIME, COLUMNS, GRID_LEFT, GRID_TOP, Library, PITCH_X};

const CARD_RADIUS: f32 = TILE_RADIUS;
const HEADER_Y: f32 = 108.;
const BAR_H: f32 = 56.;

pub(super) fn library_page(out: &mut Scene, state: &State, assets: &Assets) {
    if let Some((closing, closed)) = &state.library_closing {
        let t = 1. - motion::progress(closed.elapsed().as_secs_f32(), CLOSE_TIME);
        if t > 0. && !state.preferences.reduced_motion {
            let controls = out.controls.len();
            let start = out.draw.len();
            draw(out, state, assets, closing, false);
            out.fade(start, t, 1.);
            out.controls.truncate(controls);
        }
    }
    if let Some(library) = state.library.as_ref() {
        let start = out.draw.len();
        draw(out, state, assets, library, true);
        if !state.preferences.reduced_motion {
            let t = library.open_progress();
            if t < 1. {
                out.fade(start, t, 1.);
            }
        }
    }
}

fn draw(out: &mut Scene, state: &State, assets: &Assets, library: &Library, live: bool) {
    if live {
        out.controls.clear();
    }
    // The page covers Home completely and stands still; only the grid moves.
    let screen = Rect::new(0., 0., 1920., 1080.);
    out.shape(screen, 0., c(0x050607, 0.6));
    out.glass_on_scrim(screen, 0., 0.4);
    grid(out, state, assets, library);
    header(out, state, assets, library);
    out.modal = Some(screen);
}

fn header(out: &mut Scene, state: &State, assets: &Assets, library: &Library) {
    let language = state.preferences.language;
    let _ = language;
    // Back: the same round button as everywhere, grey or the accent.
    let back = Rect::new(GRID_LEFT, HEADER_Y, BAR_H, BAR_H);
    round_button(out, state, assets, back, "chevron-left", Target::LibBack);
    // Title and how many cards the list has.
    let title_style = heading_fixed(assets, 40., 600).tracking(-0.01);
    let count_style = text(22., 500, 0.);
    let count = count_label(state, library);
    let title_x = back.x + back.w + 24.;
    // The tabs never move: the count line changes width as focus moves.
    let title_w = assets.width(&library.title, title_style).max(250.);
    out.text(
        library.title.as_str(),
        Rect::new(title_x, HEADER_Y - 10., title_w + 4., 48.),
        title_style,
        c(0xffffff, 0.96),
        false,
    );
    out.text(
        count,
        Rect::new(title_x, HEADER_Y + 38., title_w + 4., 28.),
        count_style,
        c(0xeeeeee, 0.6),
        false,
    );
    // Tabs: the Apps category bar.
    let style = nav_label_style();
    let pad = 4.;
    let widths: Vec<f32> = library
        .tabs
        .iter()
        .map(|(_, label)| assets.width(label, style) + 2. * 24.)
        .collect();
    let bar = Rect::new(
        title_x + title_w + 40.,
        HEADER_Y,
        widths.iter().sum::<f32>()
            + pad * 2.
            + NAV_TAB_GAP * (library.tabs.len().saturating_sub(1)) as f32,
        BAR_H,
    );
    out.shadow(bar, BAR_H / 2., 18., 4., 0.06);
    out.glass(bar, BAR_H / 2.);
    let mut x = bar.x + pad;
    for (index, ((_, label), w)) in library.tabs.iter().zip(widths).enumerate() {
        let target = Target::LibTab(index);
        let rect = Rect::new(x, bar.y + pad, w, BAR_H - 2. * pad);
        // A search shows results of the whole source: no tab is selected.
        let selected = index == library.tab && library.query.is_empty();
        let focused = state.focus == target;
        // One filled element on screen: the shared sub-tab of Apps and Library.
        let ink = sub_tab(out, state, rect, focused, selected);
        out.text(label.as_str(), line_box(rect, style), style, ink, true);
        out.control(target, rect, true);
        x += w + NAV_TAB_GAP;
    }
    // Search: the Apps field.
    let search = Rect::new(1536., HEADER_Y, 300., BAR_H);
    let focused = matches!(state.focus, Target::LibSearch | Target::LibClear);
    if focused {
        out.shape(search, BAR_H / 2., c(0x303438, 1.));
        out.focus_ring(search, BAR_H / 2., 1., state);
    } else {
        out.glass(search, BAR_H / 2.);
    }
    out.icon(
        assets,
        "search",
        Rect::new(search.x + 21., search.y + (search.h - 20.) / 2., 20., 20.),
        c(0xeeeeee, 0.73),
    );
    let typing = state.text_input_active() && state.keyboard_field == TextField::Library;
    let shown = if typing {
        library.draft.as_str()
    } else {
        library.query.as_str()
    };
    let placeholder = if library.games {
        "Find a game"
    } else {
        "Find a title"
    };
    let has_text = !library.query.is_empty();
    out.text(
        if shown.is_empty() { placeholder } else { shown },
        line_box(
            Rect::new(
                search.x + 52.,
                search.y,
                if has_text { 196. } else { 230. },
                search.h,
            ),
            style,
        ),
        style,
        c(0xeeeeee, if shown.is_empty() { 0.73 } else { 1. }),
        false,
    );
    out.control(Target::LibSearch, search, true);
    if has_text {
        let clear = Rect::new(search.x + search.w - 46., search.y + 14.8, 26.4, 26.4);
        out.shape(
            clear,
            13.2,
            c(
                0xffffff,
                if state.focus == Target::LibClear {
                    0.3
                } else {
                    0.08
                },
            ),
        );
        out.icon(assets, "close", clear.inset(5.2), WHITE);
        out.control(Target::LibClear, clear.inset(-8.), true);
    }
}

fn round_button(
    out: &mut Scene,
    state: &State,
    assets: &Assets,
    rect: Rect,
    icon: &str,
    target: Target,
) {
    let focused = state.focus == target;
    let r = rect.h / 2.;
    let ink = if focused {
        let (a, b) = state.accent_colors();
        out.draw
            .push(if state.preferences.accent.kind == "gradient" {
                Draw::DiagonalGradient(rect, r, c(a, 1.), c(b, 1.))
            } else {
                Draw::RoundedGradient(rect, r, c(a, 1.), c(a, 1.))
            });
        accent_ink(state)
    } else {
        out.glass(rect, r);
        out.shape(rect, r, c(0xffffff, 0.06));
        c(0xffffff, 0.95)
    };
    out.icon(assets, icon, rect.inset(rect.w * 0.3), ink);
    out.control(target, rect, true);
}

fn count_label(state: &State, library: &Library) -> String {
    let n = library.total.max(library.items.len());
    // In a long list: where focus is, "41 of 4 812 titles".
    let at = match state.focus {
        Target::LibCard(i) if n > 2 * COLUMNS => Some(i + 1),
        _ => None,
    };
    let noun = if library.games {
        if n == 1 { "game" } else { "games" }
    } else if library.square {
        if n == 1 { "album" } else { "albums" }
    } else if n == 1 {
        "title"
    } else {
        "titles"
    };
    if !library.query.is_empty() {
        return if library.loading && library.items.is_empty() {
            "Searching…".to_owned()
        } else {
            match at {
                Some(at) => format!("{} of {} {noun} found", thousands(at), thousands(n)),
                None => format!("{} {noun} found", thousands(n)),
            }
        };
    }
    if library.loading && library.items.is_empty() {
        return "Loading…".to_owned();
    }
    match at {
        Some(at) => format!("{} of {} {noun}", thousands(at), thousands(n)),
        None => format!("{} {noun}", thousands(n)),
    }
}

fn thousands(n: usize) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(' ');
        }
        out.push(ch);
    }
    out
}

fn grid(out: &mut Scene, state: &State, assets: &Assets, library: &Library) {
    let top = GRID_TOP - 36.;
    out.draw
        .push(Draw::Clip(Some(Rect::new(0., top, 1920., 1080. - top))));
    // Cards dissolve under the header instead of being cut.
    out.draw
        .push(Draw::EdgeFade(Some([top, GRID_TOP - 8., 2000., 2001.])));
    let (first, last) = library.visible_rows();
    let len = library.items.len();
    let cascade = !state.preferences.reduced_motion;
    let since = library.changed.elapsed().as_secs_f32();
    for row in first..=last {
        for column in 0..COLUMNS {
            let start = out.draw.len();
            let index = row * COLUMNS + column;
            let rect = Rect::new(
                GRID_LEFT + column as f32 * PITCH_X,
                library.row_y(row),
                CARD_W,
                library.card_h(),
            );
            if index < len {
                card(out, state, assets, library, index, rect);
            } else if library.loading
                && index < (len.div_ceil(COLUMNS) + 1 + usize::from(len == 0)) * COLUMNS
            {
                // The next page: quiet placeholders where its cards will be.
                out.shape(rect, CARD_RADIUS, c(0xffffff, 0.05));
                out.border(rect, CARD_RADIUS, 1., c(0xffffff, 0.05));
            }
            // A new tab or search: cards come in one after another, as in Apps.
            if cascade {
                let t = motion::cascade_progress(since, (row - first) + column);
                if t < 1. {
                    motion::transform_group(
                        &mut out.draw[start..],
                        0.,
                        motion::CARD_RISE * (1. - t),
                        t,
                    );
                }
            }
        }
    }
    if len == 0 && !library.loading {
        let (title, hint) = if library.failed() {
            (
                format!("{} did not answer", source_name(library)),
                "Trying again in a few seconds.".to_owned(),
            )
        } else if !library.query.is_empty() {
            (
                "Nothing found".to_owned(),
                format!("Nothing matches “{}”.", library.query),
            )
        } else {
            let (title, hint) = match (library.games, library.tab_id()) {
                (true, "recent") => (
                    "Nothing played yet",
                    "Games you start from Luma show up here.",
                ),
                (true, "favorites") => (
                    "No favorites yet",
                    "Press the heart on a game's page to keep it here.",
                ),
                (true, _) => (
                    "No games yet",
                    "Add games in Apollo on your PC, and they appear here.",
                ),
                (false, _) => ("Nothing here yet", "This list is empty."),
            };
            (title.to_owned(), hint.to_owned())
        };
        let style = heading_fixed(assets, 40., 600);
        out.text(
            title,
            Rect::new(0., 470., 1920., 52.),
            style,
            c(0xffffff, 0.92),
            true,
        );
        out.text(
            hint,
            Rect::new(0., 532., 1920., 34.),
            text(24., 500, 0.),
            c(0xeeeeee, 0.66),
            true,
        );
    }
    out.draw.push(Draw::EdgeFade(None));
    out.draw.push(Draw::Clip(None));
}

fn card(
    out: &mut Scene,
    state: &State,
    assets: &Assets,
    library: &Library,
    index: usize,
    rect: Rect,
) {
    let item = &library.items[index];
    let target = Target::LibCard(index);
    let f = state.focus_level(&target);
    let (pop, sheen) = state.focus_pop(&target);
    let zoom = 1. + 0.06 * pop;
    let r = Rect::new(
        rect.x - rect.w * (zoom - 1.) / 2.,
        rect.y - rect.h * (zoom - 1.) / 2.,
        rect.w * zoom,
        rect.h * zoom,
    );
    out.shadow(r, CARD_RADIUS, 26. + 14. * f, 8. + 4. * f, 0.16 + 0.1 * f);
    out.accent_glow(r, CARD_RADIUS, f, state);
    // The grey card shows until the cover is decoded (off the GL thread).
    out.shape(r, CARD_RADIUS, c(0x34373e, 1.));
    match item.poster.as_ref() {
        Some(path) => out.draw.push(Draw::TintedImage(
            format!("{path}#async"),
            r,
            CARD_RADIUS,
            false,
            [1.; 4],
        )),
        None => {
            // Only a card that will never get a poster shows its title.
            if item.poster_url.is_empty() {
                out.text(
                    item.title.as_str(),
                    Rect::new(r.x + 20., r.y + r.h - 110., r.w - 40., 85.),
                    heading_fixed(assets, 28., 600),
                    c(0xffffff, 0.95),
                    false,
                );
            }
        }
    }
    out.border(r, CARD_RADIUS, 1., c(0xffffff, 0.06));
    out.focus_ring(r, CARD_RADIUS, f, state);
    if let Some(progress) = sheen {
        out.draw
            .push(Draw::Sheen(r, CARD_RADIUS, progress, c(0xffffff, 0.14)));
    }
    // Under the header only the visible part of a card takes the pointer.
    let top = rect.y.max(GRID_TOP - 36.);
    if rect.y + rect.h > top {
        out.control(
            target,
            Rect::new(rect.x, top, rect.w, rect.y + rect.h - top),
            true,
        );
    }
}

fn source_name(library: &Library) -> &'static str {
    match library.source.as_str() {
        "jellyfin" => "Jellyfin",
        "plex" => "Plex",
        _ => "Media",
    }
}
