//! Home widgets: the layout preferences, the grid math and the background
//! data they show (extended Open-Meteo forecast).
//! No request runs on the SDL thread; workers hand results over a channel.
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    io::Read,
    sync::{Mutex, OnceLock, mpsc},
    thread,
    time::{Duration, Instant},
};

/// Widgets live in Home grid columns 4–6, right of the headline.
pub const ZONE_COLUMNS: usize = 3;
/// Block widget height (the approved weather card height).
pub const BLOCK_H: f32 = 322.;
/// One widget plate radius; nested plates use `inner_radius`.
pub const RADIUS: f32 = 28.;
/// Text line: up to three items above the blocks.
pub const TEXT_MAX: usize = 3;

/// Concentric corners: a plate inset by `padding` inside one with `outer`
/// radius keeps the same corner centre (Apple's rule).
pub fn inner_radius(outer: f32, padding: f32) -> f32 {
    (outer - padding).max(4.)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Weather,
    Hdmi,
    Smart,
}
impl Kind {
    pub const ALL: [Self; 3] = [Self::Weather, Self::Hdmi, Self::Smart];
    /// Widgets that have a one-line text form.
    pub const TEXT: [Self; 2] = [Self::Weather, Self::Smart];
    pub fn title(self) -> &'static str {
        match self {
            Self::Weather => "Weather",
            Self::Hdmi => "HDMI input",
            Self::Smart => "Smart Home",
        }
    }
    /// HDMI has no text form and no one-column form.
    pub fn sizes(self) -> &'static [Size] {
        match self {
            Self::Hdmi => &[Size::M, Size::L],
            _ => &[Size::Text, Size::S, Size::M, Size::L],
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Size {
    Text,
    S,
    M,
    L,
}
impl Size {
    pub const ALL: [Self; 4] = [Self::Text, Self::S, Self::M, Self::L];
    pub fn columns(self) -> usize {
        match self {
            Self::Text => 0,
            Self::S => 1,
            Self::M => 2,
            Self::L => 3,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Text => "Text",
            Self::S => "S",
            Self::M => "M",
            Self::L => "L",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Block {
    pub kind: Kind,
    pub size: Size,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct WidgetPrefs {
    /// Text line, left to right. Entries of removed widget kinds (the old
    /// crypto widget) are dropped on load instead of failing the whole file.
    #[serde(deserialize_with = "known_entries")]
    pub text: Vec<Kind>,
    /// Blocks, left to right; together at most three columns.
    #[serde(deserialize_with = "known_entries")]
    pub blocks: Vec<Block>,
    /// HDMI M widget input id; empty = the first connected input.
    pub hdmi_input: String,
    pub hdmi_live: bool,
    pub hdmi_hide_no_signal: bool,
    /// Smart Home camera entity for M/L; empty = the first camera.
    pub smart_camera: String,
    /// Devices on the L widget and the S device; empty = recommended.
    pub smart_devices: Vec<String>,
    /// Text line sensor; empty = first temperature sensor.
    pub smart_sensor: String,
}
impl Default for WidgetPrefs {
    fn default() -> Self {
        Self {
            text: vec![Kind::Weather, Kind::Smart],
            blocks: vec![Block {
                kind: Kind::Weather,
                size: Size::M,
            }],
            hdmi_input: String::new(),
            hdmi_live: true,
            hdmi_hide_no_signal: false,
            smart_camera: String::new(),
            smart_devices: vec![],
            smart_sensor: String::new(),
        }
    }
}
/// Reads a list, skipping entries that no longer parse (removed kinds).
fn known_entries<'de, D, T>(deserializer: D) -> std::result::Result<Vec<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: serde::de::DeserializeOwned,
{
    let raw = Vec::<Value>::deserialize(deserializer)?;
    Ok(raw
        .into_iter()
        .filter_map(|v| serde_json::from_value(v).ok())
        .collect())
}

impl WidgetPrefs {
    /// Repair anything a hand-edited or old file could hold.
    pub fn normalize(&mut self) {
        let mut seen = Vec::new();
        self.text.retain(|k| {
            let keep = *k != Kind::Hdmi && !seen.contains(k);
            seen.push(*k);
            keep
        });
        self.text.truncate(TEXT_MAX);
        let mut used = 0;
        self.blocks.retain_mut(|b| {
            if b.size == Size::Text {
                return false;
            }
            if b.kind == Kind::Hdmi && b.size == Size::S {
                b.size = Size::M;
            }
            if used + b.size.columns() > ZONE_COLUMNS {
                return false;
            }
            used += b.size.columns();
            true
        });
    }
    pub fn columns_used(&self) -> usize {
        self.blocks.iter().map(|b| b.size.columns()).sum()
    }
    pub fn uses(&self, kind: Kind) -> bool {
        self.text.contains(&kind) || self.blocks.iter().any(|b| b.kind == kind)
    }
}

/// Block x and width on the Home grid, and the left edge of the zone.
pub struct Layout {
    pub blocks: Vec<(f32, f32)>,
    /// Left edge of everything; the headline wraps 44 px before it.
    pub left: f32,
}
/// Columns run right to left from the strip's right edge, so one M widget
/// stays at columns 5–6 exactly where the weather card used to be.
pub fn layout(blocks: &[Block], has_text: bool, pitch: f32, tile: f32, strip_right: f32) -> Layout {
    let used: usize = blocks
        .iter()
        .map(|b| b.size.columns())
        .sum::<usize>()
        .min(ZONE_COLUMNS);
    let start_col = ZONE_COLUMNS - used;
    let zone_left = strip_right - (ZONE_COLUMNS as f32 - 1.) * pitch - tile;
    let mut col = start_col;
    let mut out = Vec::new();
    for b in blocks {
        let x = zone_left + col as f32 * pitch;
        let w = (b.size.columns() as f32 - 1.) * pitch + tile;
        out.push((x, w));
        col += b.size.columns();
    }
    let left = if used > 0 {
        zone_left + start_col as f32 * pitch
    } else if has_text {
        zone_left
    } else {
        strip_right
    };
    Layout { blocks: out, left }
}

/// Full-screen page a widget opened.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Detail {
    /// 0 today, 1 tomorrow, 2 ten days.
    Weather { tab: usize },
}

/// Home edit mode; `grab` is the block the pointer carries.
#[derive(Clone, Copy, Debug)]
pub struct Edit {
    pub grab: Option<Grab>,
}
#[derive(Clone, Copy, Debug)]
pub struct Grab {
    pub index: usize,
    /// Pointer position inside the block when it was picked up.
    pub offset: (f32, f32),
    pub at: (f32, f32),
}

/// Pages of Settings › Widgets.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum SettingsPage {
    Root,
    Add,
    Text,
    Hdmi,
    HdmiInput,
    Smart,
    SmartCamera,
    SmartDevices,
    SmartSensor,
}

// ---------------------------------------------------------------- data

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Day {
    /// 0 = Monday.
    pub weekday: u8,
    pub code: u8,
    pub max: f64,
    pub min: f64,
    pub rain_mm: f64,
    pub rain_chance: f64,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Hour {
    /// 0 today, 1 tomorrow.
    pub day: u8,
    pub hour: u8,
    pub temperature: f64,
    pub code: u8,
    pub is_day: bool,
    pub rain_chance: f64,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Forecast {
    pub key: String,
    pub temperature: f64,
    pub feels_like: f64,
    pub code: u8,
    pub is_day: bool,
    pub humidity: f64,
    pub dew_point: f64,
    pub wind_ms: f64,
    pub gust_ms: f64,
    pub wind_dir: f64,
    pub pressure: f64,
    pub uv: f64,
    pub visibility_km: f64,
    pub sunrise: String,
    pub sunset: String,
    pub hours: Vec<Hour>,
    pub days: Vec<Day>,
}

#[derive(Default)]
pub struct Data {
    pub forecast: Option<Forecast>,
    pub forecast_at: Option<Instant>,
    pending_forecast: bool,
}

pub enum Job {
    Forecast {
        key: String,
        latitude: f64,
        longitude: f64,
    },
}
pub enum Result {
    Forecast(Option<Forecast>),
}

pub struct Service {
    jobs: mpsc::Sender<Job>,
    pub results: Mutex<mpsc::Receiver<Result>>,
}
pub fn service() -> &'static Service {
    static SERVICE: OnceLock<Service> = OnceLock::new();
    SERVICE.get_or_init(|| {
        let (jobs, inbox) = mpsc::channel::<Job>();
        let (out, results) = mpsc::channel();
        thread::spawn(move || {
            let agent = ureq::Agent::config_builder()
                .timeout_global(Some(Duration::from_secs(10)))
                .max_redirects(2)
                .proxy(None)
                .build()
                .new_agent();
            for job in inbox {
                let result = match job {
                    Job::Forecast {
                        key,
                        latitude,
                        longitude,
                    } => Result::Forecast(fetch_forecast(&agent, &key, latitude, longitude)),
                };
                if out.send(result).is_err() {
                    break;
                }
            }
        });
        Service {
            jobs,
            results: Mutex::new(results),
        }
    })
}

/// Called every frame: asks for fresh data when a widget needs it and takes
/// finished results. Returns true when something visible changed.
pub fn tick(
    data: &mut Data,
    prefs: &WidgetPrefs,
    location: Option<&crate::weather::Location>,
    active: bool,
    detail_open: bool,
) -> bool {
    let mut changed = false;
    if let Ok(results) = service().results.try_lock() {
        while let Ok(result) = results.try_recv() {
            changed = true;
            match result {
                Result::Forecast(forecast) => {
                    data.pending_forecast = false;
                    data.forecast_at = Some(Instant::now());
                    if forecast.is_some() {
                        data.forecast = forecast;
                    }
                }
            }
        }
    }
    if !active && !detail_open {
        return changed;
    }
    if let Some(place) = location
        && (prefs.uses(Kind::Weather) || detail_open)
        && !data.pending_forecast
    {
        let key = place.key();
        let stale = data.forecast.as_ref().is_none_or(|f| f.key != key)
            && data
                .forecast_at
                .is_none_or(|at| at.elapsed() >= Duration::from_secs(60))
            || data
                .forecast_at
                .is_none_or(|at| at.elapsed() >= Duration::from_secs(15 * 60));
        if stale {
            data.pending_forecast = true;
            let _ = service().jobs.send(Job::Forecast {
                key,
                latitude: place.latitude,
                longitude: place.longitude,
            });
        }
    }
    changed
}

const LIMIT: u64 = 512 * 1024;
fn get_json(agent: &ureq::Agent, url: &str) -> Option<Value> {
    let mut response = agent
        .get(url)
        .header("Accept", "application/json")
        .header("User-Agent", "LumaNative")
        .call()
        .ok()?;
    if !response.status().is_success() {
        return None;
    }
    let mut bytes = Vec::new();
    response
        .body_mut()
        .as_reader()
        .take(LIMIT)
        .read_to_end(&mut bytes)
        .ok()?;
    serde_json::from_slice(&bytes).ok()
}
fn num(v: &Value) -> Option<f64> {
    v.as_f64()
        .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
        .filter(|n: &f64| n.is_finite())
}

fn fetch_forecast(
    agent: &ureq::Agent,
    key: &str,
    latitude: f64,
    longitude: f64,
) -> Option<Forecast> {
    let url = format!(
        "https://api.open-meteo.com/v1/forecast?latitude={latitude}&longitude={longitude}\
&current=temperature_2m,apparent_temperature,relative_humidity_2m,dew_point_2m,is_day,weather_code,wind_speed_10m,wind_gusts_10m,wind_direction_10m,surface_pressure,uv_index,visibility\
&hourly=temperature_2m,weather_code,is_day,precipitation_probability\
&daily=weather_code,temperature_2m_max,temperature_2m_min,sunrise,sunset,precipitation_sum,precipitation_probability_max\
&forecast_days=10&wind_speed_unit=ms&timezone=auto"
    );
    parse_forecast(key, &get_json(agent, &url)?)
}
pub fn parse_forecast(key: &str, v: &Value) -> Option<Forecast> {
    let c = &v["current"];
    let at = |name: &str, i: usize| num(&v["hourly"][name][i]);
    let mut hours = Vec::new();
    let times = v["hourly"]["time"].as_array()?;
    let now = c["time"].as_str().and_then(|t| t.get(..13)).unwrap_or("");
    let today = now.get(..10).unwrap_or("");
    let mut tomorrow: Option<&str> = None;
    for (i, t) in times.iter().enumerate() {
        let Some(t) = t.as_str() else { continue };
        if t.get(..13).is_some_and(|h| h < now) {
            continue;
        }
        let date = t.get(..10).unwrap_or("");
        let day = if date == today {
            0
        } else if *tomorrow.get_or_insert(date) == date {
            1
        } else {
            break;
        };
        let (Some(hour), Some(temperature)) = (
            t.get(11..13).and_then(|h| h.parse().ok()),
            at("temperature_2m", i),
        ) else {
            continue;
        };
        hours.push(Hour {
            day,
            hour,
            temperature,
            code: at("weather_code", i).unwrap_or(0.).clamp(0., 99.) as u8,
            is_day: at("is_day", i).unwrap_or(1.) == 1.,
            rain_chance: at("precipitation_probability", i).unwrap_or(0.),
        });
        if hours.len() == 48 {
            break;
        }
    }
    let d = &v["daily"];
    let dat = |name: &str, i: usize| num(&d[name][i]);
    let mut days = Vec::new();
    for (i, t) in d["time"].as_array()?.iter().enumerate() {
        let date = t.as_str().and_then(|s| {
            time::Date::parse(s, time::macros::format_description!("[year]-[month]-[day]")).ok()
        })?;
        days.push(Day {
            weekday: date.weekday().number_days_from_monday(),
            code: dat("weather_code", i).unwrap_or(0.).clamp(0., 99.) as u8,
            max: dat("temperature_2m_max", i)?,
            min: dat("temperature_2m_min", i)?,
            rain_mm: dat("precipitation_sum", i).unwrap_or(0.),
            rain_chance: dat("precipitation_probability_max", i).unwrap_or(0.),
        });
    }
    let clock = |s: &Value| {
        s.as_str()
            .and_then(|s| s.get(11..16))
            .unwrap_or("--:--")
            .to_owned()
    };
    Some(Forecast {
        key: key.into(),
        temperature: num(&c["temperature_2m"])?,
        feels_like: num(&c["apparent_temperature"]).unwrap_or(f64::NAN),
        code: num(&c["weather_code"]).unwrap_or(0.).clamp(0., 99.) as u8,
        is_day: num(&c["is_day"]).unwrap_or(1.) == 1.,
        humidity: num(&c["relative_humidity_2m"]).unwrap_or(f64::NAN),
        dew_point: num(&c["dew_point_2m"]).unwrap_or(f64::NAN),
        wind_ms: num(&c["wind_speed_10m"]).unwrap_or(0.),
        gust_ms: num(&c["wind_gusts_10m"]).unwrap_or(0.),
        wind_dir: num(&c["wind_direction_10m"]).unwrap_or(0.),
        pressure: num(&c["surface_pressure"]).unwrap_or(f64::NAN),
        uv: num(&c["uv_index"]).unwrap_or(0.),
        visibility_km: num(&c["visibility"]).map_or(f64::NAN, |m| m / 1000.),
        sunrise: clock(&d["sunrise"][0]),
        sunset: clock(&d["sunset"][0]),
        hours,
        days,
    })
}

pub const WEEKDAYS: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];

#[cfg(test)]
mod tests {
    use super::*;
    const PITCH: f32 = 296.4;
    const TILE: f32 = 270.;
    const RIGHT: f32 = 1836.;
    #[test]
    fn widths_match_grid_columns_and_end_at_the_strip_edge() {
        let prefs = WidgetPrefs::default();
        let l = layout(&prefs.blocks, true, PITCH, TILE, RIGHT);
        // A lone M weather block sits at columns 5–6.
        assert!((l.blocks[0].1 - 566.4).abs() < 0.01);
        assert!((l.blocks[0].0 + l.blocks[0].1 - RIGHT).abs() < 0.01);
        assert!((l.left - (84. + 4. * PITCH)).abs() < 0.01);
        let one = WidgetPrefs {
            blocks: vec![Block {
                kind: Kind::Weather,
                size: Size::M,
            }],
            ..Default::default()
        };
        // A lone M stays at columns 5–6, where the weather card was.
        let l = layout(&one.blocks, false, PITCH, TILE, RIGHT);
        assert!((l.blocks[0].0 - (84. + 4. * PITCH)).abs() < 0.01);
        assert_eq!(layout(&[], false, PITCH, TILE, RIGHT).left, RIGHT);
    }
    #[test]
    fn blocks_never_exceed_three_columns_and_hdmi_has_no_s() {
        let mut p = WidgetPrefs {
            blocks: vec![
                Block {
                    kind: Kind::Hdmi,
                    size: Size::S,
                },
                Block {
                    kind: Kind::Weather,
                    size: Size::M,
                },
            ],
            text: vec![
                Kind::Hdmi,
                Kind::Weather,
                Kind::Weather,
                Kind::Smart,
                Kind::Weather,
            ],
            ..Default::default()
        };
        p.normalize();
        assert_eq!(p.blocks.len(), 1);
        assert_eq!(p.blocks[0].size, Size::M);
        assert!(p.columns_used() <= ZONE_COLUMNS);
        assert_eq!(p.text, vec![Kind::Weather, Kind::Smart]);
    }
    #[test]
    fn nested_corners_are_concentric() {
        assert_eq!(inner_radius(RADIUS, 16.), 12.);
        assert_eq!(inner_radius(RADIUS, 20.), 8.);
        assert_eq!(inner_radius(RADIUS + 3., 3.), RADIUS);
    }
    #[test]
    fn old_preferences_without_widgets_get_the_default_layout() {
        let p: WidgetPrefs = serde_json::from_str("{}").unwrap();
        assert_eq!(p, WidgetPrefs::default());
    }
    #[test]
    fn saved_crypto_widgets_are_dropped_on_load() {
        let p: WidgetPrefs = serde_json::from_str(
            r#"{"text":["weather","crypto","smart"],"blocks":[{"kind":"crypto","size":"s"},{"kind":"weather","size":"m"}],"exchange":"kraken","period":"7d"}"#,
        )
        .unwrap();
        assert_eq!(p.text, vec![Kind::Weather, Kind::Smart]);
        assert_eq!(
            p.blocks,
            vec![Block {
                kind: Kind::Weather,
                size: Size::M
            }]
        );
    }
    #[test]
    fn forecast_parses_open_meteo_payload() {
        let v: Value = serde_json::json!({
            "current": {"time": "2026-10-25T13:00", "temperature_2m": 22.1, "weather_code": 2,
                "is_day": 1, "relative_humidity_2m": 62, "wind_speed_10m": 4.2, "uv_index": 3.1},
            "hourly": {"time": ["2026-10-25T12:00", "2026-10-25T13:00", "2026-10-25T14:00"],
                "temperature_2m": [21, 22, 23], "weather_code": [1, 2, 3], "is_day": [1, 1, 1],
                "precipitation_probability": [0, 10, 20]},
            "daily": {"time": ["2026-10-25", "2026-10-26"], "weather_code": [2, 61],
                "temperature_2m_max": [26, 24], "temperature_2m_min": [15, 14],
                "sunrise": ["2026-10-25T07:42", "x"], "sunset": ["2026-10-25T18:31", "y"]}
        });
        let f = parse_forecast("k", &v).unwrap();
        assert_eq!(f.hours.len(), 2);
        assert_eq!(f.hours[0].hour, 13);
        assert_eq!(f.days[0].weekday, 6);
        assert_eq!(f.days[1].max, 24.);
        assert_eq!(f.sunset, "18:31");
    }
}
