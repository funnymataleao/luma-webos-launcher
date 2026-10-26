//! What the remote and the pointer do with Home widgets: focus routes, OK,
//! Back, edit mode, the full-screen pages and Settings › Widgets.
use crate::{
    model::Input,
    smart_home::Entity,
    state::{Control, Direction, Effect, Page, SettingsLevel, State, Target, WEATHER_SECTION},
    widgets::{
        Block, COINS, Detail, Edit, Exchange, Grab, Kind, Period, QUOTES, REFRESH_CHOICES,
        SettingsPage, Size, TEXT_MAX, ZONE_COLUMNS,
    },
};
use std::time::Instant;

/// One row of a Settings › Widgets page.
pub struct Row {
    pub label: String,
    pub value: Option<String>,
    pub chevron: bool,
    pub checked: bool,
    /// Shown but cannot be chosen now (does not fit, not available).
    pub dim: bool,
}
fn row(label: impl Into<String>, value: Option<String>, chevron: bool, checked: bool) -> Row {
    Row {
        label: label.into(),
        value,
        chevron,
        checked,
        dim: false,
    }
}
/// The Root page shows a WIDGET SETTINGS heading before this row.
pub const ROOT_SETTINGS_FROM: usize = 3;

pub fn input_live(input: &Input) -> bool {
    input.signal.unwrap_or(input.connected.unwrap_or(false))
}
/// Inputs the HDMI widget shows: the chosen one first, then connected ones.
pub fn hdmi_inputs(state: &State) -> Vec<&Input> {
    let prefs = &state.preferences.widgets;
    let mut list: Vec<&Input> = state.inventory.inputs.iter().collect();
    list.sort_by_key(|i| {
        (
            i.id != prefs.hdmi_input,
            !i.connected.unwrap_or(false),
            !input_live(i),
        )
    });
    if prefs.hdmi_hide_no_signal {
        let live: Vec<&Input> = list.iter().copied().filter(|i| input_live(i)).collect();
        if !live.is_empty() {
            return live;
        }
    }
    list
}
pub fn input_name(state: &State, input: &Input) -> String {
    state
        .preferences
        .input_names
        .get(&input.id)
        .cloned()
        .filter(|n| !n.trim().is_empty())
        .unwrap_or_else(|| input.title.clone())
}
pub fn smart_camera(state: &State) -> Option<&Entity> {
    let want = &state.preferences.widgets.smart_camera;
    let cams = state.ha_tab(1);
    cams.iter()
        .copied()
        .find(|e| !want.is_empty() && e.id == *want)
        .or_else(|| cams.first().copied())
}
pub fn smart_devices(state: &State) -> Vec<&Entity> {
    let want = &state.preferences.widgets.smart_devices;
    if !want.is_empty() {
        return want
            .iter()
            .filter_map(|id| state.ha_entities.iter().find(|e| &e.id == id))
            .collect();
    }
    let all = state.ha_tab(0);
    let mut list: Vec<&Entity> = all.iter().copied().filter(|e| e.actionable()).collect();
    list.extend(all.iter().copied().filter(|e| !e.actionable()));
    list.truncate(4);
    list
}
fn is_temperature(e: &Entity) -> bool {
    e.domain == "sensor" && (e.device_class == "temperature" || e.unit.contains('°'))
}
/// What the Smart Home text widget can show: sensors and states, temperature first.
pub fn text_candidates(state: &State) -> Vec<&Entity> {
    let mut list: Vec<&Entity> = state
        .ha_entities
        .iter()
        .filter(|e| {
            !e.hidden
                && matches!(
                    e.domain.as_str(),
                    "sensor" | "binary_sensor" | "climate" | "lock" | "alarm_control_panel"
                )
        })
        .collect();
    list.sort_by_key(|e| !is_temperature(e));
    list
}
/// The chosen entity; without a choice, only a room temperature (never an
/// appliance such as a kettle), otherwise nothing and the line shows lights.
pub fn smart_sensor(state: &State) -> Option<&Entity> {
    let want = &state.preferences.widgets.smart_sensor;
    if !want.is_empty() {
        return state.ha_entities.iter().find(|e| e.id == *want);
    }
    const ROOM: [&str; 14] = [
        "room",
        "living",
        "bedroom",
        "indoor",
        "home",
        "house",
        "hall",
        "комнат",
        "гостин",
        "спальн",
        "дом",
        "квартир",
        "зал",
        "детск",
    ];
    const APPLIANCE: [&str; 12] = [
        "kettle",
        "чайник",
        "boiler",
        "бойлер",
        "water",
        "вод",
        "cpu",
        "battery",
        "fridge",
        "холодил",
        "oven",
        "плит",
    ];
    state.ha_entities.iter().find(|e| {
        let text = format!("{} {} {}", e.name, e.area, e.id).to_lowercase();
        !e.hidden
            && is_temperature(e)
            && e.state.parse::<f64>().is_ok()
            && ROOM.iter().any(|w| text.contains(w))
            && !APPLIANCE.iter().any(|w| text.contains(w))
    })
}

/// Nearest control in a direction, the same score as the generic route but
/// without the header rule (full-screen pages put Close at the top).
fn nearest(controls: &[Control], from: &Target, direction: Direction) -> Option<Target> {
    nearest_where(controls, from, direction, |_| true)
}
/// `nearest` among the controls `keep` accepts.
fn nearest_where(
    controls: &[Control],
    from: &Target,
    direction: Direction,
    keep: impl Fn(&Target) -> bool,
) -> Option<Target> {
    let current = controls.iter().find(|c| &c.target == from)?;
    let (x, y) = current.rect.center();
    controls
        .iter()
        .filter(|c| &c.target != from && keep(&c.target))
        .filter_map(|c| {
            let (cx, cy) = c.rect.center();
            let (primary, cross, overlap) = match direction {
                Direction::Up => (y - cy, (x - cx).abs(), (current.rect.w + c.rect.w) / 2.),
                Direction::Down => (cy - y, (x - cx).abs(), (current.rect.w + c.rect.w) / 2.),
                Direction::Left => (x - cx, (y - cy).abs(), (current.rect.h + c.rect.h) / 2.),
                Direction::Right => (cx - x, (y - cy).abs(), (current.rect.h + c.rect.h) / 2.),
            };
            (primary > 1.).then_some((
                c.target.clone(),
                primary + cross * 2. + if cross >= overlap { 10000. } else { 0. },
            ))
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(t, _)| t)
}
fn is_widget(t: &Target) -> bool {
    matches!(
        t,
        Target::Widget(_) | Target::WidgetText(_) | Target::WidgetTile(..)
    )
}
fn rect_of(controls: &[Control], t: &Target) -> Option<crate::state::Rect> {
    controls.iter().find(|c| &c.target == t).map(|c| c.rect)
}

impl State {
    fn widget_save(&mut self) -> Vec<Effect> {
        self.preferences.widgets.normalize();
        self.dirty = true;
        vec![Effect::Save]
    }
    pub fn weather_widget_shown(&self) -> bool {
        self.preferences.show_weather && self.preferences.widgets.uses(Kind::Weather)
    }
    /// Put a weather widget back: the largest block that fits, else the text line.
    pub fn restore_weather_widget(&mut self) {
        if self.preferences.widgets.uses(Kind::Weather) {
            return;
        }
        let size = [Size::M, Size::S]
            .into_iter()
            .find(|s| self.widget_add_allowed(Kind::Weather, *s));
        let prefs = &mut self.preferences.widgets;
        match size {
            Some(size) => prefs.blocks.insert(
                0,
                Block {
                    kind: Kind::Weather,
                    size,
                },
            ),
            None if prefs.text.len() < TEXT_MAX => prefs.text.insert(0, Kind::Weather),
            None => prefs.text[0] = Kind::Weather,
        }
        prefs.normalize();
    }
    pub fn widget_page(&self) -> Option<SettingsPage> {
        match self.settings_level {
            SettingsLevel::Widgets(p) if self.page == Page::Settings => Some(p),
            _ => None,
        }
    }

    // ------------------------------------------------------------ remote

    /// Arrow keys for widgets, their pages and Settings › Widgets. None hands
    /// the key to the generic route.
    pub fn widget_direction(
        &mut self,
        direction: Direction,
        controls: &[Control],
    ) -> Option<Vec<Effect>> {
        if self.widget_detail.is_some() {
            if let Some(next) = nearest(controls, &self.focus, direction) {
                self.focus(next, controls);
            } else if matches!(direction, Direction::Up)
                && controls.iter().any(|c| c.target == Target::WidgetClose)
            {
                self.focus(Target::WidgetClose, controls);
            }
            return Some(vec![]);
        }
        if let Some(page) = self.widget_page()
            && self.subpage.is_none()
        {
            return Some(self.widget_settings_direction(page, direction, controls));
        }
        if self.page != Page::Home || self.overlay.is_some() {
            return None;
        }
        if self.widget_edit.is_some() {
            return Some(self.widget_edit_direction(direction, controls));
        }
        // Up from a Favorites card reaches the widget above it.
        if let Target::App(_) = &self.focus
            && matches!(direction, Direction::Up)
            && let Some(app) = rect_of(controls, &self.focus)
        {
            let cx = app.center().0;
            let above = controls
                .iter()
                .filter(|c| matches!(c.target, Target::Widget(_) | Target::WidgetTile(..)))
                .filter(|c| c.rect.y < app.y && c.rect.x <= cx && cx <= c.rect.x + c.rect.w)
                .max_by(|a, b| a.rect.y.total_cmp(&b.rect.y))
                .map(|c| c.target.clone());
            if let Some(t) = above {
                self.focus(t, controls);
                return Some(vec![]);
            }
            return None;
        }
        if !is_widget(&self.focus) {
            return None;
        }
        match direction {
            Direction::Up => {
                let up = nearest_where(controls, &self.focus, Direction::Up, is_widget);
                let next = up.unwrap_or(Target::Nav(self.page_index()));
                self.focus(next, controls);
                Some(vec![])
            }
            _ => None,
        }
    }

    fn widget_edit_direction(&mut self, direction: Direction, controls: &[Control]) -> Vec<Effect> {
        let horizontal = matches!(direction, Direction::Left | Direction::Right);
        let step = |at: usize, len: usize| -> Option<usize> {
            match direction {
                Direction::Left => at.checked_sub(1),
                _ => (at + 1 < len).then_some(at + 1),
            }
        };
        match self.focus.clone() {
            Target::Widget(i) if horizontal => {
                let len = self.preferences.widgets.blocks.len();
                if let Some(j) = step(i, len) {
                    self.preferences.widgets.blocks.swap(i, j);
                    self.change_focus(Target::Widget(j));
                    return self.widget_save();
                }
                return vec![];
            }
            Target::WidgetText(i) if horizontal => {
                let len = self.preferences.widgets.text.len();
                if let Some(j) = step(i, len) {
                    self.preferences.widgets.text.swap(i, j);
                    self.change_focus(Target::WidgetText(j));
                    return self.widget_save();
                }
                return vec![];
            }
            Target::Widget(i) if matches!(direction, Direction::Up) => {
                self.focus(Target::WidgetRemove(i), controls);
                return vec![];
            }
            Target::Widget(_) if matches!(direction, Direction::Down) => {
                self.focus(Target::WidgetAdd, controls);
                return vec![];
            }
            Target::WidgetRemove(i) if matches!(direction, Direction::Down) => {
                self.focus(Target::Widget(i), controls);
                return vec![];
            }
            Target::WidgetText(i) if matches!(direction, Direction::Up) => {
                self.focus(Target::WidgetTextRemove(i), controls);
                return vec![];
            }
            Target::WidgetTextRemove(i) if matches!(direction, Direction::Down) => {
                self.focus(Target::WidgetText(i), controls);
                return vec![];
            }
            _ => {}
        }
        // Edit mode keeps focus on the widgets and their buttons.
        if let Some(next) = nearest_where(controls, &self.focus, direction, |t| {
            is_widget(t)
                || matches!(
                    t,
                    Target::WidgetRemove(_) | Target::WidgetTextRemove(_) | Target::WidgetAdd
                )
        }) {
            self.focus(next, controls);
        }
        vec![]
    }

    // ---------------------------------------------------------------- OK

    pub fn widget_activate(&mut self) -> Option<Vec<Effect>> {
        if self.page == Page::Settings
            && self.settings_level == SettingsLevel::Root
            && self.focus == Target::Sidebar(WEATHER_SECTION)
        {
            self.section = WEATHER_SECTION;
            self.settings_push(
                SettingsLevel::Widgets(SettingsPage::Root),
                Target::WidgetRow(0),
            );
            return Some(vec![]);
        }
        let editing = self.widget_edit.is_some();
        Some(match self.focus.clone() {
            Target::Widget(i) => {
                let b = *self.preferences.widgets.blocks.get(i)?;
                if editing {
                    return Some(vec![]);
                }
                self.open_widget(b.kind, b.size)
            }
            Target::WidgetText(i) => {
                let k = *self.preferences.widgets.text.get(i)?;
                if editing {
                    return Some(vec![]);
                }
                self.open_widget(k, Size::Text)
            }
            Target::WidgetTile(i, k) => {
                let b = *self.preferences.widgets.blocks.get(i)?;
                match b.kind {
                    Kind::Hdmi => {
                        let id = hdmi_inputs(self).get(k).map(|i| i.id.clone());
                        match id {
                            Some(id) if !self.launching => vec![Effect::Launch(id)],
                            _ => vec![],
                        }
                    }
                    Kind::Smart => {
                        let id = smart_devices(self).get(k).map(|e| e.id.clone());
                        id.map_or_else(Vec::new, |id| self.smart_toggle(&id))
                    }
                    _ => vec![],
                }
            }
            Target::WidgetRemove(i) => {
                if i < self.preferences.widgets.blocks.len() {
                    self.preferences.widgets.blocks.remove(i);
                }
                let n = self.preferences.widgets.blocks.len();
                self.change_focus(if n == 0 {
                    Target::WidgetAdd
                } else {
                    Target::Widget(i.min(n - 1))
                });
                self.widget_save()
            }
            Target::WidgetTextRemove(i) => {
                if i < self.preferences.widgets.text.len() {
                    self.preferences.widgets.text.remove(i);
                }
                let n = self.preferences.widgets.text.len();
                self.change_focus(if n > 0 {
                    Target::WidgetText(i.min(n - 1))
                } else if self.preferences.widgets.blocks.is_empty() {
                    Target::WidgetAdd
                } else {
                    Target::Widget(0)
                });
                self.widget_save()
            }
            Target::WidgetAdd => {
                self.widget_edit = None;
                self.open_settings();
                self.section = WEATHER_SECTION;
                self.settings_level = SettingsLevel::Widgets(SettingsPage::Add);
                self.change_focus(Target::WidgetRow(0));
                vec![]
            }
            Target::WidgetTab(i) => {
                match self.widget_detail.as_mut()? {
                    Detail::Weather { tab } => *tab = i,
                    Detail::Crypto { .. } => {
                        if let Some(p) = Period::ALL.get(i) {
                            self.preferences.widgets.period = *p;
                            return Some(self.widget_save());
                        }
                    }
                }
                vec![]
            }
            Target::WidgetCoin(k) => {
                if let Some(Detail::Crypto { coin }) = self.widget_detail.as_mut() {
                    *coin = k;
                }
                vec![]
            }
            Target::WidgetClose => {
                self.close_widget_detail();
                vec![]
            }
            Target::WidgetRow(i) => {
                let page = self.widget_page()?;
                self.widget_settings_row(page, i)
            }
            Target::WidgetSize(k) => {
                if let Some(size) = Size::ALL.get(k)
                    && self.widget_add_allowed(self.widget_add.0, *size)
                {
                    self.widget_add.1 = *size;
                }
                vec![]
            }
            _ => return None,
        })
    }

    fn smart_toggle(&mut self, id: &str) -> Vec<Effect> {
        if !self.ha_online {
            return vec![];
        }
        let Some(entity) = self.ha_entities.iter_mut().find(|e| e.id == id) else {
            return vec![];
        };
        if !entity.actionable() {
            return vec![];
        }
        let turn_on = !entity.on();
        let service = if entity.domain == "cover" {
            if turn_on { "open_cover" } else { "close_cover" }
        } else if turn_on {
            "turn_on"
        } else {
            "turn_off"
        };
        entity.state = if turn_on { "on".into() } else { "off".into() };
        self.dirty = true;
        vec![Effect::HomeAssistant(crate::smart_home::Command::Call {
            entity_id: id.to_owned(),
            service: service.into(),
            data: serde_json::json!({ "entity_id": id }),
        })]
    }

    fn open_widget(&mut self, kind: Kind, size: Size) -> Vec<Effect> {
        let back = self.focus.clone();
        match kind {
            Kind::Weather => {
                if self.preferences.weather_location.is_none() {
                    self.open_settings();
                    self.section = WEATHER_SECTION;
                    self.settings_level = SettingsLevel::Section;
                    self.change_focus(Target::WeatherLocation);
                    return vec![];
                }
                self.widget_detail = Some(Detail::Weather { tab: 0 });
                self.change_focus(Target::WidgetTab(0));
            }
            Kind::Crypto => {
                self.widget_detail = Some(Detail::Crypto { coin: 0 });
                self.change_focus(Target::WidgetCoin(0));
            }
            Kind::Hdmi => {
                let id = hdmi_inputs(self).first().map(|i| i.id.clone());
                return match id {
                    Some(id) if !self.launching => vec![Effect::Launch(id)],
                    _ => vec![],
                };
            }
            Kind::Smart => match size {
                Size::Text => {
                    self.open(2);
                    return vec![];
                }
                Size::S => {
                    let id = smart_devices(self).first().map(|e| e.id.clone());
                    return id.map_or_else(Vec::new, |id| self.smart_toggle(&id));
                }
                _ => {
                    let Some(id) = smart_camera(self).map(|e| e.id.clone()) else {
                        return vec![];
                    };
                    self.ha_camera = Some(id);
                    self.widget_return = Some(back);
                    self.change_focus(Target::SmartClose);
                    self.dirty = true;
                    return vec![];
                }
            },
        }
        self.widget_return = Some(back);
        self.widget_detail_started = Some(Instant::now());
        self.dirty = true;
        vec![]
    }
    pub fn close_widget_detail(&mut self) {
        self.widget_detail = None;
        self.widget_detail_started = None;
        let back = self
            .widget_return
            .take()
            .unwrap_or_else(|| self.first_content());
        self.change_focus(back);
        self.dirty = true;
    }
    /// After the camera opened from a widget closes, focus returns to it.
    pub fn widget_camera_closed(&mut self) -> bool {
        if let Some(back) = self.widget_return.take() {
            self.change_focus(back);
            return true;
        }
        false
    }

    // -------------------------------------------------------------- Back

    pub fn widget_back(&mut self) -> Option<Vec<Effect>> {
        if self.widget_detail.is_some() {
            self.close_widget_detail();
            return Some(vec![]);
        }
        if self.widget_edit.take().is_some() {
            self.dirty = true;
            if !is_widget(&self.focus) {
                self.change_focus(if self.preferences.widgets.blocks.is_empty() {
                    self.first_content()
                } else {
                    Target::Widget(0)
                });
            }
            return Some(self.widget_save());
        }
        if self.page != Page::Settings || self.subpage.is_some() {
            return None;
        }
        if self.settings_level == SettingsLevel::Section && self.section == WEATHER_SECTION {
            self.settings_pop(
                SettingsLevel::Widgets(SettingsPage::Root),
                Target::WidgetRow(ROOT_SETTINGS_FROM),
            );
            return Some(vec![]);
        }
        let page = self.widget_page()?;
        use SettingsPage as P;
        let (level, row) = match page {
            P::Root => {
                self.settings_pop(SettingsLevel::Root, Target::Sidebar(WEATHER_SECTION));
                return Some(vec![]);
            }
            P::Text => (P::Root, 1),
            P::Add => (P::Root, 2),
            P::Crypto => (P::Root, 4),
            P::Hdmi => (P::Root, 5),
            P::Smart => (P::Root, 6),
            P::Exchange => (P::Crypto, 0),
            P::Coins => (P::Crypto, 1),
            P::MainCoin => (P::Crypto, 2),
            P::Quote => (P::Crypto, 3),
            P::Period => (P::Crypto, 4),
            P::Refresh => (P::Crypto, 5),
            P::HdmiInput => (P::Hdmi, 0),
            P::SmartCamera => (P::Smart, 0),
            P::SmartDevices => (P::Smart, 1),
            P::SmartSensor => (P::Smart, 2),
        };
        self.settings_pop(SettingsLevel::Widgets(level), Target::WidgetRow(row));
        Some(vec![])
    }

    // -------------------------------------------------------------- hold

    /// Holding OK on a widget starts edit mode; the pointer may then carry it.
    pub fn widget_hold(&mut self) -> bool {
        if self.page != Page::Home || self.overlay.is_some() || !is_widget(&self.focus) {
            return false;
        }
        if let Target::WidgetTile(i, _) = self.focus {
            self.change_focus(Target::Widget(i));
        }
        if self.widget_edit.is_none() {
            self.widget_edit = Some(Edit { grab: None });
        }
        self.notice = None;
        self.dirty = true;
        true
    }
    pub fn widget_enter_edit(&mut self) {
        self.open(0);
        self.widget_edit = Some(Edit { grab: None });
        self.change_focus(if self.preferences.widgets.blocks.is_empty() {
            Target::WidgetAdd
        } else {
            Target::Widget(0)
        });
    }
    /// Pick a block up with the pointer at (x, y) inside `rect`.
    pub fn widget_grab(&mut self, index: usize, rect: crate::state::Rect, x: f32, y: f32) {
        if let Some(edit) = self.widget_edit.as_mut() {
            edit.grab = Some(Grab {
                index,
                offset: (x - rect.x, y - rect.y),
                at: (x, y),
            });
            self.change_focus(Target::Widget(index));
            self.dirty = true;
        }
    }
    pub fn widget_grabbed(&self) -> bool {
        self.widget_edit.is_some_and(|e| e.grab.is_some())
    }
    /// Move the carried block; it swaps places as it passes a neighbour's centre.
    pub fn widget_drag_to(&mut self, x: f32, y: f32, controls: &[Control]) -> bool {
        let Some(mut grab) = self.widget_edit.and_then(|e| e.grab) else {
            return false;
        };
        grab.at = (x, y);
        let centre = x - grab.offset.0
            + controls
                .iter()
                .find(|c| c.target == Target::Widget(grab.index))
                .map_or(0., |c| c.rect.w / 2.);
        let slots: Vec<(usize, f32)> = controls
            .iter()
            .filter_map(|c| match c.target {
                Target::Widget(i) => Some((i, c.rect.center().0)),
                _ => None,
            })
            .collect();
        let mut target = grab.index;
        for (i, cx) in &slots {
            if *i < grab.index && centre < *cx {
                target = target.min(*i);
            }
            if *i > grab.index && centre > *cx {
                target = target.max(*i);
            }
        }
        let mut changed = false;
        if target != grab.index && target < self.preferences.widgets.blocks.len() {
            let b = self.preferences.widgets.blocks.remove(grab.index);
            self.preferences.widgets.blocks.insert(target, b);
            grab.index = target;
            self.focus = Target::Widget(target);
            changed = true;
        }
        if let Some(edit) = self.widget_edit.as_mut() {
            edit.grab = Some(grab);
        }
        self.dirty = true;
        changed
    }
    /// Drop the carried block; returns true when one was carried.
    pub fn widget_drop(&mut self) -> Vec<Effect> {
        let carried = self
            .widget_edit
            .as_mut()
            .and_then(|e| e.grab.take())
            .is_some();
        if carried {
            return self.widget_save();
        }
        vec![]
    }

    // ---------------------------------------------- Settings › Widgets

    pub fn widget_add_allowed(&self, kind: Kind, size: Size) -> bool {
        let prefs = &self.preferences.widgets;
        if !kind.sizes().contains(&size) {
            return false;
        }
        if size == Size::Text {
            return prefs.text.contains(&kind) || prefs.text.len() < TEXT_MAX;
        }
        prefs.columns_used() + size.columns() <= ZONE_COLUMNS
    }
    pub fn widget_settings_rows(&self, page: SettingsPage) -> Vec<Row> {
        use SettingsPage as P;
        let prefs = &self.preferences.widgets;
        let check = |label: &str, on: bool| row(label, None, false, on);
        match page {
            P::Root => {
                let layout = if prefs.blocks.is_empty() {
                    "Text only".to_owned()
                } else {
                    prefs
                        .blocks
                        .iter()
                        .map(|b| b.size.label())
                        .collect::<Vec<_>>()
                        .join(" + ")
                };
                let city = self
                    .preferences
                    .weather_location
                    .as_ref()
                    .map_or("Set location".to_owned(), |l| l.name.clone());
                let hdmi = hdmi_inputs(self)
                    .first()
                    .map_or("No inputs".to_owned(), |i| input_name(self, i));
                let smart = if self.preferences.smart_home.server_url.is_empty() {
                    "Not connected".to_owned()
                } else {
                    format!(
                        "{} · {} devices",
                        smart_camera(self).map_or("No camera".to_owned(), |e| e.name.clone()),
                        smart_devices(self).len()
                    )
                };
                vec![
                    row("Edit layout on Home", Some(layout), true, false),
                    row(
                        "Text line",
                        Some(match prefs.text.len() {
                            0 => "Off".to_owned(),
                            1 => "1 widget".to_owned(),
                            n => format!("{n} widgets"),
                        }),
                        true,
                        false,
                    ),
                    row("Add widget", None, true, false),
                    row("Weather", Some(city), true, false),
                    row(
                        "Crypto",
                        Some(format!(
                            "{} · {}",
                            prefs.exchange.short(),
                            prefs.coins.join(", ")
                        )),
                        true,
                        false,
                    ),
                    row("HDMI input", Some(hdmi), true, false),
                    row("Smart Home", Some(smart), true, false),
                ]
            }
            P::Add => {
                let mut rows: Vec<Row> = Kind::ALL
                    .iter()
                    .map(|k| check(k.title(), self.widget_add.0 == *k))
                    .collect();
                rows.push(row("Add to Home", None, false, false));
                rows
            }
            P::Text => [Kind::Weather, Kind::Crypto, Kind::Smart]
                .iter()
                .map(|k| {
                    let mut r = check(k.title(), prefs.text.contains(k));
                    r.dim = !prefs.text.contains(k) && prefs.text.len() >= TEXT_MAX;
                    r
                })
                .collect(),
            P::Crypto => vec![
                row("Exchange", Some(prefs.exchange.short().into()), true, false),
                row("Coins", Some(prefs.coins.join(", ")), true, false),
                row("Main coin", Some(prefs.main_coin.clone()), true, false),
                row("Quote currency", Some(prefs.quote.clone()), true, false),
                row(
                    "Change period",
                    Some(prefs.period.long().into()),
                    true,
                    false,
                ),
                row(
                    "Refresh",
                    Some(refresh_label(prefs.refresh_seconds)),
                    true,
                    false,
                ),
            ],
            P::Exchange => Exchange::ALL
                .iter()
                .map(|e| check(e.title(), prefs.exchange == *e))
                .collect(),
            P::Coins => COINS
                .iter()
                .map(|c| {
                    check(
                        &format!("{}  ·  {}", c.0, c.1),
                        prefs.coins.iter().any(|s| s == c.0),
                    )
                })
                .collect(),
            P::MainCoin => prefs
                .coins
                .iter()
                .map(|s| check(s, prefs.main_coin == *s))
                .collect(),
            P::Quote => QUOTES.iter().map(|q| check(q, prefs.quote == *q)).collect(),
            P::Period => Period::ALL
                .iter()
                .map(|p| check(p.long(), prefs.period == *p))
                .collect(),
            P::Refresh => REFRESH_CHOICES
                .iter()
                .map(|s| check(&refresh_label(*s), prefs.refresh_seconds == *s))
                .collect(),
            P::Hdmi => vec![
                row(
                    "Input on M widget",
                    Some(
                        hdmi_inputs(self)
                            .first()
                            .map_or("None".to_owned(), |i| input_name(self, i)),
                    ),
                    true,
                    false,
                ),
                row(
                    "Hide inputs without signal",
                    Some(
                        if prefs.hdmi_hide_no_signal {
                            "On"
                        } else {
                            "Off"
                        }
                        .into(),
                    ),
                    false,
                    false,
                ),
                row(
                    "Names and logos",
                    Some("In Settings › Inputs".into()),
                    true,
                    false,
                ),
            ],
            P::HdmiInput => {
                let mut rows = vec![check("First connected", prefs.hdmi_input.is_empty())];
                rows.extend(
                    self.inventory
                        .inputs
                        .iter()
                        .map(|i| check(&input_name(self, i), prefs.hdmi_input == i.id)),
                );
                rows
            }
            P::Smart => vec![
                row(
                    "Camera",
                    Some(smart_camera(self).map_or("None".to_owned(), |e| e.name.clone())),
                    true,
                    false,
                ),
                row(
                    "Devices (S and L widgets)",
                    Some(if prefs.smart_devices.is_empty() {
                        "Recommended".to_owned()
                    } else {
                        format!("{} selected", prefs.smart_devices.len())
                    }),
                    true,
                    false,
                ),
                row(
                    "Text widget shows",
                    Some(smart_sensor(self).map_or("Lights only".to_owned(), |e| e.name.clone())),
                    true,
                    false,
                ),
                row(
                    "Home Assistant",
                    Some(
                        if self.ha_online {
                            "Connected"
                        } else {
                            "Offline"
                        }
                        .into(),
                    ),
                    true,
                    false,
                ),
            ],
            P::SmartCamera => {
                let mut rows = vec![check("First camera", prefs.smart_camera.is_empty())];
                rows.extend(
                    self.ha_tab(1)
                        .iter()
                        .map(|e| check(&e.name, prefs.smart_camera == e.id)),
                );
                rows
            }
            P::SmartDevices => {
                let mut rows = vec![check("Recommended", prefs.smart_devices.is_empty())];
                rows.extend(
                    self.ha_listed(0)
                        .into_iter()
                        .filter(|e| e.actionable() || e.domain == "lock" || e.domain == "sensor")
                        .map(|e| {
                            let on = prefs.smart_devices.contains(&e.id);
                            let mut r = check(&e.name, on);
                            r.dim = !on && prefs.smart_devices.len() >= 4;
                            r
                        }),
                );
                rows
            }
            P::SmartSensor => {
                let mut rows = vec![check("Lights only", prefs.smart_sensor.is_empty())];
                rows.extend(
                    text_candidates(self)
                        .into_iter()
                        .map(|e| check(&e.name, prefs.smart_sensor == e.id)),
                );
                rows
            }
        }
    }

    fn widget_settings_direction(
        &mut self,
        page: SettingsPage,
        direction: Direction,
        controls: &[Control],
    ) -> Vec<Effect> {
        let n = self.widget_settings_rows(page).len();
        let sizes = page == SettingsPage::Add;
        let next = match (&self.focus, direction) {
            (Target::SettingsBack, Direction::Right | Direction::Down) => {
                Some(Target::WidgetRow(0))
            }
            (Target::SettingsBack, Direction::Up) => Some(Target::Nav(self.settings_nav_index())),
            (Target::SettingsBack, _) => None,
            (Target::WidgetRow(_), Direction::Left) | (Target::WidgetSize(0), Direction::Left) => {
                Some(Target::SettingsBack)
            }
            (Target::WidgetRow(0), Direction::Up) => Some(Target::SettingsBack),
            (Target::WidgetRow(i), Direction::Up) if sizes && *i + 1 == n => {
                Some(Target::WidgetSize(
                    Size::ALL
                        .iter()
                        .position(|s| *s == self.widget_add.1)
                        .unwrap_or(0),
                ))
            }
            (Target::WidgetRow(i), Direction::Up) => Some(Target::WidgetRow(i - 1)),
            (Target::WidgetRow(i), Direction::Down) if sizes && *i + 2 == n => {
                Some(Target::WidgetSize(
                    Size::ALL
                        .iter()
                        .position(|s| *s == self.widget_add.1)
                        .unwrap_or(0),
                ))
            }
            (Target::WidgetRow(i), Direction::Down) => {
                Some(Target::WidgetRow((i + 1).min(n.saturating_sub(1))))
            }
            (Target::WidgetRow(_), Direction::Right) => {
                return self.widget_activate().unwrap_or_default();
            }
            (Target::WidgetSize(_), Direction::Up) => Some(Target::WidgetRow(n.saturating_sub(2))),
            (Target::WidgetSize(_), Direction::Down) => {
                Some(Target::WidgetRow(n.saturating_sub(1)))
            }
            (Target::WidgetSize(k), Direction::Left) => Some(Target::WidgetSize(k - 1)),
            (Target::WidgetSize(k), Direction::Right) => Some(Target::WidgetSize((k + 1).min(3))),
            (Target::Nav(_), Direction::Down) => Some(Target::WidgetRow(0)),
            _ => None,
        };
        if let Some(next) = next {
            self.focus(next, controls);
        }
        vec![]
    }

    fn widget_settings_row(&mut self, page: SettingsPage, i: usize) -> Vec<Effect> {
        use SettingsPage as P;
        let push = |s: &mut Self, p: SettingsPage, row: usize| {
            s.settings_push(SettingsLevel::Widgets(p), Target::WidgetRow(row));
            vec![]
        };
        let prefs = &mut self.preferences.widgets;
        match page {
            P::Root => match i {
                0 => {
                    self.widget_enter_edit();
                    vec![]
                }
                1 => push(self, P::Text, 0),
                2 => {
                    let first = Kind::ALL
                        .iter()
                        .copied()
                        .find(|k| k.sizes().iter().any(|s| self.widget_add_allowed(*k, *s)))
                        .unwrap_or(Kind::Weather);
                    let size = first
                        .sizes()
                        .iter()
                        .copied()
                        .find(|s| *s != Size::Text && self.widget_add_allowed(first, *s))
                        .unwrap_or(Size::Text);
                    self.widget_add = (first, size);
                    push(self, P::Add, 0)
                }
                3 => {
                    self.section = WEATHER_SECTION;
                    self.settings_push(SettingsLevel::Section, Target::WeatherToggle);
                    vec![]
                }
                4 => push(self, P::Crypto, 0),
                5 => push(self, P::Hdmi, 0),
                6 => push(self, P::Smart, 0),
                _ => vec![],
            },
            P::Add => {
                if let Some(kind) = Kind::ALL.get(i) {
                    self.widget_add.0 = *kind;
                    if !self.widget_add_allowed(*kind, self.widget_add.1) {
                        self.widget_add.1 = kind
                            .sizes()
                            .iter()
                            .copied()
                            .find(|s| self.widget_add_allowed(*kind, *s))
                            .unwrap_or(kind.sizes()[0]);
                    }
                    self.dirty = true;
                    return vec![];
                }
                let (kind, size) = self.widget_add;
                if !self.widget_add_allowed(kind, size) {
                    self.notify("No room: remove a widget or choose a smaller size");
                    return vec![];
                }
                let prefs = &mut self.preferences.widgets;
                if size == Size::Text {
                    if !prefs.text.contains(&kind) {
                        prefs.text.push(kind);
                    }
                } else {
                    prefs.blocks.push(Block { kind, size });
                }
                self.notify(format!("{} {} added to Home", kind.title(), size.label()));
                // Smart Home: straight to choosing what it shows.
                match (kind, size) {
                    (Kind::Smart, Size::Text) => self.settings_push(
                        SettingsLevel::Widgets(P::SmartSensor),
                        Target::WidgetRow(0),
                    ),
                    (Kind::Smart, Size::S | Size::L) => self.settings_push(
                        SettingsLevel::Widgets(P::SmartDevices),
                        Target::WidgetRow(0),
                    ),
                    (Kind::Smart, _) => self.settings_push(
                        SettingsLevel::Widgets(P::SmartCamera),
                        Target::WidgetRow(0),
                    ),
                    _ => self.settings_pop(SettingsLevel::Widgets(P::Root), Target::WidgetRow(2)),
                }
                self.widget_save()
            }
            P::Text => {
                let kind = [Kind::Weather, Kind::Crypto, Kind::Smart][i.min(2)];
                if let Some(at) = prefs.text.iter().position(|k| *k == kind) {
                    prefs.text.remove(at);
                } else if prefs.text.len() < TEXT_MAX {
                    prefs.text.push(kind);
                }
                self.widget_save()
            }
            P::Crypto => push(
                self,
                [
                    P::Exchange,
                    P::Coins,
                    P::MainCoin,
                    P::Quote,
                    P::Period,
                    P::Refresh,
                ][i.min(5)],
                0,
            ),
            P::Exchange => {
                prefs.exchange = Exchange::ALL[i.min(Exchange::ALL.len() - 1)];
                self.widget_save()
            }
            P::Coins => {
                let symbol = COINS[i.min(COINS.len() - 1)].0.to_owned();
                if let Some(at) = prefs.coins.iter().position(|s| *s == symbol) {
                    if prefs.coins.len() > 1 {
                        prefs.coins.remove(at);
                    }
                } else if prefs.coins.len() < 8 {
                    prefs.coins.push(symbol);
                }
                self.widget_save()
            }
            P::MainCoin => {
                if let Some(s) = prefs.coins.get(i) {
                    prefs.main_coin = s.clone();
                }
                self.widget_save()
            }
            P::Quote => {
                prefs.quote = QUOTES[i.min(QUOTES.len() - 1)].into();
                self.widget_save()
            }
            P::Period => {
                prefs.period = Period::ALL[i.min(3)];
                self.widget_save()
            }
            P::Refresh => {
                prefs.refresh_seconds = REFRESH_CHOICES[i.min(REFRESH_CHOICES.len() - 1)];
                self.widget_save()
            }
            P::Hdmi => match i {
                0 => push(self, P::HdmiInput, 0),
                1 => {
                    prefs.hdmi_hide_no_signal = !prefs.hdmi_hide_no_signal;
                    self.widget_save()
                }
                _ => {
                    self.section = crate::state::INPUTS_SECTION;
                    let target = self
                        .inventory
                        .inputs
                        .first()
                        .map(|input| Target::InputMode(input.id.clone()))
                        .unwrap_or(Target::Setting(3));
                    self.settings_push(SettingsLevel::Section, target);
                    vec![]
                }
            },
            P::HdmiInput => {
                prefs.hdmi_input = if i == 0 {
                    String::new()
                } else {
                    self.inventory
                        .inputs
                        .get(i - 1)
                        .map(|x| x.id.clone())
                        .unwrap_or_default()
                };
                self.widget_save()
            }
            P::Smart => match i {
                0 => push(self, P::SmartCamera, 0),
                1 => push(self, P::SmartDevices, 0),
                2 => push(self, P::SmartSensor, 0),
                _ => {
                    self.section = crate::state::SMARTHOME_SECTION;
                    let target = if self.preferences.smart_home.server_url.is_empty() {
                        Target::SmartConnect
                    } else {
                        Target::SmartShowTab
                    };
                    self.settings_push(SettingsLevel::Section, target);
                    vec![]
                }
            },
            P::SmartCamera => {
                let id = if i == 0 {
                    String::new()
                } else {
                    self.ha_tab(1)
                        .get(i - 1)
                        .map(|e| e.id.clone())
                        .unwrap_or_default()
                };
                self.preferences.widgets.smart_camera = id;
                self.widget_save()
            }
            P::SmartDevices => {
                if i == 0 {
                    self.preferences.widgets.smart_devices.clear();
                    return self.widget_save();
                }
                let id = self
                    .ha_listed(0)
                    .into_iter()
                    .filter(|e| e.actionable() || e.domain == "lock" || e.domain == "sensor")
                    .nth(i - 1)
                    .map(|e| e.id.clone());
                if let Some(id) = id {
                    let list = &mut self.preferences.widgets.smart_devices;
                    if let Some(at) = list.iter().position(|x| *x == id) {
                        list.remove(at);
                    } else if list.len() < 4 {
                        list.push(id);
                    }
                }
                self.widget_save()
            }
            P::SmartSensor => {
                let id = if i == 0 {
                    String::new()
                } else {
                    text_candidates(self)
                        .get(i - 1)
                        .map(|e| e.id.clone())
                        .unwrap_or_default()
                };
                self.preferences.widgets.smart_sensor = id;
                self.widget_save()
            }
        }
    }
}

pub fn refresh_label(seconds: u32) -> String {
    match seconds {
        s if s < 60 => format!("Every {s} s"),
        60 => "Every minute".into(),
        s => format!("Every {} minutes", s / 60),
    }
}
