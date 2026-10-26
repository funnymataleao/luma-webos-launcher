//! Settings › Smart Home, drawn in the right glass panel with the shared tvOS cells.
//! State owns push/pop through Subpage::SmartConnect / SmartSelect.
//! Lists register every row and clip to the panel, so remote focus and the
//! wheel can reach rows below the fold.
use super::tvos_settings_view::{MAX_HEIGHT, RIGHT, ROW_HEIGHT, S16, S24, S32, TOP, WIDTH};
use super::*;

const LEFT: f32 = RIGHT + S32;
const ROW_W: f32 = WIDTH - 2. * S32;
const PITCH: f32 = ROW_HEIGHT + 12.;
const FIRST: f32 = TOP + S32;
const INK: u32 = 0xf5f5f7;
const QR: f32 = 264.;
const CARD_H: f32 = QR + 2. * S24;

fn clip(out: &mut Scene) {
    out.draw.push(Draw::Clip(Some(Rect::new(
        RIGHT + S16,
        TOP + S16,
        WIDTH - S32,
        MAX_HEIGHT - S32,
    ))));
}
fn visible(y: f32, h: f32) -> bool {
    y + h > TOP && y < TOP + MAX_HEIGHT
}
/// Group heading above a run of cells, like CONNECTED / NOT CONNECTED in Inputs.
fn caption(out: &mut Scene, label: &str, y: f32) {
    out.text(
        label.to_uppercase(),
        Rect::new(LEFT + 21., y + 6., ROW_W - 42., 34.),
        text(25., 600, 0.02),
        c(INK, 0.62),
        false,
    );
}
fn line(out: &mut Scene, assets: &Assets, label: &str, r: Rect, alpha: f32, weight: u16) {
    let style = text(25., weight, 0.);
    out.text(
        super::tvos_settings_view::ellipsis_for_smart(assets, label, style, r.w),
        r,
        style,
        c(INK, alpha),
        false,
    );
}
/// One Smart Home row; an empty `value` shows no trailing text.
struct Row<'a> {
    label: &'a str,
    value: &'a str,
    chevron: bool,
    checked: bool,
}
fn cell(out: &mut Scene, state: &State, assets: &Assets, target: Target, r: Rect, content: Row) {
    let Row {
        label,
        value,
        chevron,
        checked,
    } = content;
    if !visible(r.y, r.h) {
        // Off-panel rows still exist for remote focus, reveal and wheel bounds.
        out.control(target, r, true);
        return;
    }
    // A chosen option is outlined in the accent; only focus fills. Device
    // picks are a multi-select list, so they keep just the check.
    let outline =
        checked && !matches!(target, Target::SmartPick(..)) && state.focus_level(&target) < 0.5;
    super::tvos_settings_view::cell(
        out,
        state,
        assets,
        target,
        r,
        super::tvos_settings_view::CellContent {
            name: label,
            value: (!value.is_empty()).then_some(value),
            desc: None,
            chevron,
            checked,
            partial: false,
        },
    );
    if outline {
        out.accent_ring(r, super::tvos_settings_view::CELL_RADIUS, state);
    }
}
fn row(y: f32) -> Rect {
    Rect::new(LEFT, y, ROW_W, ROW_HEIGHT)
}
fn summary(out: &mut Scene, state: &State, assets: &Assets, y: f32) {
    let r = Rect::new(LEFT, y, ROW_W, 132.);
    out.shape(r, super::tvos_settings_view::CELL_RADIUS, c(0xffffff, 0.07));
    out.text(
        "Home Assistant",
        Rect::new(r.x + S32, r.y + 22., ROW_W - 300., 44.),
        text(31., 600, -0.015),
        c(INK, 1.),
        false,
    );
    let address = &state.preferences.smart_home.server_url;
    line(
        out,
        assets,
        if address.is_empty() {
            "Not connected"
        } else {
            address
        },
        Rect::new(r.x + S32, r.y + 76., ROW_W - 300., 36.),
        0.62,
        500,
    );
    if !address.is_empty() {
        let online = state.ha_online;
        let label = if online { "Connected" } else { "Offline" };
        let style = text(25., 600, 0.);
        let w = assets.width(label, style).ceil();
        let x = r.x + r.w - S32 - w;
        out.shape(
            Rect::new(x - 28., r.y + 56., 16., 16.),
            8.,
            c(if online { 0x30d158 } else { 0xffb340 }, 1.),
        );
        out.text(
            label,
            Rect::new(x, r.y + 46., w + 2., 36.),
            style,
            c(INK, 0.9),
            false,
        );
    }
}
fn shown(state: &State, section: usize) -> String {
    if state.ha_entities.is_empty() {
        return String::new();
    }
    format!("{} shown", state.ha_tab(section).len())
}
fn root(out: &mut Scene, state: &State, assets: &Assets) {
    let y = FIRST - state.scroll;
    summary(out, state, assets, y);
    let connected = !state.preferences.smart_home.server_url.is_empty();
    let rows: Vec<(Target, &str, String, bool)> = if connected {
        vec![
            (
                Target::SmartShowTab,
                "Show Smart Home tab",
                if state.preferences.smart_home.show_tab {
                    "On"
                } else {
                    "Off"
                }
                .into(),
                false,
            ),
            (Target::SmartSection(0), "Devices", shown(state, 0), true),
            (Target::SmartSection(1), "Cameras", shown(state, 1), true),
            (Target::SmartSection(2), "Scenes", shown(state, 2), true),
            (Target::SmartSignOut, "Sign out", String::new(), false),
        ]
    } else {
        vec![(
            Target::SmartConnect,
            "Connect Home Assistant",
            String::new(),
            true,
        )]
    };
    for (i, (target, label, value, chevron)) in rows.into_iter().enumerate() {
        cell(
            out,
            state,
            assets,
            target,
            row(y + 132. + S24 + i as f32 * PITCH),
            Row {
                label,
                value: &value,
                chevron,
                checked: false,
            },
        );
    }
}
fn entity_value(e: &crate::smart_home::Entity) -> String {
    match e.state.as_str() {
        "unavailable" | "unknown" => "Unavailable".into(),
        _ if e.domain == "camera" || matches!(e.domain.as_str(), "scene" | "script") => {
            String::new()
        }
        _ if !e.unit.is_empty() => format!("{} {}", e.state, e.unit),
        "on" => "On".into(),
        "off" => "Off".into(),
        other => {
            let other = other.replace('_', " ");
            let mut chars = other.chars();
            chars
                .next()
                .map(|first| first.to_uppercase().chain(chars).collect())
                .unwrap_or_default()
        }
    }
}
fn selected(out: &mut Scene, state: &State, assets: &Assets, section: usize) {
    clip(out);
    let listed = state.ha_listed(section);
    let default = state.ha_selection_is_default(section);
    let all = !default && listed.iter().all(|e| state.ha_selected(section, e));
    let none = !default && !listed.iter().any(|e| state.ha_selected(section, e));
    let mut y = FIRST - state.scroll;
    for (kind, label, checked) in [
        (0, "Recommended", default),
        (1, "Show all", all && !listed.is_empty()),
        (2, "Hide all", none),
    ] {
        cell(
            out,
            state,
            assets,
            Target::SmartBulk(section, kind),
            row(y),
            Row {
                label,
                value: "",
                chevron: false,
                checked,
            },
        );
        y += PITCH;
    }
    if listed.is_empty() {
        y += S24;
        line(
            out,
            assets,
            if state.ha_online {
                "Home Assistant has nothing to show here yet."
            } else {
                "Home Assistant is offline."
            },
            Rect::new(LEFT + 21., y, ROW_W - 42., 40.),
            0.62,
            500,
        );
    }
    let mut room: Option<&str> = None;
    for e in listed {
        let this = if e.area.is_empty() {
            "Other"
        } else {
            e.area.as_str()
        };
        if room != Some(this) {
            room = Some(this);
            y += S24;
            if visible(y, 46.) {
                caption(out, this, y);
            }
            y += 46.;
        }
        cell(
            out,
            state,
            assets,
            Target::SmartPick(section, e.id.clone()),
            row(y),
            Row {
                label: &e.name,
                value: &entity_value(e),
                chevron: false,
                checked: state.ha_selected(section, e),
            },
        );
        y += PITCH;
    }
    out.draw.push(Draw::Clip(None));
}
fn connect(out: &mut Scene, state: &State, assets: &Assets) {
    clip(out);
    let mut y = FIRST - state.scroll;
    if !state.preferences.smart_home.server_url.is_empty() {
        // Already signed in: no QR, just the connection and the way out.
        summary(out, state, assets, y);
        cell(
            out,
            state,
            assets,
            Target::SmartSignOut,
            row(y + 132. + S24),
            Row {
                label: "Sign out",
                value: "",
                chevron: false,
                checked: false,
            },
        );
        out.draw.push(Draw::Clip(None));
        return;
    }
    caption(out, "Server", y);
    y += 46.;
    if state.ha_servers.is_empty() {
        let label = if state.ha_discovery_since.elapsed().as_secs() >= 10 {
            "Not found on this network. Enter the address below."
        } else {
            "Searching your network…"
        };
        line(
            out,
            assets,
            label,
            Rect::new(LEFT + 21., y + 20., ROW_W - 42., 40.),
            0.78,
            500,
        );
        y += PITCH;
    }
    for (i, server) in state.ha_servers.iter().take(2).enumerate() {
        let active = state.ha_address_draft == server.url;
        cell(
            out,
            state,
            assets,
            Target::SmartServer(i),
            row(y),
            Row {
                label: &server.name,
                value: &server.version,
                chevron: false,
                checked: active,
            },
        );
        y += PITCH;
    }
    cell(
        out,
        state,
        assets,
        Target::SmartAddress,
        row(y),
        Row {
            label: "Enter address",
            value: &state.ha_address_draft,
            chevron: true,
            checked: false,
        },
    );
    y += PITCH + S16;
    caption(
        out,
        if state.ha_token_qr {
            "Use an access token"
        } else {
            "Sign in with your phone"
        },
        y,
    );
    y += 46.;
    let card = Rect::new(LEFT, y, ROW_W, CARD_H);
    out.shape(
        card,
        super::tvos_settings_view::CELL_RADIUS,
        c(0xffffff, 0.07),
    );
    let qr = Rect::new(card.x + S24, card.y + S24, QR, QR);
    let modules = if state.ha_token_qr {
        &state.ha_fallback_qr
    } else {
        &state.ha_qr
    };
    if modules.is_empty() {
        out.shape(qr, 18., c(0xffffff, 0.07));
        out.text(
            if state.ha_address_draft.is_empty() {
                "Choose a server"
            } else {
                "Preparing…"
            },
            Rect::new(qr.x, qr.y + QR / 2. - 20., QR, 40.),
            text(25., 600, 0.),
            c(INK, 0.78),
            true,
        );
    } else {
        crate::phone_link::draw_qr(out, modules, qr);
    }
    let text_x = qr.x + QR + S32;
    let text_w = card.x + card.w - S32 - text_x;
    let steps: [&str; 3] = if state.ha_token_qr {
        [
            "1. Scan with your phone camera",
            "2. Paste a long-lived access token",
            "3. Luma connects by itself",
        ]
    } else {
        [
            "1. Scan with your phone camera",
            "2. Sign in to Home Assistant",
            "3. Luma connects by itself",
        ]
    };
    for (i, step) in steps.iter().enumerate() {
        line(
            out,
            assets,
            step,
            Rect::new(text_x, qr.y + 8. + i as f32 * 48., text_w, 40.),
            1.,
            500,
        );
    }
    line(
        out,
        assets,
        &state.ha_auth_status,
        Rect::new(text_x, qr.y + QR - 48., text_w, 40.),
        0.62,
        500,
    );
    y += CARD_H + S16;
    let half = (ROW_W - S16) / 2.;
    cell(
        out,
        state,
        assets,
        Target::SmartFallback,
        Rect::new(LEFT, y, half, ROW_HEIGHT),
        Row {
            label: if state.ha_token_qr {
                "Sign in instead"
            } else {
                "Use a token"
            },
            value: "",
            chevron: false,
            checked: false,
        },
    );
    cell(
        out,
        state,
        assets,
        Target::SmartRetry,
        Rect::new(LEFT + half + S16, y, half, ROW_HEIGHT),
        Row {
            label: "New QR code",
            value: "",
            chevron: false,
            checked: false,
        },
    );
    out.draw.push(Draw::Clip(None));
}
pub(super) fn render(out: &mut Scene, state: &State, assets: &Assets) {
    match state.subpage.as_ref() {
        Some(Subpage::SmartConnect) => connect(out, state, assets),
        Some(Subpage::SmartSelect(n)) => selected(out, state, assets, *n),
        _ => root(out, state, assets),
    }
}
