use super::settings_art::{self, Art};
use super::*;
use crate::state::SettingsLevel;
#[path = "widgets_settings_view.rs"]
mod widgets_settings;
// Unified living-room geometry. All panel/component spacing uses the 8px scale.
pub(super) const S8: f32 = 8.;
pub(super) const S16: f32 = 16.;
pub(super) const S24: f32 = 24.;
pub(super) const S32: f32 = 32.;
pub(super) const S48: f32 = 48.;
pub(super) const S64: f32 = 64.;
// Home, Apps and Settings share exactly the same horizontal safe area.
pub(super) const EDGE: f32 = crate::state::HOME_STRIP_LEFT;
pub(super) const LEFT: f32 = EDGE;
pub(super) const LEFT_W: f32 = 700.;
pub(super) const RIGHT: f32 = LEFT + LEFT_W + S48;
pub(super) const WIDTH: f32 = 1920. - EDGE - RIGHT;
pub(super) const TOP: f32 = 184.;
pub(super) const MAX_HEIGHT: f32 = 808.;
pub(super) const ROW_HEIGHT: f32 = 80.;
pub(super) const STEP: f32 = ROW_HEIGHT + S8;
pub(super) const PANEL_RADIUS: f32 = 28.;
pub(super) const CELL_RADIUS: f32 = 16.;
pub(super) const MEDIA_RADIUS: f32 = 12.;
// Root: 32 top + six 80px rows + five 12px gaps + 32 bottom.
const ROOT_HEIGHT: f32 = 2. * S32 + 8. * ROW_HEIGHT + 7. * 12.;
const PREVIEW_MIN: f32 = 568.;
const BACK_SIZE: f32 = 56.;

// Only the two permanent glass backdrops take this signature. They never rise,
// slide, fade or zoom with content; height may ease when navigating.
pub(crate) fn backdrop_rect(draw: &Draw) -> Option<Rect> {
    let (r, radius, top) = match draw {
        Draw::Shape(r, radius, _, _) => (r, radius, TOP),
        Draw::Shadow(r, radius, _, _) => (r, radius, TOP + S8),
        _ => return None,
    };
    if (*radius - PANEL_RADIUS).abs() > 0.01 || (r.y - top).abs() > 0.01 {
        return None;
    }
    let standard_left = (r.x - LEFT).abs() < 0.01 && (r.w - LEFT_W).abs() < 0.01;
    let standard_right = (r.x - RIGHT).abs() < 0.01 && (r.w - WIDTH).abs() < 0.01;
    let category = (r.x - LEFT).abs() < 0.01 && (r.w - (1920. - 2. * EDGE)).abs() < 0.01;
    (standard_left || standard_right || category).then_some(*r)
}
fn glass(out: &mut Scene, r: Rect) {
    out.shadow(r, PANEL_RADIUS, S24, S8, 0.20);
    out.glass(r, PANEL_RADIUS);
}
fn panels(out: &mut Scene, requested: f32) {
    let h = requested.clamp(PREVIEW_MIN, MAX_HEIGHT);
    glass(out, Rect::new(LEFT, TOP, LEFT_W, h));
    glass(out, Rect::new(RIGHT, TOP, WIDTH, h));
}
/// A list row moved by the page scroll, for lists that can outgrow the panel.
fn scrolled_cell(state: &State, index: usize) -> Rect {
    let r = list_cell(index);
    Rect::new(r.x, r.y - state.scroll, r.w, r.h)
}
/// Rows that scroll stay inside the right glass panel.
fn clip_list(out: &mut Scene) {
    out.draw.push(Draw::Clip(Some(Rect::new(
        RIGHT + S16,
        TOP + S16,
        WIDTH - S32,
        MAX_HEIGHT - S32,
    ))));
}
fn list_cell(index: usize) -> Rect {
    Rect::new(
        RIGHT + S32,
        TOP + S32 + index as f32 * 92.,
        WIDTH - 2. * S32,
        ROW_HEIGHT,
    )
}
fn trunc(assets: &Assets, value: &str, style: TextStyle, w: f32) -> String {
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
pub(super) fn ellipsis_for_smart(
    assets: &Assets,
    value: &str,
    style: TextStyle,
    width: f32,
) -> String {
    trunc(assets, value, style, width)
}
fn focus(state: &State, target: &Target, r: Rect, amplitude: f32) -> (Rect, f32) {
    let t = state.focus_level(target);
    let _ = (state, amplitude);
    let scale = 1.;
    (
        Rect::new(
            r.x - r.w * (scale - 1.) / 2.,
            r.y - r.h * (scale - 1.) / 2.,
            r.w * scale,
            r.h * scale,
        ),
        t,
    )
}
fn focus_ink(state: &State) -> Color {
    if state.preferences.accent.use_focus {
        accent_ink(state)
    } else {
        c(0x15171b, 1.)
    }
}
/// Focused Settings surface: one solid accent fill (no white cell, no outline).
/// Returns the ink to blend toward. Without accent focus the cell turns white.
pub(super) fn focus_fill(out: &mut Scene, state: &State, r: Rect, radius: f32, t: f32) -> Color {
    if t <= 0. {
        return if state.preferences.accent.use_focus {
            accent_ink(state)
        } else {
            c(0x15171b, 1.)
        };
    }
    out.shadow(r, radius, S24, S8, t * 0.3);
    if state.preferences.accent.use_focus {
        let start = out.draw.len();
        accent_fill(out, r, radius, state);
        out.fade(start, t, 1.);
        accent_ink(state)
    } else {
        out.shape(r, radius, c(0xffffff, t));
        c(0x15171b, 1.)
    }
}
fn swatch_ring(out: &mut Scene, state: &State, r: Rect) {
    let gap = r.inset(-5.);
    let outer = r.inset(-10.);
    out.border(gap, gap.w / 2., 5., c(0x181a1e, 1.));
    if state.preferences.accent.use_focus {
        let (a, b) = state.accent_colors();
        out.draw.push(Draw::DiagonalGradientBorder(
            outer,
            outer.w / 2.,
            4.,
            c(a, 1.),
            c(b, 1.),
        ));
    } else {
        out.border(outer, outer.w / 2., 4., WHITE);
    }
}
fn dot(out: &mut Scene, state: &State, x: f32, y: f32, selected: bool, partial: bool, dark: bool) {
    let r = Rect::new(x, y, 36., 36.);
    // Only the focused row is filled: on a plain row the mark is an accent
    // ring with a white tick; on the focused accent row it turns dark.
    let fg = if dark { c(0x15171b, 1.) } else { WHITE };
    if (selected || partial) && !dark {
        crate::view::accent_outline(out, state, r, r.w / 2.);
    } else {
        out.border(r, r.w / 2., 3., fg);
    }
    if selected || partial {
        out.text(
            if selected { "✓" } else { "−" },
            Rect::new(x, y - 1., 36., 38.),
            text(29., 600, 0.),
            fg,
            true,
        );
    }
}
/// What a settings cell shows: its label, optional value and description,
/// and the trailing chevron and check marks.
pub(super) struct CellContent<'a> {
    pub name: &'a str,
    pub value: Option<&'a str>,
    pub desc: Option<&'a str>,
    pub chevron: bool,
    pub checked: bool,
    pub partial: bool,
}
pub(super) fn cell(
    out: &mut Scene,
    state: &State,
    assets: &Assets,
    target: Target,
    r: Rect,
    content: CellContent,
) {
    let CellContent {
        name,
        value,
        desc,
        chevron,
        checked,
        partial,
    } = content;
    let (active, t) = focus(state, &target, r, 0.04);
    out.shape(r, CELL_RADIUS, c(0xffffff, 0.07));
    if checked && t < 0.5 {
        crate::view::accent_outline(out, state, r, CELL_RADIUS);
    }
    let ink = focus_fill(out, state, active, CELL_RADIUS, t);
    let primary = blend(WHITE, ink, t);
    let secondary = blend(c(0xffffff, 0.62), [ink[0], ink[1], ink[2], 0.62], t);
    let label_w = if value.is_some() {
        (r.w - 390.).max(90.)
    } else {
        (r.w - 108.).max(90.)
    };
    out.text(
        trunc(assets, name, text(31., 500, -0.015), label_w),
        Rect::new(
            r.x + S32,
            if desc.is_some() {
                r.y + 17.
            } else {
                r.y + (r.h - 42.) / 2.
            },
            label_w,
            42.,
        ),
        text(31., 500, -0.015),
        primary,
        false,
    );
    if let Some(desc) = desc {
        let baseline = r.y + if r.h > 90. { 61. } else { 47. };
        if state.section == crate::state::INPUTS_SECTION
            && desc.ends_with("Connected")
            && !desc.contains("No device connected")
        {
            let prefix = desc.strip_suffix("Connected").unwrap_or(desc);
            let style = text(25., 500, 0.);
            let prefix = trunc(assets, prefix, style, r.w - 145.);
            out.text(
                &prefix,
                Rect::new(r.x + S32, baseline, r.w - 120., 34.),
                style,
                secondary,
                false,
            );
            let start = r.x + S32 + assets.width(&prefix, style);
            out.text(
                "Connected",
                Rect::new(start, baseline, 170., 34.),
                style,
                c(0x30d158, 1.),
                false,
            );
        } else {
            out.text(
                trunc(assets, desc, text(25., 500, 0.), r.w - 120.),
                Rect::new(r.x + S32, baseline, r.w - 120., 34.),
                text(25., 500, 0.),
                secondary,
                false,
            );
        }
    }
    if let Some(value) = value {
        let trailing = chevron || checked || partial;
        let value_w = if trailing { 295. } else { 325. };
        out.text(
            trunc(assets, value, text(29., 500, 0.), value_w),
            Rect::new(
                r.x + r.w - value_w - if trailing { 83. } else { 29. },
                r.y + 19.,
                value_w,
                42.,
            ),
            text(29., 500, 0.).right(),
            secondary,
            false,
        );
    }
    if checked || partial {
        dot(
            out,
            state,
            r.x + r.w - if chevron { 94. } else { 64. },
            r.y + 22.,
            checked,
            partial,
            t > 0.5,
        );
    }
    if chevron {
        out.text(
            "›",
            Rect::new(r.x + r.w - 51., r.y + 15., 31., 48.),
            text(37., 500, 0.),
            blend(c(0xffffff, 0.45), c(0x15171b, 0.55), t),
            true,
        );
    }
    out.control(target, r, true);
}
// No title or breadcrumbs: the permanent navigation and preview identify the page.
// The 56px Back control sits 24px inside the preview panel and never overlaps content.
fn back_button(out: &mut Scene, state: &State, assets: &Assets) {
    if state.settings_level == SettingsLevel::Root && state.subpage.is_none() {
        return;
    }
    let r = Rect::new(LEFT + S24, TOP + S24, BACK_SIZE, BACK_SIZE);
    let (active, t) = focus(state, &Target::SettingsBack, r, 0.);
    out.shadow(active, r.h / 2., S16, S8, 0.20);
    out.shape(active, r.h / 2., c(0xffffff, 0.07));
    out.border(active, r.h / 2., 1., c(0xffffff, 0.08));
    let ink = focus_fill(out, state, active, r.h / 2., t);
    out.icon(
        assets,
        "left",
        Rect::new(r.x + S16, r.y + S16, S24, S24),
        blend(WHITE, ink, t),
    );
    out.control(Target::SettingsBack, r, false);
}
pub(super) fn page_head(
    out: &mut Scene,
    state: &State,
    assets: &Assets,
    title: &str,
    _icon: &str,
    detail: &str,
) {
    // Legacy sections and subpages share exactly the same panel size.
    let height = if state.section == crate::state::SMARTHOME_SECTION {
        if state.subpage.is_some() {
            MAX_HEIGHT
        } else if state.preferences.smart_home.server_url.is_empty() {
            PREVIEW_MIN
        } else {
            744.
        }
    } else if state.section == crate::state::SYSTEM_SECTION
        && state.subpage.is_none()
        && state.root.is_some()
    {
        // Four rows, the ROOT FEATURES heading and two described rows.
        2. * S32 + 4. * 92. + 16. + 48. + 2. * 104. + 12.
    } else if state.section == crate::state::INPUTS_SECTION && state.subpage.is_none() {
        let groups = [true, false]
            .into_iter()
            .filter(|connected| {
                state
                    .inventory
                    .inputs
                    .iter()
                    .any(|input| input.connected.unwrap_or(false) == *connected)
            })
            .count() as f32;
        let requested =
            2. * S32 + 80. + groups * (48. + 32.) + state.inventory.inputs.len() as f32 * 116.;
        requested.clamp(PREVIEW_MIN, MAX_HEIGHT)
    } else if matches!(
        state.subpage,
        Some(crate::state::Subpage::WeatherLocation | crate::state::Subpage::Greeting)
    ) {
        MAX_HEIGHT
    } else {
        PREVIEW_MIN
    };
    panels(out, height);
    let first = out.draw.len();
    art_preview(
        out,
        state,
        assets,
        Art::for_section(state.section),
        title,
        detail,
        height,
    );
    out.settings_sidebar_range = first..out.draw.len();
    back_button(out, state, assets);
}

/// A wallpaper still with its caption, or the Wallpaper illustration without one.
fn wallpaper_preview(
    out: &mut Scene,
    state: &State,
    assets: &Assets,
    still: Option<usize>,
    name: &str,
    detail: &str,
    h: f32,
) {
    let Some(id) = still else {
        art_preview(out, state, assets, Art::Wallpaper, name, detail, h);
        return;
    };
    let y = TOP;
    let poster_offset = if name != "Wallpaper" {
        BACK_SIZE + S16
    } else {
        0.
    };
    let image = Rect::new(
        LEFT + S32,
        y + S32 + poster_offset,
        LEFT_W - 2. * S32,
        357.75,
    );
    out.image(
        assets.path(format!("video-stills/{id}.jpg")),
        image,
        MEDIA_RADIUS,
        false,
    );
    let label_y = if h >= 700. {
        y + h - S32 - 44. - 6. - 40.
    } else {
        image.y + image.h + S24
    };
    out.text(
        trunc(assets, name, text(31., 600, 0.), LEFT_W - 2. * S32),
        Rect::new(LEFT + S32, label_y, LEFT_W - 2. * S32, 44.),
        text(31., 600, 0.),
        WHITE,
        false,
    );
    out.text(
        trunc(assets, detail, text(25., 500, 0.), LEFT_W - 2. * S32),
        Rect::new(LEFT + S32, label_y + 44. + 6., LEFT_W - 2. * S32, 40.),
        text(25., 500, 0.),
        c(0xffffff, 0.68),
        false,
    );
}
/// The section's animated illustration with a centered caption below it.
/// The stage keeps clear of the Back control and never grows past 1:1.
fn art_preview(
    out: &mut Scene,
    state: &State,
    assets: &Assets,
    art: Art,
    name: &str,
    detail: &str,
    h: f32,
) {
    use settings_art::{STAGE_H, STAGE_W};
    let y = TOP;
    let back = !(state.settings_level == SettingsLevel::Root && state.subpage.is_none());
    let title_y = y + h - 154.;
    let bottom = title_y - S24;
    let fit = |top: f32| {
        ((bottom - top) / STAGE_H)
            .min((LEFT_W - 2. * S32) / STAGE_W)
            .min(1.)
    };
    let mut top = y + S32;
    let mut s = fit(top);
    if back && (LEFT_W - STAGE_W * s) / 2. < S24 + BACK_SIZE + S16 {
        top = y + S24 + BACK_SIZE + S16;
        s = fit(top);
    }
    let (w, sh) = (STAGE_W * s, STAGE_H * s);
    let stage = Rect::new(
        LEFT + (LEFT_W - w) / 2.,
        top + (bottom - top - sh) / 2.,
        w,
        sh,
    );
    settings_art::draw(out, state, assets, art, stage);
    out.settings_sidebar_key = settings_art::key(art, name, detail);
    let text_w = LEFT_W - 2. * S24;
    out.text(
        trunc(assets, name, text(31., 600, 0.), text_w),
        Rect::new(LEFT + S24, title_y, text_w, 44.),
        text(31., 600, 0.),
        WHITE,
        true,
    );
    out.text(
        trunc(assets, detail, text(25., 500, 0.), text_w),
        Rect::new(LEFT + S24, title_y + 52., text_w, 40.),
        text(25., 500, 0.),
        c(0xffffff, 0.68),
        true,
    );
}
fn root(out: &mut Scene, state: &State, assets: &Assets) {
    panels(out, ROOT_HEIGHT);
    let selected = match state.focus {
        Target::Sidebar(n) => n.min(7),
        _ => state.section.min(7),
    };
    let names = [
        "General",
        "Wallpaper",
        "Widgets",
        "Inputs",
        "Smart Home",
        "System",
        "Appearance",
        "Stacks",
    ];
    let hints = [
        "Language, clock and motion",
        "Video wallpaper collection",
        "Weather, crypto, HDMI and Smart Home on Home",
        "Connected HDMI devices",
        "Home Assistant devices",
        "Device and launcher controls",
        "Accent color and highlights",
        "Rows of media on Home",
    ];
    let widgets = &state.preferences.widgets;
    let shown = widgets
        .blocks
        .iter()
        .map(|b| b.kind)
        .chain(widgets.text.iter().copied())
        .filter(|k| super::widgets_view::available(state, *k))
        .count();
    let on_home = match shown {
        0 => "Off".to_owned(),
        n => format!("{n} on Home"),
    };
    let values = [
        None,
        Some(if state.preferences.background == "video" {
            "Video"
        } else {
            "Photo"
        }),
        Some(on_home.as_str()),
        None,
        None,
        Some(assets.version.trim()),
        Some(state.preferences.accent.id.as_str()),
        None,
    ];
    let preview_start = out.draw.len();
    if selected == crate::state::WEATHER_SECTION {
        widgets_settings::preview(out, state, assets, None, ROOT_HEIGHT);
    } else {
        art_preview(
            out,
            state,
            assets,
            Art::for_section(selected),
            names[selected],
            hints[selected],
            ROOT_HEIGHT,
        );
    }
    out.settings_sidebar_range = preview_start..out.draw.len();
    // Appearance follows Wallpaper visually; IDs of other sections remain
    // unchanged so all existing routes retain their saved mappings.
    for (pos, &i) in crate::state::SECTION_ORDER.iter().enumerate() {
        cell(
            out,
            state,
            assets,
            Target::Sidebar(i),
            list_cell(pos),
            CellContent {
                name: names[i],
                value: values[i],
                desc: None,
                chevron: true,
                checked: false,
                partial: false,
            },
        );
    }
}
fn wallpaper(out: &mut Scene, state: &State, assets: &Assets) {
    let h = 640.;
    panels(out, h);
    let selected = crate::video_catalog::selected(&state.preferences.video_excluded);
    let items = [
        (
            Target::WallpaperMenu(0),
            "Type",
            if state.preferences.background == "video" {
                "Video".to_owned()
            } else {
                "Photo".to_owned()
            },
            true,
        ),
        (
            Target::WallpaperMenu(2),
            "Videos",
            format!(
                "{} of {}",
                selected.len(),
                crate::video_catalog::all().len()
            ),
            true,
        ),
        (
            Target::VideoShuffle,
            "Shuffle",
            if state.preferences.video_shuffle {
                "On".to_owned()
            } else {
                "Off".to_owned()
            },
            false,
        ),
        (Target::VideoNext, "Play Next Clip", String::new(), true),
    ];
    for (i, (target, name, value, arrow)) in items.into_iter().enumerate() {
        cell(
            out,
            state,
            assets,
            target,
            list_cell(i),
            CellContent {
                name,
                value: if value.is_empty() { None } else { Some(&value) },
                desc: None,
                chevron: arrow,
                checked: false,
                partial: false,
            },
        );
    }
    let help_y = TOP + S24 + 4. * STEP + S16;
    out.shape(
        Rect::new(RIGHT + S24, help_y, WIDTH - S48, 120.),
        CELL_RADIUS,
        c(0xffffff, 0.07),
    );
    out.text(
        "Choose whole sources, single categories or individual clips.",
        Rect::new(RIGHT + S48, help_y + S16, WIDTH - S48 - S48, 40.),
        text(25., 500, 0.),
        c(0xffffff, 0.74),
        false,
    );
    out.text(
        "Only selected clips play.",
        Rect::new(RIGHT + S48, help_y + S64, WIDTH - S48 - S48, 40.),
        text(25., 500, 0.),
        c(0xffffff, 0.74),
        false,
    );
    let preview_start = out.draw.len();
    let thumb_w = (LEFT_W - S48 - 2. * S8) / 3.;
    let thumb_h = thumb_w * 9. / 16.;
    for (pos, index) in selected.iter().take(6).enumerate() {
        out.image(
            assets.path(format!("video-thumbnails/{index}.jpg")),
            Rect::new(
                LEFT + S24 + (pos % 3) as f32 * (thumb_w + S8),
                TOP + S24 + BACK_SIZE + S16 + (pos / 3) as f32 * (thumb_h + S16),
                thumb_w,
                thumb_h,
            ),
            MEDIA_RADIUS,
            false,
        );
    }
    let summary_y = TOP + S24 + BACK_SIZE + S16 + 2. * (thumb_h + S16) + S16;
    out.text(
        format!("{} clips in rotation", selected.len()),
        Rect::new(LEFT + S24, summary_y, LEFT_W - S48, 48.),
        text(31., 600, 0.),
        WHITE,
        false,
    );
    for (i, (label, source)) in crate::video_catalog::FILTERS.iter().skip(1).enumerate() {
        let count = selected
            .iter()
            .filter(|&&j| crate::video_catalog::all()[j].source == *source)
            .count();
        out.text(
            format!("{label}: {count}"),
            Rect::new(
                LEFT + S24,
                summary_y + S48 + i as f32 * 40.,
                LEFT_W - S48,
                38.,
            ),
            text(25., 500, 0.),
            c(0xffffff, 0.68),
            false,
        );
    }
    out.settings_sidebar_range = preview_start..out.draw.len();
}
fn videos(out: &mut Scene, state: &State, assets: &Assets) {
    panels(out, MAX_HEIGHT);
    let catalog = crate::video_catalog::all();
    let groups = state.video_groups_for_filter(0);
    let top = TOP + S32;
    let mut y = top - state.scroll;
    let row_x = RIGHT + S32;
    let row_width = WIDTH - 2. * S32;
    let pitch = ROW_HEIGHT + 12.;
    out.draw.push(Draw::Clip(Some(Rect::new(
        RIGHT + S8,
        TOP + S8,
        WIDTH - 2. * S8,
        MAX_HEIGHT - 2. * S8,
    ))));
    for (filter, (display, source)) in crate::video_catalog::FILTERS.iter().enumerate().skip(1) {
        let categories = groups
            .iter()
            .enumerate()
            .filter(|(_, g)| g.0 == *source)
            .collect::<Vec<_>>();
        if categories.is_empty() {
            continue;
        }
        let indices = crate::video_catalog::display_filter(filter);
        let selected = indices
            .iter()
            .filter(|&&i| !state.preferences.video_excluded.contains(&catalog[i].id))
            .count();
        let all = selected == indices.len();
        let partial = selected > 0 && !all;
        let block_y = y - S16;
        let block_h = ROW_HEIGHT + categories.len() as f32 * pitch - 12. + S32;
        out.shape(
            Rect::new(RIGHT + S16, block_y, WIDTH - S32, block_h),
            CELL_RADIUS,
            c(0xffffff, 0.045),
        );
        // Provider row is the ONLY control for selecting an entire source.
        let r = Rect::new(row_x, y, row_width, ROW_HEIGHT);
        let target = Target::VideoSource(filter);
        let t = state.focus_level(&target);
        // The same focus as every Settings row: the accent fill, never white.
        out.shape(r, CELL_RADIUS, c(0xffffff, 0.07));
        let ink = focus_fill(out, state, r, CELL_RADIUS, t);
        out.text(
            *display,
            Rect::new(r.x + S32, r.y + 18., r.w - 320., 44.),
            text(31., 500, 0.),
            blend(WHITE, ink, t),
            false,
        );
        out.text(
            format!("{selected} of {}", indices.len()),
            Rect::new(r.x + r.w - 300., r.y + 19., 205., 42.),
            text(29., 500, 0.).right(),
            blend(c(0xffffff, 0.70), [ink[0], ink[1], ink[2], 0.65], t),
            false,
        );
        dot(
            out,
            state,
            r.x + r.w - 66.,
            r.y + 22.,
            all,
            partial,
            t > 0.5,
        );
        out.control(target, r, true);
        y += pitch;
        for (group_index, (_, category, items)) in categories {
            let count = items
                .iter()
                .filter(|&&i| !state.preferences.video_excluded.contains(&catalog[i].id))
                .count();
            let rect = Rect::new(row_x, y, row_width, ROW_HEIGHT);
            let target = Target::VideoGroup(group_index);
            let t = state.focus_level(&target);
            if y + ROW_HEIGHT > TOP && y < TOP + MAX_HEIGHT {
                out.shape(rect, CELL_RADIUS, c(0xffffff, 0.07));
                let ink = focus_fill(out, state, rect, CELL_RADIUS, t);
                out.image(
                    assets.path(format!(
                        "video-thumbnails/{}.jpg",
                        items.first().copied().unwrap_or(0)
                    )),
                    Rect::new(rect.x + S24, rect.y + 13., 96., 54.),
                    10.,
                    false,
                );
                out.text(
                    trunc(assets, category, text(31., 500, 0.), 340.),
                    Rect::new(rect.x + 140., rect.y + 18., 340., 44.),
                    text(31., 500, 0.),
                    blend(WHITE, ink, t),
                    false,
                );
                let counter = if count == items.len() {
                    format!("✓ {count} of {}", items.len())
                } else {
                    format!("{count} of {}", items.len())
                };
                out.text(
                    counter,
                    Rect::new(rect.x + rect.w - 275., rect.y + 20., 220., 42.),
                    text(29., 500, 0.).right(),
                    blend(
                        c(0xffffff, if count == 0 { 0.40 } else { 0.90 }),
                        [ink[0], ink[1], ink[2], 0.65],
                        t,
                    ),
                    false,
                );
                out.text(
                    "›",
                    Rect::new(rect.x + rect.w - 44., rect.y + 17., 26., 46.),
                    text(35., 500, 0.),
                    blend(c(0xffffff, 0.65), [ink[0], ink[1], ink[2], 0.75], t),
                    true,
                );
            }
            out.control(target, rect, true);
            y += pitch;
        }
        y += S32;
    }
    out.draw.push(Draw::Clip(Some(Rect::new(
        0.,
        0.,
        1920.,
        state.main_bottom(),
    ))));
    let chosen = match state.focus {
        Target::VideoSource(filter) => groups.iter().find(|g| {
            g.0 == crate::video_catalog::FILTERS
                .get(filter)
                .map(|v| v.1)
                .unwrap_or("")
        }),
        Target::VideoGroup(i) => groups.get(i),
        _ => groups.first(),
    };
    if let Some((source, name, items)) = chosen {
        let count = items
            .iter()
            .filter(|&&i| !state.preferences.video_excluded.contains(&catalog[i].id))
            .count();
        let preview_start = out.draw.len();
        wallpaper_preview(
            out,
            state,
            assets,
            items.first().copied(),
            &format!("{source} · {name}"),
            &format!("{count} of {} selected", items.len()),
            MAX_HEIGHT,
        );
        out.settings_sidebar_range = preview_start..out.draw.len();
    }
}

fn font_gallery(out: &mut Scene, state: &State, assets: &Assets, heading: bool) {
    use crate::typography::{HeadingFont, TextFont};
    panels(out, MAX_HEIGHT);
    let first = out.draw.len();
    // One centered stack, like every other preview: live specimen, then the
    // setting name and the chosen font. No icon, so nothing can overlap it.
    let w = LEFT_W - 2. * S32;
    let x = LEFT + S32;
    let (big, big_style, small, small_style) = if heading {
        (
            "Good Evening",
            TextStyle::new(61.).weight(800).display(true).leading(1.15),
            "A beautiful day on your television",
            TextStyle::new(27.).weight(500).display(false).leading(1.3),
        )
    } else {
        (
            "21:45 · 18°",
            TextStyle::new(51.).weight(700).display(true).leading(1.15),
            "Netflix · YouTube · HDMI 1",
            TextStyle::new(30.).weight(500).display(false).leading(1.3),
        )
    };
    let big_h = big_style.size * big_style.line_height;
    let small_h = small_style.size * small_style.line_height;
    let title_h = 44.;
    let detail_h = 40.;
    let group_h = big_h + S8 + small_h + S48 + title_h + S8 + detail_h;
    let mut y = TOP + (MAX_HEIGHT - group_h) / 2.;
    out.text(big, Rect::new(x, y, w, big_h + 2.), big_style, WHITE, true);
    y += big_h + S8;
    out.text(
        trunc(assets, small, small_style, w),
        Rect::new(x, y, w, small_h + 2.),
        small_style,
        c(0xffffff, 0.86),
        true,
    );
    y += small_h + S48;
    out.text(
        if heading { "Heading font" } else { "Text font" },
        Rect::new(x, y, w, title_h),
        text(31., 600, 0.),
        WHITE,
        true,
    );
    out.text(
        if heading {
            state.preferences.heading_font.name()
        } else {
            state.preferences.text_font.name()
        },
        Rect::new(x, y + title_h + S8, w, detail_h),
        text(25., 500, 0.),
        c(0xffffff, 0.68),
        true,
    );
    out.settings_sidebar_range = first..out.draw.len();
    out.draw.push(Draw::Clip(Some(Rect::new(
        RIGHT + S16,
        TOP + S16,
        WIDTH - S32,
        MAX_HEIGHT - S32,
    ))));
    if heading {
        for (i, &font) in HeadingFont::ALL.iter().enumerate() {
            let r = list_cell(i);
            let r = Rect::new(r.x, r.y - state.scroll, r.w, r.h);
            let target = Target::HeadingFontPick(i);
            cell(
                out,
                state,
                assets,
                target.clone(),
                r,
                CellContent {
                    name: "",
                    value: None,
                    desc: None,
                    chevron: false,
                    checked: font == state.preferences.heading_font,
                    partial: false,
                },
            );
            let style = TextStyle::new(34.).weight(700).display(true).heading(font);
            let color = blend(WHITE, focus_ink(state), state.focus_level(&target));
            out.text(
                trunc(assets, font.name(), style, r.w - 146.),
                Rect::new(r.x + S32, r.y + 16., r.w - 146., 48.),
                style,
                color,
                false,
            );
        }
    } else {
        for (i, &font) in TextFont::ALL.iter().enumerate() {
            let r = list_cell(i);
            let r = Rect::new(r.x, r.y - state.scroll, r.w, r.h);
            let target = Target::TextFontPick(i);
            cell(
                out,
                state,
                assets,
                target.clone(),
                r,
                CellContent {
                    name: "",
                    value: None,
                    desc: None,
                    chevron: false,
                    checked: font == state.preferences.text_font,
                    partial: false,
                },
            );
            let style = TextStyle::new(31.).weight(500).display(false).body(font);
            let color = blend(WHITE, focus_ink(state), state.focus_level(&target));
            out.text(
                trunc(assets, font.name(), style, r.w - 146.),
                Rect::new(r.x + S32, r.y + 17., r.w - 146., 46.),
                style,
                color,
                false,
            );
        }
    }
    out.draw.push(Draw::Clip(None));
}

fn ui_resolution_gallery(out: &mut Scene, state: &State, assets: &Assets) {
    use crate::model::UiResolution;
    panels(out, MAX_HEIGHT);
    let start = out.draw.len();
    art_preview(
        out,
        state,
        assets,
        Art::Appearance,
        "UI resolution",
        &format!("Now {}", state.preferences.ui_resolution_value()),
        MAX_HEIGHT,
    );
    out.settings_sidebar_range = start..out.draw.len();
    for (i, choice) in UiResolution::ALL.iter().copied().enumerate() {
        let target = Target::UiResolutionPick(i);
        let current = if choice == UiResolution::Auto {
            state.preferences.ui_resolution_value()
        } else {
            choice.label().into()
        };
        cell(
            out,
            state,
            assets,
            target,
            list_cell(i),
            CellContent {
                name: &current,
                value: None,
                desc: None,
                chevron: false,
                checked: state.preferences.ui_resolution == choice,
                partial: false,
            },
        );
    }
}

fn appearance(out: &mut Scene, state: &State, assets: &Assets) {
    use crate::model::{ACCENT_GRADIENTS, ACCENT_SOLIDS};
    // The glass panel never exceeds the 1080p safe area; ONLY its content scrolls.
    panels(out, MAX_HEIGHT);
    let accent = &state.preferences.accent;
    let (first, second) = accent.colors();
    let x = RIGHT + S32;
    let y = TOP + S32 - state.scroll;
    out.draw.push(Draw::Clip(Some(Rect::new(
        RIGHT + S16,
        TOP + S16,
        WIDTH - S32,
        MAX_HEIGHT - S32,
    ))));
    // Seven equal intervals fill the right glass exactly, leaving 32px
    // at each edge in BOTH rows.
    let swatch_step = (WIDTH - 2. * S32 - 72.) / 7.;
    let clip_top = TOP + S16 + 12.;
    let row_alpha = |row_y: f32| ((row_y - clip_top) / 24.).clamp(0., 1.);
    let color_start = out.draw.len();
    out.text(
        "COLOR",
        Rect::new(x + S8, y, WIDTH - 2. * S32, 40.),
        text(25., 600, 0.03),
        c(0xffffff, 0.55),
        false,
    );
    for (index, (name, color)) in ACCENT_SOLIDS.iter().enumerate() {
        let r = Rect::new(x + index as f32 * swatch_step, y + 58., 72., 72.);
        let selected = accent.kind == "solid" && accent.id == *name;
        let target = Target::AccentChoice(index);
        let t = if state.focus == target {
            state.focus_level(&target)
        } else {
            0.
        };
        let r = Rect::new(
            r.x - 72. * 0.06 * t,
            r.y - 72. * 0.06 * t,
            72. * (1. + 0.12 * t),
            72. * (1. + 0.12 * t),
        );
        out.shadow(r, 36., 14., 4., 0.26);
        out.shape(r, r.w / 2., c(*color, 1.));
        if t > 0. {
            swatch_ring(out, state, r);
        }
        if selected {
            out.icon(
                assets,
                "check",
                r.inset(23.),
                if *name == "White" {
                    c(0x15171b, 1.)
                } else {
                    WHITE
                },
            );
        }
        out.control(
            Target::AccentChoice(index),
            Rect::new(x + index as f32 * swatch_step, y + 58., 72., 72.),
            true,
        );
    }
    out.fade(color_start, row_alpha(y + 58. - 10.), 1.);
    let grad_y = y + 58. + 72. + 44.;
    let grad_start = out.draw.len();
    out.text(
        "GRADIENT",
        Rect::new(x + S8, grad_y, WIDTH - 2. * S32, 40.),
        text(25., 600, 0.03),
        c(0xffffff, 0.55),
        false,
    );
    for (index, (name, start, end)) in ACCENT_GRADIENTS.iter().enumerate() {
        let original = Rect::new(x + index as f32 * swatch_step, grad_y + 58., 72., 72.);
        let selected = accent.kind == "gradient" && accent.id == *name;
        let target = Target::AccentChoice(index + 8);
        // Focus ring is unique to the CURRENT target. During focus tween the
        // outgoing swatch must not retain the second white ring.
        let t = if state.focus == target {
            state.focus_level(&target)
        } else {
            0.
        };
        let size = 72. * (1. + 0.12 * t);
        let r = Rect::new(
            original.x - (size - 72.) / 2.,
            original.y - (size - 72.) / 2.,
            size,
            size,
        );
        out.shadow(r, r.w / 2., 14., 4., 0.26);
        out.draw.push(Draw::DiagonalGradient(
            r,
            r.w / 2.,
            c(*start, 1.),
            c(*end, 1.),
        ));
        if t > 0. {
            swatch_ring(out, state, r);
        }
        if selected {
            out.icon(
                assets,
                "check",
                r.inset(23.),
                crate::view::contrast_text_for_accent(*start, *end),
            );
        }
        out.control(target, original, true);
    }
    out.fade(grad_start, row_alpha(grad_y + 58. - 10.), 1.);
    // Exactly 44px between swatch row and toggle group title.
    let opt_y = grad_y + 58. + 72. + 44.;
    out.text(
        "USE ACCENT FOR",
        Rect::new(x + S8, opt_y, WIDTH - 2. * S32, 40.),
        text(25., 600, 0.03),
        c(0xffffff, 0.55),
        false,
    );
    for (i, (title, enabled)) in [
        ("Focus ring", accent.use_focus),
        ("Navigation", accent.use_nav),
        ("Headline word", accent.use_headline),
    ]
    .iter()
    .enumerate()
    {
        cell(
            out,
            state,
            assets,
            Target::AccentOption(i),
            Rect::new(x, opt_y + 44. + i as f32 * 92., WIDTH - 2. * S32, 80.),
            CellContent {
                name: title,
                value: Some(if *enabled { "On" } else { "Off" }),
                desc: None,
                chevron: false,
                checked: false,
                partial: false,
            },
        );
    }
    // Independent font selection and bounded size controls, in the SAME component system.
    for (i, label, value, target) in [
        (
            0,
            "Heading font",
            state.preferences.heading_font.name(),
            Target::HeadingFontOpen,
        ),
        (
            1,
            "Text font",
            state.preferences.text_font.name(),
            Target::TextFontOpen,
        ),
    ] {
        cell(
            out,
            state,
            assets,
            target,
            Rect::new(
                x,
                opt_y + 44. + (4 + i) as f32 * 92.,
                WIDTH - 2. * S32,
                ROW_HEIGHT,
            ),
            CellContent {
                name: label,
                value: Some(value),
                desc: None,
                chevron: true,
                checked: false,
                partial: false,
            },
        );
    }
    let heading_size = format!("{}%  •  85–170%", state.preferences.heading_size_pct);
    let text_size = format!("{}%  •  90–150%", state.preferences.text_size_pct);
    for (i, label, value, target) in [
        (
            0,
            "Heading size",
            heading_size.as_str(),
            Target::HeadingSize,
        ),
        (1, "Text size", text_size.as_str(), Target::TextSize),
    ] {
        cell(
            out,
            state,
            assets,
            target,
            Rect::new(
                x,
                opt_y + 44. + (6 + i) as f32 * 92.,
                WIDTH - 2. * S32,
                ROW_HEIGHT,
            ),
            CellContent {
                name: label,
                value: Some(value),
                desc: None,
                chevron: false,
                checked: false,
                partial: false,
            },
        );
    }
    let title_size = format!(
        "{}%  •  {}–{}%",
        state.preferences.phrase_title_pct,
        crate::model::PHRASE_PCT_MIN,
        crate::model::PHRASE_PCT_MAX
    );
    let subtitle_size = format!(
        "{}%  •  {}–{}%",
        state.preferences.phrase_subtitle_pct,
        crate::model::PHRASE_PCT_MIN,
        crate::model::PHRASE_PCT_MAX
    );
    for (i, label, value, target) in [
        (
            0,
            "Headline size",
            title_size.as_str(),
            Target::PhraseTitleSize,
        ),
        (
            1,
            "Subtitle size",
            subtitle_size.as_str(),
            Target::PhraseSubtitleSize,
        ),
    ] {
        cell(
            out,
            state,
            assets,
            target,
            Rect::new(
                x,
                opt_y + 44. + (8 + i) as f32 * 92.,
                WIDTH - 2. * S32,
                ROW_HEIGHT,
            ),
            CellContent {
                name: label,
                value: Some(value),
                desc: None,
                chevron: false,
                checked: false,
                partial: false,
            },
        );
    }
    cell(
        out,
        state,
        assets,
        Target::UiSounds,
        Rect::new(x, opt_y + 44. + 10. * 92., WIDTH - 2. * S32, ROW_HEIGHT),
        CellContent {
            name: "Navigation sounds",
            value: Some(if state.preferences.ui_sounds {
                "On"
            } else {
                "Off"
            }),
            desc: None,
            chevron: false,
            checked: false,
            partial: false,
        },
    );
    let screensaver = match state.preferences.screensaver_minutes {
        0 => "Off".to_owned(),
        m => format!("After {m} min"),
    };
    cell(
        out,
        state,
        assets,
        Target::Screensaver,
        Rect::new(x, opt_y + 44. + 11. * 92., WIDTH - 2. * S32, ROW_HEIGHT),
        CellContent {
            name: "Screensaver",
            value: Some(&screensaver),
            desc: None,
            chevron: false,
            checked: false,
            partial: false,
        },
    );
    let selected_resolution = state.preferences.ui_resolution_value();
    cell(
        out,
        state,
        assets,
        Target::UiResolutionOpen,
        Rect::new(x, opt_y + 44. + 3. * 92., WIDTH - 2. * S32, ROW_HEIGHT),
        CellContent {
            name: "UI resolution",
            value: Some(&selected_resolution),
            desc: None,
            chevron: true,
            checked: false,
            partial: false,
        },
    );
    // Clip only Appearance content; miniature preview and Back stay unaffected.
    out.draw.push(Draw::Clip(None));
    // Live 16:9 miniature with current wallpaper, tvOS navigation and Rude
    // accent highlight. The miniature never overflows the glass safe inset.
    let start = out.draw.len();
    let p = Rect::new(
        LEFT + S32,
        TOP + S32 + BACK_SIZE + S16,
        LEFT_W - 2. * S32,
        357.75,
    );
    out.image(
        assets.path(format!(
            "wallpapers/{}.jpg",
            if state.preferences.background == "video" {
                "dunes"
            } else {
                &state.preferences.background
            }
        )),
        p,
        18.,
        false,
    );
    out.shape(p, 18., c(0x000000, 0.35));
    // This is a scaled layout thumbnail, not a second full-size UI:
    // all typography and controls are sized for the 636x358 preview.
    // Settings text never goes below 25 px, so the miniature nav is laid out
    // with that same size (measured == rendered) at 0.7 of the real geometry.
    let k = 0.7;
    let nav_style = TextStyle::new(25.).weight(600).leading(1.2);
    let labels = ["Home", "Apps", "Settings"];
    let pad = 8. * k;
    let widths: Vec<f32> = labels
        .iter()
        .map(|l| (assets.width(l, nav_style) + 56. * k).max(104. * k))
        .collect();
    let bar_w = widths.iter().sum::<f32>() + 2. * pad + 4. * k * 2.;
    let bar_h = 68. * k;
    let bar = Rect::new(p.x + (p.w - bar_w) / 2., p.y + 18., bar_w, bar_h);
    out.shape(bar, bar_h / 2., c(0x151719, 0.76));
    out.border(bar, bar_h / 2., 1., c(0xffffff, 0.07));
    let mut tx = bar.x + pad;
    let pill_h = bar_h - 2. * pad;
    for (i, label) in labels.iter().enumerate() {
        let tab = Rect::new(tx, bar.y + pad, widths[i], pill_h);
        let selected = i == 2;
        let ink = if selected {
            if accent.use_nav {
                out.draw.push(Draw::DiagonalGradient(
                    tab,
                    pill_h / 2.,
                    c(first, 1.),
                    c(second, 1.),
                ));
                crate::view::contrast_text_for_accent(first, second)
            } else {
                out.shape(tab, pill_h / 2., WHITE);
                c(0x15171b, 1.)
            }
        } else {
            c(0xffffff, 0.82)
        };
        let line = nav_style.size * nav_style.line_height;
        out.text(
            *label,
            Rect::new(tab.x, tab.y + (tab.h - line) / 2., tab.w, line + 1.),
            nav_style,
            ink,
            true,
        );
        tx += widths[i] + 4. * k;
    }
    let headline_y = p.y + 112.;
    let headline_style = crate::view::heading_fixed(assets, 31., 800).tracking(-0.03);
    let lead = "It's ";
    out.text(
        lead,
        Rect::new(p.x + 32., headline_y, 90., 44.),
        headline_style,
        WHITE,
        false,
    );
    let text_x = p.x + 32. + assets.width(lead, headline_style);
    out.text(
        "fucking alright.",
        Rect::new(text_x, headline_y, p.w - (text_x - p.x) - 24., 44.),
        headline_style,
        if accent.use_headline {
            c(first, 1.)
        } else {
            WHITE
        },
        false,
    );
    // Four representative tiles at miniature scale; the accent-ring specimen
    // is the first and never extends beyond the thumbnail margins.
    for i in 0..4 {
        let tile = Rect::new(p.x + 34. + i as f32 * 145., p.y + 221., 126., 75.);
        out.shape(tile, 14., c(0x17191c, 0.84));
        if i == 0 {
            if accent.use_focus && accent.kind == "gradient" {
                let outer = tile.inset(-2.);
                out.draw.push(Draw::DiagonalGradientBorder(
                    outer,
                    16.,
                    4.,
                    c(first, 1.),
                    c(second, 1.),
                ));
            } else {
                out.border(
                    tile.inset(-2.),
                    16.,
                    3.,
                    if accent.use_focus {
                        c(first, 1.)
                    } else {
                        WHITE
                    },
                );
            }
        }
    }
    // Caption hugs the actual miniature rather than the bottom of the glass.
    let label_y = TOP + S32 + BACK_SIZE + S16 + 357.75 + S16;
    out.text(
        &accent.id,
        Rect::new(LEFT + S32, label_y, LEFT_W - 2. * S32, 44.),
        text(31., 600, 0.),
        WHITE,
        false,
    );
    let enabled = [
        (accent.use_focus, "focus ring"),
        (accent.use_nav, "navigation"),
        (accent.use_headline, "headline"),
    ]
    .iter()
    .filter_map(|(on, label)| on.then_some(*label))
    .collect::<Vec<_>>();
    let summary = if enabled.is_empty() {
        "No UI highlights".into()
    } else {
        enabled.join(", ")
    };
    out.text(
        format!(
            "{} · {}",
            if accent.kind == "gradient" {
                "Gradient"
            } else {
                "Color"
            },
            summary
        ),
        Rect::new(LEFT + S32, label_y + 50., LEFT_W - 2. * S32, 40.),
        text(25., 500, 0.),
        c(0xffffff, 0.67),
        false,
    );
    out.settings_sidebar_range = start..out.draw.len();
}

fn stacks_list(out: &mut Scene, state: &State, assets: &Assets) {
    let n = state.preferences.stacks.len();
    panels(out, MAX_HEIGHT);
    let start = out.draw.len();
    art_preview(
        out,
        state,
        assets,
        Art::Stacks,
        "Stacks",
        "Rows under Favorites, top to bottom",
        MAX_HEIGHT,
    );
    out.settings_sidebar_range = start..out.draw.len();
    clip_list(out);
    for (i, stack) in state.preferences.stacks.iter().enumerate() {
        let detail = crate::stacks::row_title(stack);
        cell(
            out,
            state,
            assets,
            Target::StackRow(i),
            scrolled_cell(state, i),
            CellContent {
                name: &stack.name,
                value: Some(if stack.show { &detail } else { "Hidden" }),
                desc: None,
                chevron: true,
                checked: false,
                partial: false,
            },
        );
    }
    cell(
        out,
        state,
        assets,
        Target::StackRow(n),
        scrolled_cell(state, n),
        CellContent {
            name: "Add Stack",
            value: None,
            desc: None,
            chevron: true,
            checked: false,
            partial: false,
        },
    );
    out.draw.push(Draw::Clip(None));
}

fn stack_add(out: &mut Scene, state: &State, assets: &Assets) {
    panels(out, MAX_HEIGHT);
    let start = out.draw.len();
    art_preview(
        out,
        state,
        assets,
        Art::Stacks,
        "Add Stack",
        "Choose where the row comes from",
        MAX_HEIGHT,
    );
    out.settings_sidebar_range = start..out.draw.len();
    for (i, (_, name, ready)) in crate::stacks::SOURCES.iter().enumerate() {
        cell(
            out,
            state,
            assets,
            Target::StackSource(i),
            list_cell(i),
            CellContent {
                name,
                value: Some(if *ready { "" } else { "Soon" }),
                desc: None,
                chevron: *ready,
                checked: false,
                partial: false,
            },
        );
    }
}

/// Sunshine hosts are resolved asynchronously by the Moonlight service.
fn moonlight_brand_preview(out: &mut Scene, assets: &Assets, name: &str, detail: &str) {
    let icon_size = 288.;
    let group_h = icon_size + S24 + 44. + S8 + 40.;
    let y = TOP + (MAX_HEIGHT - group_h) / 2.;
    let x = LEFT + (LEFT_W - icon_size) / 2.;
    // Official Moonlight mark, not the generic monitor-play icon.
    out.image(
        assets.path("moonlight-brand.png"),
        Rect::new(x, y, icon_size, icon_size),
        icon_size / 2.,
        true,
    );
    out.text(
        trunc(assets, name, text(31., 600, 0.), LEFT_W - 2. * S24),
        Rect::new(LEFT + S24, y + icon_size + S24, LEFT_W - 2. * S24, 44.),
        text(31., 600, 0.),
        WHITE,
        true,
    );
    out.text(
        trunc(assets, detail, text(25., 500, 0.), LEFT_W - 2. * S24),
        Rect::new(
            LEFT + S24,
            y + icon_size + S24 + 44. + S8,
            LEFT_W - 2. * S24,
            40.,
        ),
        text(25., 500, 0.),
        c(0xffffff, 0.68),
        true,
    );
}
fn moonlight_hosts(out: &mut Scene, state: &State, assets: &Assets) {
    panels(out, MAX_HEIGHT);
    let start = out.draw.len();
    moonlight_brand_preview(
        out,
        assets,
        "Moonlight",
        "Find a Sunshine PC on your network",
    );
    out.settings_sidebar_range = start..out.draw.len();
    for (i, host) in state.moonlight_hosts.iter().take(7).enumerate() {
        let detail = if host.online {
            format!("{} · Online", host.address)
        } else {
            format!("{} · Offline", host.address)
        };
        cell(
            out,
            state,
            assets,
            Target::MoonlightHost(i),
            list_cell(i),
            CellContent {
                name: &host.name,
                value: Some(&detail),
                desc: None,
                chevron: true,
                checked: false,
                partial: false,
            },
        );
    }
    let n = state.moonlight_hosts.len();
    cell(
        out,
        state,
        assets,
        Target::MoonlightHost(n),
        list_cell(n),
        CellContent {
            name: "Enter IP address",
            value: Some("Manual entry"),
            desc: None,
            chevron: true,
            checked: false,
            partial: false,
        },
    );
    cell(
        out,
        state,
        assets,
        Target::MoonlightHost(n + 1),
        list_cell(n + 1),
        CellContent {
            name: "Search again",
            value: Some("Scan local network"),
            desc: None,
            chevron: true,
            checked: false,
            partial: false,
        },
    );
    if let Some(status) = &state.moonlight_status {
        out.text(
            status,
            Rect::new(
                RIGHT + S32,
                list_cell(n + 2).y + S16,
                WIDTH - 2. * S32,
                105.,
            ),
            text(25., 500, 0.),
            c(0xffffff, 0.75),
            false,
        );
    }
    if n <= 3 {
        moonlight_steps(out, state);
    }
}
/// What has to happen before Luma can pair: Luma streams through the Moonlight
/// app, so that app must be installed, set up and paired with the PC first.
fn moonlight_steps(out: &mut Scene, state: &State) {
    let apps = &state.inventory.apps;
    let missing = !apps.is_empty() && !apps.iter().any(|a| a.id == "com.limelight.webos");
    let steps = [
        if missing {
            "1. Install Moonlight from Homebrew Channel. It is not on this TV yet"
        } else {
            "1. Install Moonlight from Homebrew Channel"
        },
        "2. Open Moonlight, pair it with your PC and pick resolution, FPS and bitrate",
        "3. Come back and pair Luma too. The PC asks for a second PIN",
    ];
    let line = 44.;
    let x = RIGHT + S32;
    let w = WIDTH - 2. * S32;
    let mut y = TOP + MAX_HEIGHT - S32 - line * steps.len() as f32 - 40.;
    out.text(
        "BEFORE YOU PAIR",
        Rect::new(x, y, w, 32.),
        text(20., 700, 0.06),
        c(0xffffff, 0.6),
        false,
    );
    y += 40.;
    for (i, step) in steps.iter().enumerate() {
        let ink = if i == 0 && missing {
            c(0xffd60a, 1.)
        } else {
            c(0xffffff, 0.8)
        };
        out.text(
            *step,
            Rect::new(x, y, w, line),
            text(25., 500, 0.),
            ink,
            false,
        );
        y += line;
    }
}
// 24px animated circular spinner; never confuse it with a standalone status dot.
fn moonlight_pin_spinner(out: &mut Scene, x: f32, y: f32, state: &State, ink: Color) {
    let phase = state
        .panel_started
        .map(|at| (at.elapsed().as_millis() / 95) as usize % 12)
        .unwrap_or(0);
    for i in 0..12 {
        let angle = (i as f32 / 12.) * std::f32::consts::TAU - std::f32::consts::FRAC_PI_2;
        let fade = ((i + 12 - phase) % 12) as f32;
        let alpha = 0.14 + 0.86 * (1. - fade / 12.).powf(2.);
        let dot = Rect::new(
            x + 12. + 9. * angle.cos() - 2.,
            y + 12. + 9. * angle.sin() - 2.,
            4.,
            4.,
        );
        out.shape(dot, 2., [ink[0], ink[1], ink[2], alpha]);
    }
}
fn moonlight_pin(out: &mut Scene, state: &State, assets: &Assets) {
    panels(out, MAX_HEIGHT);
    let start = out.draw.len();
    let host = state.moonlight_retry_host.as_ref();
    let name = host.map(|h| h.name.as_str()).unwrap_or("PC");
    moonlight_brand_preview(
        out,
        assets,
        &format!("Pair with {name}"),
        "Luma pairs separately from the Moonlight app",
    );
    out.settings_sidebar_range = start..out.draw.len();

    // One vertically and horizontally centred group; gaps follow the 8px grid.
    let center = RIGHT + WIDTH / 2.;
    let block_h = 76. + S16 + 48. + 40. + 192. + S32 + 64. + 40. + 84.;
    let block_y = TOP + (MAX_HEIGHT - block_h) / 2.;
    out.text(
        "Enter this PIN in Sunshine",
        Rect::new(RIGHT + S32, block_y, WIDTH - 2. * S32, 76.),
        TextStyle::new(52.)
            .display(true)
            .weight(700)
            .tracking(-0.015),
        WHITE,
        true,
    );
    let ip = host.map(|h| h.address.as_str()).unwrap_or("your PC");
    let instructions = format!("Open https://{ip}:47990  ›  PIN, device name Luma");
    let instruction_y = block_y + 76. + S16;
    out.text(
        trunc(assets, &instructions, text(25., 500, 0.), WIDTH - 2. * S32),
        Rect::new(RIGHT + S32, instruction_y, WIDTH - 2. * S32, 48.),
        text(25., 500, 0.),
        c(0xffffff, 0.76),
        true,
    );

    let tile_w = 152.;
    let tile_h = 192.;
    let gap = S24;
    let x0 = center - (4. * tile_w + 3. * gap) / 2.;
    let tile_y = instruction_y + 48. + 40.;
    let pin = state.moonlight_pin.as_deref().unwrap_or("");
    let digits = pin.chars().collect::<Vec<_>>();
    for i in 0..4 {
        let r = Rect::new(x0 + i as f32 * (tile_w + gap), tile_y, tile_w, tile_h);
        out.shape(r, 28., c(0xffffff, 0.085));
        out.border(r, 28., 1., c(0xffffff, 0.15));
        // No placeholder glyphs: empty tiles until the PIN exists.
        let digit = digits.get(i).map(|d| d.to_string()).unwrap_or_default();
        out.text(
            &digit,
            Rect::new(r.x, r.y + 26., r.w, 140.),
            TextStyle::new(120.)
                .display(true)
                .weight(750)
                .tracking(0.)
                .tabular(),
            WHITE,
            true,
        );
    }

    let paired = state.preferences.moonlight.paired;
    let (status, ink) = if state.moonlight_pair_error {
        let raw = state
            .moonlight_status
            .as_deref()
            .unwrap_or("Pairing failed");
        let reason = raw.split_once(": ").map(|(_, r)| r).unwrap_or(raw);
        (reason.to_owned(), c(0xff6961, 1.))
    } else if paired {
        (format!("Paired with {name}"), c(0x30d158, 1.))
    } else if state.moonlight_status.as_deref().is_some_and(|v| {
        v.contains("Step 2/")
            || v.contains("Step 3/")
            || v.contains("Step 4/")
            || v.contains("Step 5/")
    }) {
        ("Connecting…".into(), c(0xffffff, 0.95))
    } else {
        ("Waiting for the PIN…".into(), c(0xffffff, 0.85))
    };
    let status_y = tile_y + tile_h + S32;
    let status_style = text(28., 600, 0.);
    let waiting = !paired && !state.moonlight_pair_error;
    let icon_space = if waiting || paired { 36. } else { 0. };
    let max_width = WIDTH - 2. * S48 - icon_space;
    let status = trunc(assets, &status, status_style, max_width);
    let text_width = assets.width(&status, status_style).min(max_width);
    let text_x = center - (text_width + icon_space) / 2. + icon_space;
    if waiting {
        moonlight_pin_spinner(out, text_x - 36., status_y + 10., state, ink);
    } else if paired {
        out.icon(
            assets,
            "check",
            Rect::new(text_x - 36., status_y + 10., 24., 24.),
            ink,
        );
    }
    out.text(
        &status,
        Rect::new(text_x, status_y, text_width + 2., 64.),
        status_style,
        ink,
        false,
    );

    let actions_y = status_y + 64. + 40.;
    let button_w = if state.moonlight_pair_error {
        272.
    } else {
        256.
    };
    let gap = S24;
    let total_buttons = if state.moonlight_pair_error {
        2. * button_w + gap
    } else {
        button_w
    };
    let base = center - total_buttons / 2.;
    let draw_button = |out: &mut Scene, target: Target, label: &str, x: f32| {
        let rect = Rect::new(x, actions_y, button_w, 84.);
        // Shared two-state control: grey at rest, accent gradient with focus.
        let level = state.focus_level(&target);
        let ink = super::movie_view::two_state(out, state, rect, 42., level);
        out.text(
            label,
            Rect::new(rect.x, rect.y + 20., rect.w, 46.),
            text(31., 650, 0.),
            ink,
            true,
        );
        out.control(target, rect, false);
    };
    draw_button(out, Target::MoonlightCancelButton, "Cancel", base);
    if state.moonlight_pair_error {
        draw_button(
            out,
            Target::MoonlightRetry,
            "Try again",
            base + button_w + gap,
        );
    }
}

/// Jellyfin sign-in (approved mockup 25.10): a large QR fills the left
/// column; the right one lists servers, then the Quick Connect code.
fn jellyfin_link(out: &mut Scene, state: &State, assets: &Assets) {
    panels(out, MAX_HEIGHT);
    let start = out.draw.len();
    let (title, detail) = &state.jellyfin_qr_caption;
    sign_in_qr(
        out,
        assets,
        &state.jellyfin_qr,
        if title.is_empty() { "Jellyfin" } else { title },
        if detail.is_empty() {
            "Looking for your server"
        } else {
            detail
        },
    );
    out.settings_sidebar_range = start..out.draw.len();

    let x = RIGHT + S32;
    let w = WIDTH - 2. * S32;
    let shown = state.jellyfin_servers.len().min(3);
    for (k, server) in state.jellyfin_servers.iter().take(shown).enumerate() {
        let selected = state.jellyfin_selected.as_deref() == Some(server.address.as_str());
        let host = server
            .address
            .trim_start_matches("http://")
            .trim_start_matches("https://");
        let detail = host.to_owned();
        let name = if server.name.is_empty() {
            "Jellyfin"
        } else {
            server.name.as_str()
        };
        cell(
            out,
            state,
            assets,
            Target::JellyfinServer(k),
            list_cell(k),
            CellContent {
                name,
                value: Some(&detail),
                desc: None,
                chevron: false,
                checked: selected,
                partial: false,
            },
        );
    }
    cell(
        out,
        state,
        assets,
        Target::JellyfinServer(shown),
        list_cell(shown),
        CellContent {
            name: "Enter address with your phone",
            value: None,
            desc: None,
            chevron: true,
            checked: false,
            partial: false,
        },
    );

    // Code, hint and status: one centred group under the rows.
    let center = RIGHT + WIDTH / 2.;
    let top = list_cell(shown + 1).y + S16;
    let actions_y = TOP + MAX_HEIGHT - S32 - 84.;
    let tile_w = 96.;
    let tile_h = 124.;
    let gap = S16;
    let group_h = tile_h + S24 + 76. + S24 + 44.;
    let group_y = top + ((actions_y - S24 - top) - group_h).max(0.) / 2.;
    if let Some(code) = state.jellyfin_code.as_deref() {
        let digits = code.chars().collect::<Vec<_>>();
        let n = digits.len().max(1) as f32;
        let x0 = center - (n * tile_w + (n - 1.) * gap) / 2.;
        for (k, digit) in digits.iter().enumerate() {
            let r = Rect::new(x0 + k as f32 * (tile_w + gap), group_y, tile_w, tile_h);
            out.shape(r, 22., c(0xffffff, 0.085));
            out.border(r, 22., 1., c(0xffffff, 0.15));
            out.text(
                digit.to_string(),
                Rect::new(r.x, r.y + 18., r.w, 90.),
                TextStyle::new(76.).display(true).weight(600).tabular(),
                WHITE,
                true,
            );
        }
        out.text(
            "Scan the QR, or open Jellyfin › Profile › Quick Connect\nand enter this code. Luma signs in by itself.",
            Rect::new(x, group_y + tile_h + S24, w, 76.),
            text(25., 500, 0.).leading(1.5),
            c(0xffffff, 0.78),
            true,
        );
    }
    if let Some(status) = state.jellyfin_status.as_deref() {
        let status_y = if state.jellyfin_code.is_some() {
            group_y + tile_h + S24 + 76. + S24
        } else {
            group_y + group_h / 2. - 22.
        };
        sign_in_status(out, state, assets, status, state.jellyfin_error, status_y);
    }
    sign_in_buttons(
        out,
        state,
        [
            (Target::JellyfinNewCode, "New code"),
            (Target::JellyfinCancel, "Cancel"),
        ],
    );
}

/// Large QR (left column) and a two-button row shared by the media-server
/// sign-in pages.
fn sign_in_qr(out: &mut Scene, assets: &Assets, modules: &[Vec<bool>], title: &str, detail: &str) {
    // No QR yet (searching): only the caption, centred, no empty square.
    let qr = if modules.is_empty() { -S32 } else { 432. };
    let group_h = qr + S32 + 44. + S8 + 40.;
    let y = TOP + (MAX_HEIGHT - group_h) / 2.;
    let x = LEFT + (LEFT_W - qr) / 2.;
    if !modules.is_empty() {
        crate::phone_link::draw_qr(out, modules, Rect::new(x, y, qr, qr));
    }
    let w = LEFT_W - 2. * S24;
    out.text(
        trunc(assets, title, text(31., 600, 0.), w),
        Rect::new(LEFT + S24, y + qr + S32, w, 44.),
        text(31., 600, 0.),
        WHITE,
        true,
    );
    out.text(
        trunc(assets, detail, text(25., 500, 0.), w),
        Rect::new(LEFT + S24, y + qr + S32 + 44. + S8, w, 40.),
        text(25., 500, 0.),
        c(0xffffff, 0.68),
        true,
    );
}

fn sign_in_status(
    out: &mut Scene,
    state: &State,
    assets: &Assets,
    status: &str,
    error: bool,
    y: f32,
) {
    let w = WIDTH - 2. * S32;
    let center = RIGHT + WIDTH / 2.;
    let style = text(27., 600, 0.);
    let ink = if error {
        c(0xff6961, 1.)
    } else {
        c(0xffffff, 0.85)
    };
    let icon = if error { 0. } else { 36. };
    let status = trunc(assets, status, style, w - icon);
    let text_w = assets.width(&status, style).min(w - icon);
    let text_x = center - (text_w + icon) / 2. + icon;
    if !error {
        moonlight_pin_spinner(out, text_x - 36., y + 10., state, ink);
    }
    out.text(
        &status,
        Rect::new(text_x, y, text_w + 2., 44.),
        style,
        ink,
        false,
    );
}

fn sign_in_buttons(out: &mut Scene, state: &State, buttons: [(Target, &str); 2]) {
    let x = RIGHT + S32;
    let w = WIDTH - 2. * S32;
    let actions_y = TOP + MAX_HEIGHT - S32 - 84.;
    let button_w = (w - S24) / 2.;
    for (k, (target, label)) in buttons.into_iter().enumerate() {
        let rect = Rect::new(x + k as f32 * (button_w + S24), actions_y, button_w, 84.);
        let level = state.focus_level(&target);
        let ink = super::movie_view::two_state(out, state, rect, 42., level);
        out.text(
            label,
            Rect::new(rect.x, rect.y + 20., rect.w, 46.),
            text(31., 650, 0.),
            ink,
            true,
        );
        out.control(target, rect, false);
    }
}

/// Plex sign-in (approved mockup 25.10): QR left, plex.tv/link code right.
fn plex_link(out: &mut Scene, state: &State, assets: &Assets) {
    panels(out, MAX_HEIGHT);
    let start = out.draw.len();
    sign_in_qr(
        out,
        assets,
        &state.plex_qr,
        "Scan with your phone",
        "The code is filled in for you",
    );
    out.settings_sidebar_range = start..out.draw.len();

    let center = RIGHT + WIDTH / 2.;
    let tile_w = 152.;
    let tile_h = 192.;
    let gap = S24;
    let actions_y = TOP + MAX_HEIGHT - S32 - 84.;
    let block_h = 64. + S32 + tile_h + S32 + 44. + S24 + 44.;
    let block_y = TOP + (actions_y - S24 - TOP - block_h) / 2.;
    out.text(
        "Go to plex.tv/link and enter",
        Rect::new(RIGHT + S32, block_y, WIDTH - 2. * S32, 64.),
        TextStyle::new(44.)
            .display(true)
            .weight(600)
            .tracking(-0.01),
        WHITE,
        true,
    );
    let tile_y = block_y + 64. + S32;
    let code = state.plex_code.as_deref().unwrap_or("");
    let letters = code.chars().collect::<Vec<_>>();
    let n = 4usize.max(letters.len());
    let x0 = center - (n as f32 * tile_w + (n - 1) as f32 * gap) / 2.;
    for k in 0..n {
        let r = Rect::new(x0 + k as f32 * (tile_w + gap), tile_y, tile_w, tile_h);
        out.shape(r, 28., c(0xffffff, 0.085));
        out.border(r, 28., 1., c(0xffffff, 0.15));
        let letter = letters.get(k).map(|l| l.to_string()).unwrap_or_default();
        out.text(
            &letter,
            Rect::new(r.x, r.y + 26., r.w, 140.),
            TextStyle::new(120.).display(true).weight(600).tabular(),
            WHITE,
            true,
        );
    }
    let hint_y = tile_y + tile_h + S32;
    out.text(
        "Sign in to Plex and press Link. The code works 15 minutes",
        Rect::new(RIGHT + S32, hint_y, WIDTH - 2. * S32, 44.),
        text(25., 500, 0.),
        c(0xffffff, 0.78),
        true,
    );
    if let Some(status) = state.plex_status.as_deref() {
        sign_in_status(
            out,
            state,
            assets,
            status,
            state.plex_error,
            hint_y + 44. + S24,
        );
    }
    sign_in_buttons(
        out,
        state,
        [
            (Target::PlexNewCode, "New code"),
            (Target::PlexCancel, "Cancel"),
        ],
    );
}

fn stack_config(out: &mut Scene, state: &State, assets: &Assets, i: usize) {
    let Some(stack) = state.preferences.stacks.get(i) else {
        return;
    };
    panels(out, MAX_HEIGHT);
    let start = out.draw.len();
    art_preview(
        out,
        state,
        assets,
        Art::Stacks,
        &stack.name,
        crate::stacks::source_name(&stack.source),
        MAX_HEIGHT,
    );
    out.settings_sidebar_range = start..out.draw.len();
    let n = state.preferences.stacks.len();
    if crate::stacks::compact(&stack.source) {
        let jellyfin = stack.source == "jellyfin";
        let category = crate::stacks::collection_label(stack);
        let account = if stack.source == "plex" {
            format!(
                "{} · {}",
                state.preferences.plex.user_name, state.preferences.plex.server_name
            )
        } else {
            format!(
                "{} · {}",
                state.preferences.jellyfin.user_name, state.preferences.jellyfin.server_name
            )
        };
        let jellyfin = jellyfin || stack.source == "plex";
        let rows = [
            (
                if jellyfin { "Row" } else { "Category" },
                Some(category.as_str()),
                true,
            ),
            ("Card shape", Some(stack.shape.label()), true),
            (
                "Show on Home",
                Some(if stack.show { "On" } else { "Off" }),
                false,
            ),
            ("Move up", if i == 0 { Some("Top") } else { None }, false),
            (
                "Move down",
                if i + 1 == n { Some("Bottom") } else { None },
                false,
            ),
            if jellyfin {
                ("Sign out", Some(account.as_str()), false)
            } else {
                ("Unpair Luma", Some("Delete Luma credentials"), false)
            },
            ("Remove", None, false),
        ];
        for (k, (name, value, chevron)) in rows.into_iter().enumerate() {
            cell(
                out,
                state,
                assets,
                Target::StackOpt(k),
                list_cell(k),
                CellContent {
                    name,
                    value,
                    desc: None,
                    chevron,
                    checked: false,
                    partial: false,
                },
            );
        }
        return;
    }
    let collection = crate::stacks::collection_label(stack);
    let rows: [(&str, Option<&str>, bool); 7] = [
        ("Collection", Some(&collection), true),
        ("Card shape", Some(stack.shape.label()), true),
        ("Card shape", Some(stack.shape.label()), true),
        (
            "Show on Home",
            Some(if stack.show { "On" } else { "Off" }),
            false,
        ),
        ("Move up", if i == 0 { Some("Top") } else { None }, false),
        (
            "Move down",
            if i + 1 == n { Some("Bottom") } else { None },
            false,
        ),
        ("Remove", None, false),
    ];
    for (k, (name, value, chevron)) in rows.into_iter().enumerate() {
        cell(
            out,
            state,
            assets,
            Target::StackOpt(k),
            list_cell(k),
            CellContent {
                name,
                value,
                desc: None,
                chevron,
                checked: false,
                partial: false,
            },
        );
    }
}

/// Choice list of one stack option: 0 collection, 1 type, 2 card shape.
fn stack_choices(out: &mut Scene, state: &State, assets: &Assets, i: usize, mode: u8) {
    use crate::stacks::CardShape;
    let Some(stack) = state.preferences.stacks.get(i) else {
        return;
    };
    panels(out, MAX_HEIGHT);
    let start = out.draw.len();
    art_preview(
        out,
        state,
        assets,
        Art::Stacks,
        &stack.name,
        crate::stacks::source_name(&stack.source),
        MAX_HEIGHT,
    );
    out.settings_sidebar_range = start..out.draw.len();
    clip_list(out);
    if mode == 2 {
        for (k, choice) in CardShape::ALL.iter().enumerate() {
            cell(
                out,
                state,
                assets,
                Target::StackPick(k),
                scrolled_cell(state, k),
                CellContent {
                    name: choice.label(),
                    value: Some(choice.hint()),
                    desc: None,
                    chevron: false,
                    checked: stack.shape == *choice,
                    partial: false,
                },
            );
        }
    } else {
        for (k, (id, label)) in crate::stacks::collection_choices(state, i)
            .iter()
            .enumerate()
        {
            cell(
                out,
                state,
                assets,
                Target::StackPick(k),
                scrolled_cell(state, k),
                CellContent {
                    name: label,
                    value: None,
                    desc: None,
                    chevron: false,
                    checked: stack.collection == *id,
                    partial: false,
                },
            );
        }
    }
    out.draw.push(Draw::Clip(None));
}

fn category(out: &mut Scene, state: &State, assets: &Assets, group: usize) {
    let groups = state.video_groups_for_filter(0);
    let Some((_src, _name, items)) = groups.get(group) else {
        return;
    };
    glass(out, Rect::new(LEFT, TOP, 1920. - 2. * EDGE, MAX_HEIGHT));
    let catalog = crate::video_catalog::all();
    let count = items
        .iter()
        .filter(|&&i| !state.preferences.video_excluded.contains(&catalog[i].id))
        .count();
    out.text(
        format!("{count} of {} selected", items.len()),
        Rect::new(
            LEFT + S24 + BACK_SIZE + S16,
            TOP + S24 + (BACK_SIZE - 42.) / 2.,
            620.,
            42.,
        ),
        text(29., 500, 0.),
        WHITE,
        false,
    );
    // A 56px Select All control with vertically and horizontally centred copy.
    // The regular 80px list-cell renderer cannot be used for this short button.
    let target = Target::VideoSelectAll(group);
    let button = Rect::new(1920. - EDGE - S24 - 270., TOP + S24, 270., BACK_SIZE);
    let (focused, t) = focus(state, &target, button, 0.04);
    out.shape(button, CELL_RADIUS, c(0xffffff, 0.10));
    let ink = focus_fill(out, state, focused, CELL_RADIUS, t);
    out.text(
        if count == items.len() {
            "Deselect All"
        } else {
            "Select All"
        },
        Rect::new(button.x, button.y + S8, button.w, 40.),
        text(29., 500, 0.),
        blend(WHITE, ink, t),
        true,
    );
    out.control(target, button, true);
    out.draw.push(Draw::Clip(Some(Rect::new(
        LEFT + S8,
        TOP + 96.,
        1920. - 2. * EDGE - S16,
        MAX_HEIGHT - 96. - S8,
    ))));
    for (pos, index) in items.iter().enumerate() {
        let x = LEFT + S24 + (pos % 4) as f32 * (408. + S16);
        let y = TOP + 112. + (pos / 4) as f32 * (230. + S48) - state.scroll;
        out.control(
            Target::VideoPick(*index),
            Rect::new(x, y, 408., 230. + S48),
            true,
        );
        if !(TOP - 300. ..=TOP + MAX_HEIGHT).contains(&y) {
            continue;
        }
        let r = Rect::new(x, y, 408., 230.);
        let (r, t) = focus(state, &Target::VideoPick(*index), r, 0.06);
        let selected = !state
            .preferences
            .video_excluded
            .contains(&catalog[*index].id);
        if t > 0. {
            out.shadow(r, MEDIA_RADIUS, S24, S8, t * 0.35);
        }
        let path = assets.path(format!("video-thumbnails/{index}.jpg"));
        if selected {
            out.image(path, r, MEDIA_RADIUS, false);
        } else {
            out.draw.push(Draw::TintedImage(
                path,
                r,
                MEDIA_RADIUS,
                false,
                [1., 1., 1., 0.45],
            ));
        }
        // Focus wears the accent ring, like every card (never white).
        out.focus_ring(r, MEDIA_RADIUS, t, state);
        dot(
            out,
            state,
            r.x + r.w - 48.,
            r.y + S8,
            selected,
            false,
            false,
        );
        out.text(
            trunc(assets, &catalog[*index].name, text(25., 600, 0.), 408.),
            Rect::new(x, y + 230. + S8, 408., 40.),
            text(25., 600, 0.),
            WHITE,
            false,
        );
    }
    out.draw.push(Draw::Clip(Some(Rect::new(
        64.,
        0.,
        1792.,
        state.main_bottom(),
    ))));
}
pub(super) fn render(out: &mut Scene, state: &State, assets: &Assets) {
    match state.settings_level {
        SettingsLevel::Root => root(out, state, assets),
        SettingsLevel::Widgets(page) => widgets_settings::render(out, state, assets, page),
        SettingsLevel::HeadingFonts => font_gallery(out, state, assets, true),
        SettingsLevel::TextFonts => font_gallery(out, state, assets, false),
        SettingsLevel::UiResolution => ui_resolution_gallery(out, state, assets),
        SettingsLevel::Section if state.section == 1 => wallpaper(out, state, assets),
        SettingsLevel::Section if state.section == crate::state::APPEARANCE_SECTION => {
            appearance(out, state, assets)
        }
        SettingsLevel::Section if state.section == crate::state::STACKS_SECTION => {
            stacks_list(out, state, assets)
        }
        SettingsLevel::StackAdd => stack_add(out, state, assets),
        SettingsLevel::MoonlightHosts => moonlight_hosts(out, state, assets),
        SettingsLevel::MoonlightPin => moonlight_pin(out, state, assets),
        SettingsLevel::JellyfinLink => jellyfin_link(out, state, assets),
        SettingsLevel::PlexLink => plex_link(out, state, assets),
        SettingsLevel::StackConfig(i) => stack_config(out, state, assets, i),
        SettingsLevel::StackCollection(i) => stack_choices(out, state, assets, i, 0),
        SettingsLevel::StackShape(i) => stack_choices(out, state, assets, i, 2),
        SettingsLevel::WallpaperVideos => videos(out, state, assets),
        SettingsLevel::WallpaperCategory(group) => category(out, state, assets, group),
        SettingsLevel::WallpaperType => {
            panels(out, PREVIEW_MIN);
            let preview_start = out.draw.len();
            wallpaper_preview(
                out,
                state,
                assets,
                Some(state.preferences.video_index),
                "Type",
                "Choose wallpaper source",
                PREVIEW_MIN,
            );
            out.settings_sidebar_range = preview_start..out.draw.len();
            for (i, name) in ["Photo", "Video"].into_iter().enumerate() {
                cell(
                    out,
                    state,
                    assets,
                    Target::WallpaperTab(i),
                    list_cell(i),
                    CellContent {
                        name,
                        value: None,
                        desc: None,
                        chevron: false,
                        checked: (state.preferences.background == "video") == (i == 1),
                        partial: false,
                    },
                );
            }
        }
        SettingsLevel::WallpaperPhotos => {
            glass(out, Rect::new(LEFT, TOP, 1920. - 2. * EDGE, MAX_HEIGHT));
            for (i, (id, _)) in BACKGROUNDS.iter().enumerate() {
                let x = LEFT + S24 + (i % 4) as f32 * (408. + S16);
                let y = TOP + S48 + S64 + (i / 4) as f32 * (230. + S48) - state.scroll;
                let r = Rect::new(x, y, 408., 230.);
                out.image(
                    assets.path(format!("thumbnails/{id}.jpg")),
                    r,
                    MEDIA_RADIUS,
                    false,
                );
                out.text(
                    state.preferences.language.wallpaper(id),
                    Rect::new(x, y + 230. + S8, 408., 45.),
                    text(25., 600, 0.),
                    WHITE,
                    false,
                );
                if state.focus == Target::Background(i) {
                    out.focus_ring(r, MEDIA_RADIUS, 1., state);
                }
                out.control(
                    Target::Background(i),
                    Rect::new(x, y, 408., 230. + S48),
                    true,
                );
            }
        }
        _ => {}
    }
    back_button(out, state, assets);
}
