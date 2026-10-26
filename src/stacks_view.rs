//! Home stack rows under Favorites. Cards are flat (no text, no gradients),
//! clipped by the bottom of the screen, and enter one after another.
use super::*;
use crate::stacks::{CardShape, StackItem};

const CARD_RADIUS: f32 = crate::view::TILE_RADIUS;

fn rows(state: &State) -> Vec<(String, &'static str, CardShape, Vec<StackItem>, bool)> {
    let mut rows = Vec::new();
    for (i, stack) in state.preferences.stacks.iter().enumerate() {
        if !stack.show {
            continue;
        }
        let mut items = state.stack_items.get(i).cloned().unwrap_or_default();
        if items.is_empty() {
            continue;
        }
        items.truncate(crate::stacks::HOME_ROW_MAX);
        rows.push((
            stack.name.clone(),
            source_label(&stack.source),
            stack.shape,
            items,
            crate::stacks::has_library(&stack.source),
        ));
    }
    // Desktop-only design QA: LUMA_STACK_PREVIEW=1 shows placeholder "New" and "Popular" rows.
    #[cfg(debug_assertions)]
    if rows.is_empty() && std::env::var_os("LUMA_STACK_PREVIEW").is_some() {
        rows.push((
            "Games".into(),
            "Moonlight",
            CardShape::Poster,
            vec![StackItem::default(); 12],
            true,
        ));
        rows.push((
            "Popular Movies".into(),
            "Media",
            CardShape::Poster,
            vec![StackItem::default(); 12],
            true,
        ));
    }
    rows
}

pub(super) fn home_stacks(out: &mut Scene, state: &State, assets: &Assets, top: f32) {
    let mut y = top;
    for (row, (name, source, shape, items, library)) in rows(state).into_iter().enumerate() {
        if y > 1080. {
            // Off screen: not drawn, but the page can still scroll down to it.
            y += SECTION_TITLE_H + SECTION_TITLE_GAP + shape.size().1 + 48.;
            continue;
        }
        let row_start = out.draw.len();
        let title_start = out.draw.len();
        stack_title(out, assets, &name, source, y);
        out.enter(title_start, state, 0.36 + 0.12 * row as f32, 22.);
        let (w, h) = shape.size();
        let card_y = y + SECTION_TITLE_H + SECTION_TITLE_GAP;
        let shift = if state.stack_scroll.0 == row {
            state.stack_scroll.1
        } else {
            0.
        } + state.stack_return_shift(row)
            + state.bounce_at(crate::state::BounceAxis::Stack(row));
        let count = items.len() + usize::from(library);
        for index in 0..count {
            let item = items.get(index);
            let x = crate::state::HOME_STRIP_LEFT + index as f32 * shape.pitch() - shift;
            if x > 1920. {
                break;
            }
            if x + w < 0. {
                continue;
            }
            let target = Target::StackCard(row, index);
            let rect = Rect::new(x, card_y, w, h);
            let start = out.draw.len();
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
            if item.is_none() {
                library_card(out, assets, r, f);
            } else if let Some(path) = item.and_then(|i| i.poster.as_ref()) {
                out.draw.push(Draw::TintedImage(
                    path.clone(),
                    r,
                    CARD_RADIUS,
                    false,
                    [1.; 4],
                ));
            } else {
                out.shape(
                    r,
                    CARD_RADIUS,
                    if index % 2 == 0 {
                        c(0x3a3d44, 1.)
                    } else {
                        c(0x34373e, 1.)
                    },
                );
            }
            if item.is_some_and(|it| it.poster.is_none()) {
                let it = item.unwrap();
                out.text(
                    &it.title,
                    Rect::new(r.x + 20., r.y + r.h - 110., r.w - 40., 85.),
                    heading_fixed(assets, 28., 800),
                    c(0xffffff, 0.95),
                    false,
                );
            }
            out.border(r, CARD_RADIUS, 1., c(0xffffff, 0.06));
            out.focus_ring(r, CARD_RADIUS, f, state);
            if let Some(progress) = sheen {
                out.draw
                    .push(Draw::Sheen(r, CARD_RADIUS, progress, c(0xffffff, 0.14)));
            }
            // Scrollable: focusing a row lifts Home until the whole card is visible.
            out.control(target, rect, true);
            out.input_cascade(start, state, index + row * 2);
        }
        // Clean wallpaper mode: rows drop away one after another.
        // Row after row, each card a beat after the one to its left.
        crate::view::ambient_move(
            &mut out.draw[row_start..],
            state,
            0.,
            (1180. - y).max(400.),
            |r| 6. + 1.5 * row as f32 + (r.x / 1920.).clamp(0., 1.),
        );
        y = card_y + h + 48.;
    }
    out.page_bottom = out.page_bottom.max(y - 48.);
}

/// Last card of a row: the grey View Library card that opens the whole list.
fn library_card(out: &mut Scene, assets: &Assets, r: Rect, f: f32) {
    out.glass(r, CARD_RADIUS);
    out.shape(r, CARD_RADIUS, c(0xffffff, 0.06 + 0.04 * f));
    let disc = r.h * 0.18;
    let circle = Rect::new(
        r.x + (r.w - disc) / 2.,
        r.y + r.h / 2. - disc - 14.,
        disc,
        disc,
    );
    out.shape(circle, disc / 2., c(0xffffff, 0.12));
    out.icon(assets, "grid", circle.inset(disc * 0.24), c(0xffffff, 0.95));
    let style = heading_fixed(assets, 32., 600).tracking(-0.01);
    out.text(
        "View Library",
        Rect::new(r.x, r.y + r.h / 2. + 14., r.w, 44.),
        style,
        c(0xffffff, 0.94),
        true,
    );
}

/// Where a row comes from, as people know it: media rows are media lists.
fn source_label(source: &str) -> &'static str {
    match source {
        "media" => "Media",
        "moonlight" => "Moonlight",
        "jellyfin" => "Jellyfin",
        "plex" => "Plex",
        _ => "",
    }
}

/// Row headline with its source beside it, quieter than the headline.
fn stack_title(out: &mut Scene, assets: &Assets, name: &str, source: &str, y: f32) {
    section_title(out, assets, name, y);
    if source.is_empty() {
        return;
    }
    let head = heading_fixed(assets, 31., 700).tracking(-0.01);
    let x = 84. + assets.width(name, head);
    let variant = std::env::var("LUMA_SOURCE_STYLE").unwrap_or_default();
    match variant.as_str() {
        "b" => {
            let style = TextStyle::new(20.).weight(600);
            let w = assets.width(source, style) + 32.;
            let pill = Rect::new(x + 16., y + (SECTION_TITLE_H - 34.) / 2. + 2., w, 34.);
            out.glass(pill, 17.);
            out.text(
                source,
                line_box(pill, style),
                style,
                c(0xffffff, 0.78),
                true,
            );
        }
        "c" => {
            let style = TextStyle::new(18.).weight(600).tracking(0.12);
            out.text(
                source.to_uppercase(),
                Rect::new(84., y - 26., 600., 24.),
                style,
                c(0xffffff, 0.55),
                false,
            );
        }
        _ => {
            let style = heading_fixed(assets, 31., 500).tracking(-0.01);
            out.text(
                format!("  ·  {source}"),
                Rect::new(x, y, 600., SECTION_TITLE_H),
                style,
                c(0xffffff, 0.5),
                false,
            );
        }
    }
}
