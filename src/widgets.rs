//! Home widgets: the layout preferences, the grid math and the background
//! data they show (extended Open-Meteo forecast, public crypto prices).
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
    Crypto,
    Hdmi,
    Smart,
}
impl Kind {
    pub const ALL: [Self; 4] = [Self::Weather, Self::Crypto, Self::Hdmi, Self::Smart];
    pub fn title(self) -> &'static str {
        match self {
            Self::Weather => "Weather",
            Self::Crypto => "Crypto",
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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Exchange {
    #[default]
    Binance,
    Coinbase,
    Kraken,
    Bybit,
    Okx,
    Bitfinex,
    Coingecko,
}
impl Exchange {
    pub const ALL: [Self; 7] = [
        Self::Binance,
        Self::Coinbase,
        Self::Kraken,
        Self::Bybit,
        Self::Okx,
        Self::Bitfinex,
        Self::Coingecko,
    ];
    pub fn title(self) -> &'static str {
        match self {
            Self::Binance => "Binance",
            Self::Coinbase => "Coinbase",
            Self::Kraken => "Kraken",
            Self::Bybit => "Bybit",
            Self::Okx => "OKX",
            Self::Bitfinex => "Bitfinex",
            Self::Coingecko => "CoinGecko (average price)",
        }
    }
    pub fn short(self) -> &'static str {
        match self {
            Self::Coingecko => "CoinGecko",
            other => other.title(),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Period {
    #[serde(rename = "1h")]
    Hour,
    #[default]
    #[serde(rename = "24h")]
    Day,
    #[serde(rename = "7d")]
    Week,
    #[serde(rename = "1y")]
    Year,
}
impl Period {
    pub const ALL: [Self; 4] = [Self::Hour, Self::Day, Self::Week, Self::Year];
    pub fn label(self) -> &'static str {
        match self {
            Self::Hour => "1H",
            Self::Day => "24H",
            Self::Week => "7D",
            Self::Year => "1Y",
        }
    }
    pub fn long(self) -> &'static str {
        match self {
            Self::Hour => "1 hour",
            Self::Day => "24 hours",
            Self::Week => "7 days",
            Self::Year => "1 year",
        }
    }
}

/// Popular coins offered in Settings: (symbol, name, brand colour, glyph, CoinGecko id).
pub const COINS: [(&str, &str, u32, &str, &str); 12] = [
    ("BTC", "Bitcoin", 0xf7931a, "₿", "bitcoin"),
    ("ETH", "Ethereum", 0x627eea, "Ξ", "ethereum"),
    ("SOL", "Solana", 0x7b4fe0, "S", "solana"),
    ("TON", "Toncoin", 0x0098ea, "T", "the-open-network"),
    ("XRP", "XRP", 0x2a2f36, "X", "ripple"),
    ("DOGE", "Dogecoin", 0xc2a633, "Ð", "dogecoin"),
    ("BNB", "BNB", 0xf3ba2f, "B", "binancecoin"),
    ("ADA", "Cardano", 0x0033ad, "A", "cardano"),
    ("TRX", "TRON", 0xeb0029, "T", "tron"),
    ("AVAX", "Avalanche", 0xe84142, "A", "avalanche-2"),
    ("LTC", "Litecoin", 0x345d9d, "Ł", "litecoin"),
    ("LINK", "Chainlink", 0x2a5ada, "L", "chainlink"),
];
pub fn coin(symbol: &str) -> (&'static str, u32, &'static str) {
    COINS
        .iter()
        .find(|c| c.0 == symbol)
        .map(|c| (c.1, c.2, c.3))
        .unwrap_or(("", 0x3a3f47, "•"))
}
pub const QUOTES: [&str; 3] = ["USD", "EUR", "USDT"];
pub const REFRESH_CHOICES: [u32; 4] = [15, 30, 60, 300];

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct WidgetPrefs {
    /// Text line, left to right.
    pub text: Vec<Kind>,
    /// Blocks, left to right; together at most three columns.
    pub blocks: Vec<Block>,
    pub exchange: Exchange,
    pub coins: Vec<String>,
    pub main_coin: String,
    pub quote: String,
    pub period: Period,
    pub refresh_seconds: u32,
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
            text: vec![Kind::Weather, Kind::Crypto, Kind::Smart],
            blocks: vec![
                Block {
                    kind: Kind::Weather,
                    size: Size::M,
                },
                Block {
                    kind: Kind::Crypto,
                    size: Size::S,
                },
            ],
            exchange: Exchange::Binance,
            coins: vec!["BTC".into(), "ETH".into(), "SOL".into()],
            main_coin: "BTC".into(),
            quote: "USD".into(),
            period: Period::Day,
            refresh_seconds: 30,
            hdmi_input: String::new(),
            hdmi_live: true,
            hdmi_hide_no_signal: false,
            smart_camera: String::new(),
            smart_devices: vec![],
            smart_sensor: String::new(),
        }
    }
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
        self.coins.retain(|c| !c.trim().is_empty());
        self.coins.dedup();
        self.coins.truncate(8);
        if self.coins.is_empty() {
            self.coins = vec!["BTC".into()];
        }
        if !self.coins.contains(&self.main_coin) {
            self.main_coin = self.coins[0].clone();
        }
        if !QUOTES.contains(&self.quote.as_str()) {
            self.quote = "USD".into();
        }
        if !REFRESH_CHOICES.contains(&self.refresh_seconds) {
            self.refresh_seconds = 30;
        }
    }
    pub fn columns_used(&self) -> usize {
        self.blocks.iter().map(|b| b.size.columns()).sum()
    }
    pub fn uses(&self, kind: Kind) -> bool {
        self.text.contains(&kind) || self.blocks.iter().any(|b| b.kind == kind)
    }
    /// Coins shown, main coin first.
    pub fn ordered_coins(&self) -> Vec<String> {
        let mut out = vec![self.main_coin.clone()];
        out.extend(self.coins.iter().filter(|c| **c != self.main_coin).cloned());
        out
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
    /// Index into `WidgetPrefs::ordered_coins`.
    Crypto { coin: usize },
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
    Crypto,
    Exchange,
    Coins,
    MainCoin,
    Quote,
    Period,
    Refresh,
    Hdmi,
    HdmiInput,
    Smart,
    SmartCamera,
    SmartDevices,
    SmartSensor,
}

// ---------------------------------------------------------------- data

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Quote {
    pub symbol: String,
    pub price: f64,
    pub change_pct: f64,
    /// Oldest first, for the sparkline and the full-screen chart.
    pub series: Vec<f64>,
    pub high: f64,
    pub low: f64,
}

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
    pub quotes: Vec<Quote>,
    /// Quote set the prices were asked for (exchange, quote, period).
    pub quotes_for: Option<(Exchange, String, Period)>,
    pub quotes_at: Option<Instant>,
    pub quotes_failed: bool,
    pub forecast: Option<Forecast>,
    pub forecast_at: Option<Instant>,
    pending_quotes: bool,
    pending_forecast: bool,
}
impl Data {
    pub fn quote(&self, symbol: &str) -> Option<&Quote> {
        self.quotes.iter().find(|q| q.symbol == symbol)
    }
}

pub enum Job {
    Quotes {
        exchange: Exchange,
        quote: String,
        period: Period,
        coins: Vec<String>,
    },
    Forecast {
        key: String,
        latitude: f64,
        longitude: f64,
    },
}
pub enum Result {
    Quotes(Exchange, String, Period, Option<Vec<Quote>>),
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
                    Job::Quotes {
                        exchange,
                        quote,
                        period,
                        coins,
                    } => {
                        let mut list = Vec::new();
                        for symbol in &coins {
                            if let Some(q) = fetch_quote(&agent, exchange, symbol, &quote, period) {
                                list.push(q);
                            }
                        }
                        Result::Quotes(exchange, quote, period, (!list.is_empty()).then_some(list))
                    }
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
                Result::Quotes(exchange, quote, period, list) => {
                    data.pending_quotes = false;
                    match list {
                        Some(list) => {
                            data.quotes = list;
                            data.quotes_for = Some((exchange, quote, period));
                            data.quotes_failed = false;
                        }
                        None => data.quotes_failed = true,
                    }
                    data.quotes_at = Some(Instant::now());
                }
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
    let crypto = prefs.uses(Kind::Crypto) || detail_open;
    let wanted = (prefs.exchange, prefs.quote.clone(), prefs.period);
    let stale_quotes = data.quotes_for.as_ref() != Some(&wanted)
        || data
            .quotes_at
            .is_none_or(|at| at.elapsed() >= Duration::from_secs(prefs.refresh_seconds as u64))
        || prefs
            .ordered_coins()
            .iter()
            .any(|c| data.quote(c).is_none() && !data.quotes_failed);
    if crypto && stale_quotes && !data.pending_quotes {
        data.pending_quotes = true;
        let _ = service().jobs.send(Job::Quotes {
            exchange: prefs.exchange,
            quote: prefs.quote.clone(),
            period: prefs.period,
            coins: prefs.ordered_coins(),
        });
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

/// Candle closes for a period, oldest first.
fn fetch_series(
    agent: &ureq::Agent,
    exchange: Exchange,
    symbol: &str,
    quote: &str,
    period: Period,
) -> Option<Vec<f64>> {
    let usd = if quote == "USDT" { "USD" } else { quote };
    let tether = if quote == "USD" { "USDT" } else { quote };
    match exchange {
        Exchange::Binance => {
            let (interval, limit) = match period {
                Period::Hour => ("1m", 60),
                Period::Day => ("15m", 96),
                Period::Week => ("2h", 84),
                Period::Year => ("1w", 52),
            };
            let v = get_json(
                agent,
                &format!(
                    "https://api.binance.com/api/v3/klines?symbol={symbol}{tether}&interval={interval}&limit={limit}"
                ),
            )?;
            Some(
                v.as_array()?
                    .iter()
                    .filter_map(|k| num(k.get(4)?))
                    .collect(),
            )
        }
        Exchange::Coinbase => {
            let (granularity, take) = match period {
                Period::Hour => (60, 60),
                Period::Day => (900, 96),
                Period::Week => (21600, 28),
                Period::Year => (86400, 300),
            };
            let v = get_json(
                agent,
                &format!(
                    "https://api.exchange.coinbase.com/products/{symbol}-{usd}/candles?granularity={granularity}"
                ),
            )?;
            let mut s: Vec<f64> = v
                .as_array()?
                .iter()
                .take(take)
                .filter_map(|k| num(k.get(4)?))
                .collect();
            s.reverse();
            Some(s)
        }
        Exchange::Kraken => {
            let pair = format!("{}{usd}", if symbol == "BTC" { "XBT" } else { symbol });
            let (interval, take) = match period {
                Period::Hour => (1, 60),
                Period::Day => (15, 96),
                Period::Week => (60, 168),
                Period::Year => (10080, 52),
            };
            let v = get_json(
                agent,
                &format!("https://api.kraken.com/0/public/OHLC?pair={pair}&interval={interval}"),
            )?;
            let result = v.get("result")?.as_object()?;
            let rows = result
                .iter()
                .find(|(k, _)| k.as_str() != "last")?
                .1
                .as_array()?;
            let s: Vec<f64> = rows.iter().filter_map(|k| num(k.get(4)?)).collect();
            Some(s[s.len().saturating_sub(take)..].to_vec())
        }
        Exchange::Bybit => {
            let (interval, limit) = match period {
                Period::Hour => ("1", 60),
                Period::Day => ("15", 96),
                Period::Week => ("120", 84),
                Period::Year => ("W", 52),
            };
            let v = get_json(
                agent,
                &format!(
                    "https://api.bybit.com/v5/market/kline?category=spot&symbol={symbol}{tether}&interval={interval}&limit={limit}"
                ),
            )?;
            let mut s: Vec<f64> = v
                .get("result")?
                .get("list")?
                .as_array()?
                .iter()
                .filter_map(|k| num(k.get(4)?))
                .collect();
            s.reverse();
            Some(s)
        }
        Exchange::Okx => {
            let (bar, limit) = match period {
                Period::Hour => ("1m", 60),
                Period::Day => ("15m", 96),
                Period::Week => ("2H", 84),
                Period::Year => ("1W", 52),
            };
            let v = get_json(
                agent,
                &format!(
                    "https://www.okx.com/api/v5/market/candles?instId={symbol}-{tether}&bar={bar}&limit={limit}"
                ),
            )?;
            let mut s: Vec<f64> = v
                .get("data")?
                .as_array()?
                .iter()
                .filter_map(|k| num(k.get(4)?))
                .collect();
            s.reverse();
            Some(s)
        }
        Exchange::Bitfinex => {
            let (frame, limit) = match period {
                Period::Hour => ("1m", 60),
                Period::Day => ("15m", 96),
                Period::Week => ("3h", 56),
                Period::Year => ("1W", 52),
            };
            let v = get_json(
                agent,
                &format!(
                    "https://api-pub.bitfinex.com/v2/candles/trade:{frame}:t{symbol}{usd}/hist?limit={limit}"
                ),
            )?;
            // Bitfinex rows are [time, open, close, high, low, volume].
            let mut s: Vec<f64> = v
                .as_array()?
                .iter()
                .filter_map(|k| num(k.get(2)?))
                .collect();
            s.reverse();
            Some(s)
        }
        Exchange::Coingecko => {
            let id = COINS.iter().find(|c| c.0 == symbol)?.4;
            let days = match period {
                Period::Hour | Period::Day => "1",
                Period::Week => "7",
                Period::Year => "365",
            };
            let v = get_json(
                agent,
                &format!(
                    "https://api.coingecko.com/api/v3/coins/{id}/market_chart?vs_currency={}&days={days}",
                    usd.to_lowercase()
                ),
            )?;
            let s: Vec<f64> = v
                .get("prices")?
                .as_array()?
                .iter()
                .filter_map(|p| num(p.get(1)?))
                .collect();
            Some(if period == Period::Hour {
                s[s.len().saturating_sub(13)..].to_vec()
            } else {
                s
            })
        }
    }
}
fn fetch_quote(
    agent: &ureq::Agent,
    exchange: Exchange,
    symbol: &str,
    quote: &str,
    period: Period,
) -> Option<Quote> {
    let series = fetch_series(agent, exchange, symbol, quote, period)?;
    quote_from_series(symbol, series)
}
pub fn quote_from_series(symbol: &str, series: Vec<f64>) -> Option<Quote> {
    let first = *series.first()?;
    let price = *series.last()?;
    if series.len() < 2 || first <= 0. || price <= 0. {
        return None;
    }
    let high = series.iter().copied().fold(f64::MIN, f64::max);
    let low = series.iter().copied().fold(f64::MAX, f64::min);
    Some(Quote {
        symbol: symbol.into(),
        price,
        change_pct: (price - first) / first * 100.,
        series,
        high,
        low,
    })
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

/// Grouped price: "$67,412", "$3,512", "$171.30", "$0.1523".
pub fn format_price(value: f64, quote: &str) -> String {
    let sign = match quote {
        "EUR" => "€",
        "USDT" => "₮",
        _ => "$",
    };
    let decimals = if value >= 1000. {
        0
    } else if value >= 1. {
        2
    } else {
        4
    };
    let raw = format!("{:.*}", decimals, value.abs());
    let (int, frac) = raw.split_once('.').unwrap_or((&raw, ""));
    let mut grouped = String::new();
    for (i, ch) in int.chars().enumerate() {
        if i > 0 && (int.len() - i) % 3 == 0 {
            grouped.push(',');
        }
        grouped.push(ch);
    }
    if frac.is_empty() {
        format!("{sign}{grouped}")
    } else {
        format!("{sign}{grouped}.{frac}")
    }
}
pub fn format_change(pct: f64) -> String {
    format!("{} {:.2}%", if pct >= 0. { "▲" } else { "▼" }, pct.abs())
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
        // M weather then S crypto: columns 4–5 and 6.
        assert!((l.blocks[0].1 - 566.4).abs() < 0.01);
        assert!((l.blocks[1].1 - TILE).abs() < 0.01);
        assert!((l.blocks[1].0 + l.blocks[1].1 - RIGHT).abs() < 0.01);
        assert!((l.left - (84. + 3. * PITCH)).abs() < 0.01);
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
                Kind::Crypto,
                Kind::Smart,
            ],
            ..Default::default()
        };
        p.normalize();
        assert_eq!(p.blocks.len(), 1);
        assert_eq!(p.blocks[0].size, Size::M);
        assert!(p.columns_used() <= ZONE_COLUMNS);
        assert_eq!(p.text, vec![Kind::Weather, Kind::Crypto, Kind::Smart]);
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
        let p: WidgetPrefs =
            serde_json::from_str(r#"{"blocks":[{"kind":"crypto","size":"l"}],"period":"7d"}"#)
                .unwrap();
        assert_eq!(p.blocks[0].size, Size::L);
        assert_eq!(p.period, Period::Week);
    }
    #[test]
    fn prices_and_changes_read_like_the_mockup() {
        assert_eq!(format_price(67412.4, "USD"), "$67,412");
        assert_eq!(format_price(171.3, "USD"), "$171.30");
        assert_eq!(format_price(0.15234, "EUR"), "€0.1523");
        assert_eq!(format_change(2.144), "▲ 2.14%");
        assert_eq!(format_change(-0.82), "▼ 0.82%");
        let q = quote_from_series("BTC", vec![100., 110., 105.]).unwrap();
        assert!((q.change_pct - 5.).abs() < 1e-9);
        assert_eq!((q.high, q.low), (110., 100.));
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
