//! Settings › Widgets. The left panel shows the real widgets, scaled to fit,
//! instead of a drawn illustration; rows use the shared Settings cell.
use super::super::widgets_view::{self as wv, Zone};
use super::*;
use crate::{
    widgets::{self as w, Block, Kind, SettingsPage, Size},
    widgets_state::ROOT_SETTINGS_FROM,
};

const HEADING_H: f32 = 50.;
const CHIP_GAP: f32 = 16.;

fn rows_height(rows: usize, extra: f32) -> f32 {
    (2. * S32 + rows as f32 * 92. - 12. + extra).clamp(PREVIEW_MIN, MAX_HEIGHT)
}

/// The widgets a page is about, drawn at Home size, then scaled into `area`.
fn widgets_picture(
    out: &mut Scene,
    state: &State,
    assets: &Assets,
    blocks: &[Block],
    text: &[Kind],
    area: Rect,
) {
    let start = out.draw.len();
    let controls = out.controls.len();
    // Lay the blocks out on a virtual strip ending at x = 0.
    let lay = w::layout(
        blocks,
        !text.is_empty(),
        HOME_TILE_PITCH,
        HOME_TILE_WIDTH,
        0.,
    );
    let min_x = lay.blocks.iter().map(|b| b.0).fold(0f32, f32::min);
    let text_w = text
        .iter()
        .map(|k| wv::text_item_width(assets, state, *k))
        .sum::<f32>()
        + 40. * text.len().saturating_sub(1) as f32;
    let text_h = if text.is_empty() { 0. } else { 60. };
    let full_w = (-min_x).max(text_w).max(1.);
    let full_h = text_h + if blocks.is_empty() { 0. } else { w::BLOCK_H };
    if full_h <= 0. {
        return;
    }
    let k = (area.w / full_w).min(area.h / full_h).min(1.);
    let ox = area.x + (area.w - full_w * k) / 2.;
    let oy = area.y + (area.h - full_h * k) / 2.;
    // Blocks keep their right alignment; the text line starts where they do.
    let right = ox + full_w;
    let left = if blocks.is_empty() { ox } else { right + min_x };
    if !text.is_empty() {
        let z = Zone {
            blocks: Vec::new(),
            text: text.iter().copied().enumerate().collect(),
            left,
            top: oy + text_h,
        };
        wv::text_line(out, state, assets, &z);
    }
    for (b, (x, wd)) in blocks.iter().zip(&lay.blocks) {
        let r = Rect::new(right + x, oy + text_h, *wd, w::BLOCK_H);
        wv::block(out, state, assets, wv::PREVIEW, *b, r);
    }
    out.controls.truncate(controls);
    wv::scale_group(&mut out.draw[start..], ox, oy, k);
    out.keep_text.push(start..out.draw.len());
}

/// Left panel: the picture between Back and the caption.
pub(super) fn preview(
    out: &mut Scene,
    state: &State,
    assets: &Assets,
    page: Option<SettingsPage>,
    h: f32,
) {
    use SettingsPage as P;
    let prefs = &state.preferences.widgets;
    let first = out.draw.len();
    let (name, detail): (String, String) = match page {
        None | Some(P::Root) => (
            "Widgets".into(),
            "Weather, crypto, HDMI and Smart Home on Home".into(),
        ),
        Some(P::Add) => {
            let (kind, size) = state.widget_add;
            (
                format!("{} {}", kind.title(), size.label()),
                match size {
                    Size::Text => "One line above the blocks".into(),
                    s => {
                        let free = w::ZONE_COLUMNS.saturating_sub(prefs.columns_used());
                        format!(
                            "{} of Home's grid · {free} of {} free",
                            match s.columns() {
                                1 => "1 column".to_owned(),
                                n => format!("{n} columns"),
                            },
                            w::ZONE_COLUMNS
                        )
                    }
                },
            )
        }
        Some(P::Text) => ("Text line".into(), "Up to three short widgets".into()),
        Some(P::Hdmi | P::HdmiInput) => (
            "HDMI input".into(),
            "OK on the widget switches to the input".into(),
        ),
        Some(P::Smart | P::SmartCamera | P::SmartDevices | P::SmartSensor) => (
            "Smart Home".into(),
            "Camera and devices from Home Assistant".into(),
        ),
        Some(_) => (
            "Crypto".into(),
            format!("{} · {}", prefs.exchange.title(), prefs.quote),
        ),
    };
    let (mut blocks, mut line): (Vec<Block>, Vec<Kind>) = match page {
        None | Some(P::Root) => (prefs.blocks.clone(), prefs.text.clone()),
        Some(P::Text) => (Vec::new(), prefs.text.clone()),
        Some(P::Add) => match state.widget_add {
            (kind, Size::Text) => (Vec::new(), vec![kind]),
            (kind, size) => (vec![Block { kind, size }], Vec::new()),
        },
        Some(P::Hdmi | P::HdmiInput) => (
            vec![Block {
                kind: Kind::Hdmi,
                size: Size::M,
            }],
            Vec::new(),
        ),
        Some(P::Smart | P::SmartCamera | P::SmartDevices) => (
            vec![Block {
                kind: Kind::Smart,
                size: Size::M,
            }],
            Vec::new(),
        ),
        Some(P::SmartSensor) => (Vec::new(), vec![Kind::Smart]),
        Some(_) => (
            vec![Block {
                kind: Kind::Crypto,
                size: Size::M,
            }],
            Vec::new(),
        ),
    };
    // Same rule as Home: a widget without its source (no inputs, no Home
    // Assistant, weather off) is not shown.
    blocks.retain(|b| wv::available(state, b.kind));
    line.retain(|k| wv::available(state, *k));
    let title_y = TOP + h - 154.;
    let top = if page.is_some() {
        TOP + S24 + BACK_SIZE + S16
    } else {
        TOP + S32
    };
    let area = Rect::new(LEFT + S32, top, LEFT_W - 2. * S32, title_y - S24 - top);
    if blocks.is_empty() && line.is_empty() {
        out.text(
            "No widgets on Home",
            Rect::new(area.x, area.y + area.h / 2. - 22., area.w, 44.),
            text(29., 500, 0.),
            c(0xffffff, 0.55),
            true,
        );
    } else {
        widgets_picture(out, state, assets, &blocks, &line, area);
    }
    // Weather and crypto move, so the preview is never cached as static.
    out.settings_sidebar_key = 0x5749_4447_0000 + page.map_or(0, |p| p as u64 + 1);
    let text_w = LEFT_W - 2. * S24;
    out.text(
        trunc(assets, &name, text(31., 600, 0.), text_w),
        Rect::new(LEFT + S24, title_y, text_w, 44.),
        text(31., 600, 0.),
        WHITE,
        true,
    );
    out.text(
        trunc(assets, &detail, text(25., 500, 0.), text_w),
        Rect::new(LEFT + S24, title_y + 52., text_w, 40.),
        text(25., 500, 0.),
        c(0xffffff, 0.68),
        true,
    );
    out.settings_sidebar_range = first..out.draw.len();
}

fn size_chips(out: &mut Scene, state: &State, _assets: &Assets, y: f32) {
    let (kind, chosen) = state.widget_add;
    let x0 = RIGHT + S32;
    let wd = (WIDTH - 2. * S32 - 3. * CHIP_GAP) / 4.;
    for (k, size) in Size::ALL.iter().enumerate() {
        let r = Rect::new(x0 + k as f32 * (wd + CHIP_GAP), y, wd, ROW_HEIGHT);
        let target = Target::WidgetSize(k);
        let allowed = state.widget_add_allowed(kind, *size);
        let start = out.draw.len();
        let t = state.focus_level(&target);
        // Grey chip; the chosen size has an accent outline, focus fills it.
        out.shape(r, r.h / 2., c(0xffffff, 0.07));
        wv::choice(out, state, r, r.h / 2., *size == chosen, 0.);
        let ink = focus_fill(out, state, r, r.h / 2., t);
        let color = blend(WHITE, ink, t);
        out.text(
            size.label(),
            Rect::new(r.x, r.y + (r.h - 42.) / 2., r.w, 42.),
            text(31., 600, 0.),
            color,
            true,
        );
        if !allowed {
            out.fade(start, 0.35, 1.);
        }
        out.control(target, r, false);
    }
}

pub(super) fn render(out: &mut Scene, state: &State, assets: &Assets, page: SettingsPage) {
    let rows = state.widget_settings_rows(page);
    let root = page == SettingsPage::Root;
    let add = page == SettingsPage::Add;
    let height = rows_height(
        rows.len() + usize::from(add),
        if root { HEADING_H } else { 0. },
    );
    panels(out, height);
    preview(out, state, assets, Some(page), height);
    back_button(out, state, assets);
    let scrolls = 2. * S32 + rows.len() as f32 * 92. > MAX_HEIGHT;
    if scrolls {
        clip_list(out);
    }
    for (i, row) in rows.iter().enumerate() {
        // Add: the size chips take the row before "Add to Home".
        let slot = if add && i + 1 == rows.len() { i + 1 } else { i };
        let mut r = if scrolls {
            scrolled_cell(state, slot)
        } else {
            list_cell(slot)
        };
        if root && i >= ROOT_SETTINGS_FROM {
            if i == ROOT_SETTINGS_FROM {
                out.text(
                    "WIDGET SETTINGS",
                    Rect::new(r.x + 32., r.y + 6., r.w - 64., 36.),
                    text(25., 600, 0.04),
                    c(0xffffff, 0.55),
                    false,
                );
            }
            r.y += HEADING_H;
        }
        let start = out.draw.len();
        cell(
            out,
            state,
            assets,
            Target::WidgetRow(i),
            r,
            CellContent {
                name: &row.label,
                value: row.value.as_deref(),
                desc: None,
                chevron: row.chevron,
                checked: row.checked,
                partial: false,
            },
        );
        if row.dim {
            out.fade(start, 0.45, 1.);
        }
    }
    if add {
        size_chips(out, state, assets, list_cell(rows.len() - 1).y);
    }
    if scrolls {
        out.draw.push(Draw::Clip(None));
    }
}
