//! Context-aware, stable Rude greeting selection. Source copy retains every approved line.
use crate::model::Preferences;
use crate::weather::Reading;
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;
use std::time::Instant;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
pub enum GreetingStyle {
    Classic,
    #[default]
    Rude,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DayRecord {
    pub day: i32,
    pub category: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Category {
    ClearNice,
    Chilly,
    Cold,
    Frost,
    Hot,
    Cloudy,
    Rain,
    Thunder,
    Snow,
    Wind,
    Fog,
    Night,
    Still,
    Change,
}
impl Category {
    fn from_label(s: &str) -> Option<Self> {
        Some(match s {
            "clear_nice" => Self::ClearNice,
            "chilly" => Self::Chilly,
            "cold" => Self::Cold,
            "frost" => Self::Frost,
            "hot" => Self::Hot,
            "cloudy" => Self::Cloudy,
            "rain" => Self::Rain,
            "thunder" => Self::Thunder,
            "snow" => Self::Snow,
            "wind" => Self::Wind,
            "fog" => Self::Fog,
            "night" => Self::Night,
            "still" => Self::Still,
            "change" => Self::Change,
            _ => return None,
        })
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::ClearNice => "sunny",
            Self::Chilly => "chilly",
            Self::Cold => "cold",
            Self::Frost => "frost",
            Self::Hot => "hot",
            Self::Cloudy => "grey",
            Self::Rain => "rain",
            Self::Thunder => "storm",
            Self::Snow => "snow",
            Self::Wind => "windy",
            Self::Fog => "fog",
            Self::Night => "night",
            Self::Still => "still",
            Self::Change => "change",
        }
    }
    pub fn harsh(self) -> bool {
        matches!(
            self,
            Self::Cold | Self::Frost | Self::Rain | Self::Cloudy | Self::Hot
        )
    }
}
#[derive(Clone, Debug)]
pub struct Template {
    pub category: Category,
    pub title: String,
    pub subtitle: String,
}
pub fn templates() -> &'static [Template] {
    static TABLE: OnceLock<Vec<Template>> = OnceLock::new();
    TABLE.get_or_init(|| {
        let mut current = None;
        let mut table = Vec::new();
        for line in include_str!("weather-phrases.md").lines() {
            if let Some(heading) = line.strip_prefix("## ") {
                current = Category::from_label(heading.split_whitespace().next().unwrap_or(""));
            } else if let (Some(category), Some(phrase)) = (current, line.strip_prefix("- "))
                && let Some((title, subtitle)) = phrase.split_once(" — ")
            {
                table.push(Template {
                    category,
                    title: title.into(),
                    subtitle: subtitle.into(),
                });
            }
        }
        table
    })
}

pub fn weather_category(reading: Reading, hour: u8) -> Category {
    // WMO codes. Unknown codes still resolve to temperature/sky, never to a blank title.
    match reading.code {
        95 | 96 | 99 => return Category::Thunder,
        71 | 73 | 75 | 77 | 85 | 86 => return Category::Snow,
        51 | 53 | 55 | 56 | 57 | 61 | 63 | 65 | 66 | 67 | 80 | 81 | 82 => return Category::Rain,
        45 | 48 => return Category::Fog,
        _ => {}
    }
    if reading.wind_speed >= 35.0 {
        return Category::Wind;
    }
    if !reading.is_day || !(5..23).contains(&hour) {
        return Category::Night;
    }
    if reading.temperature < 0.0 {
        return Category::Frost;
    }
    if reading.temperature < 5.0 {
        return Category::Cold;
    }
    if reading.temperature >= 25.0 {
        return Category::Hot;
    }
    if matches!(reading.code, 2 | 3) {
        return Category::Cloudy;
    }
    if reading.temperature < 15.0 {
        Category::Chilly
    } else {
        Category::ClearNice
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Phrase {
    pub category: Category,
    pub title: String,
    pub subtitle: String,
}

#[derive(Default)]
pub struct Engine {
    pub current: Option<Phrase>,
    pub previous: Option<Phrase>,
    pub changed_at: Option<Instant>,
    last_category: Option<Category>,
    last_reading: Option<Reading>,
    last_update: i64,
    last_choice: Option<usize>,
    chosen_category: Option<Category>,
    chosen_bucket: i64,
    change_until: i64,
    old_label: String,
    new_label: String,
    delta: i32,
}
impl Engine {
    pub fn sync(
        &mut self,
        prefs: &mut Preferences,
        reading: Option<(Reading, i64)>,
        now: i64,
        day: i32,
        hour: u8,
    ) -> bool {
        let Some((reading, updated_at)) = reading.filter(|(r, _)| r.valid()) else {
            if self.current.is_some() {
                self.previous = self.current.take();
                self.changed_at = Some(Instant::now());
            }
            return false;
        };
        let base = weather_category(reading, hour);
        let changed_reading = self.last_update != updated_at || self.last_reading != Some(reading);
        let mut save = false;
        if changed_reading {
            if let Some(old) = self.last_category
                && old != base
            {
                self.change_until = now + 3600;
                self.old_label = old.label().into();
                self.new_label = base.label().into();
                self.delta = self
                    .last_reading
                    .map_or(0, |r| (reading.temperature - r.temperature).round() as i32);
            }
            if let Some(three_hours_ago) = reading.temperature_3h_ago
                && (reading.temperature - three_hours_ago).abs() >= 6.0
                && (now - updated_at).abs() < 3600
            {
                self.change_until = now + 3600;
                self.old_label = format!("{three_hours_ago:.0}°");
                self.new_label = format!("{:.0}°", reading.temperature);
                self.delta = (reading.temperature - three_hours_ago).round() as i32;
            }
            self.last_reading = Some(reading);
            self.last_category = Some(base);
            self.last_update = updated_at;
            if !prefs
                .weather_day_history
                .iter()
                .any(|record| record.day == day && record.category == base.label())
            {
                prefs.weather_day_history.retain(|record| record.day != day);
                prefs.weather_day_history.push(DayRecord {
                    day,
                    category: base.label().into(),
                });
                prefs.weather_day_history.sort_by_key(|r| r.day);
                if prefs.weather_day_history.len() > 14 {
                    prefs
                        .weather_day_history
                        .drain(..prefs.weather_day_history.len() - 14);
                }
                save = true;
            }
        }
        let mut run = 0;
        if base.harsh() {
            let mut d = day;
            for entry in prefs.weather_day_history.iter().rev() {
                if entry.day == d && entry.category == base.label() {
                    run += 1;
                    d -= 1;
                } else if entry.day < d {
                    break;
                }
            }
        }
        let category = if now < self.change_until {
            Category::Change
        } else if run >= 2 {
            Category::Still
        } else {
            base
        };
        let bucket = now.div_euclid(3 * 3600);
        if self
            .current
            .as_ref()
            .is_some_and(|p| p.category == category)
            && self.chosen_category == Some(category)
            && self.chosen_bucket == bucket
        {
            return save;
        }
        let choices: Vec<_> = templates()
            .iter()
            .filter(|t| {
                if t.category != category {
                    return false;
                }
                if category == Category::Still {
                    let keyword = match base {
                        Category::Cold | Category::Frost => "cold",
                        Category::Rain => "raining",
                        Category::Cloudy => "grey",
                        Category::Hot => "hot",
                        _ => return false,
                    };
                    return t.title.to_ascii_lowercase().contains(keyword);
                }
                if category == Category::Change && t.title.contains("colder") {
                    return self.delta < 0;
                }
                true
            })
            .collect();
        if choices.is_empty() {
            return save;
        }
        // Pseudorandom across category and elapsed time, but deterministic until the next rotation.
        let seed = (now as u64).rotate_left(17)
            ^ ((reading.temperature.to_bits() >> 16).wrapping_mul(0x9e3779b97f4a7c15))
            ^ (category as u64).wrapping_mul(0xbf58476d1ce4e5b9);
        let mut index = (seed as usize) % choices.len();
        if self.chosen_category == Some(category)
            && self.last_choice == Some(index)
            && choices.len() > 1
        {
            index = (index + 1) % choices.len();
        }
        let selected = choices[index];
        let title = selected
            .title
            .replace("{N}", &run.to_string())
            .replace("{old}", &self.old_label)
            .replace("{new}", &self.new_label)
            .replace("{delta}", &self.delta.abs().to_string());
        let subtitle = selected
            .subtitle
            .replace("{N}", &run.to_string())
            .replace("{old}", &self.old_label)
            .replace("{new}", &self.new_label)
            .replace("{delta}", &self.delta.abs().to_string());
        self.previous = self.current.replace(Phrase {
            category,
            title,
            subtitle,
        });
        self.changed_at = Some(Instant::now());
        self.last_choice = Some(index);
        self.chosen_category = Some(category);
        self.chosen_bucket = bucket;
        save
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn reading(t: f64, code: u8) -> Reading {
        Reading {
            temperature: t,
            code,
            is_day: true,
            wind_speed: 0.,
            temperature_3h_ago: None,
            hours: [None; 5],
        }
    }
    #[test]
    fn all_approved_phrases_are_loaded() {
        assert_eq!(templates().len(), 60);
        for row in templates() {
            assert!(!row.title.is_empty() && !row.subtitle.is_empty());
        }
    }
    #[test]
    fn priority_and_wmo_codes() {
        assert_eq!(weather_category(reading(30., 95), 12), Category::Thunder);
        assert_eq!(weather_category(reading(30., 85), 12), Category::Snow);
        assert_eq!(weather_category(reading(30., 51), 12), Category::Rain);
        assert_eq!(weather_category(reading(20., 45), 12), Category::Fog);
        assert_eq!(
            weather_category(
                Reading {
                    wind_speed: 36.,
                    ..reading(20., 0)
                },
                12
            ),
            Category::Wind
        );
        assert_eq!(weather_category(reading(-1., 0), 12), Category::Frost);
        assert_eq!(weather_category(reading(4., 0), 12), Category::Cold);
        assert_eq!(weather_category(reading(14., 0), 12), Category::Chilly);
        assert_eq!(weather_category(reading(15., 0), 12), Category::ClearNice);
        assert_eq!(weather_category(reading(25., 0), 12), Category::Hot);
        assert_eq!(weather_category(reading(20., 3), 12), Category::Cloudy);
        assert_eq!(weather_category(reading(20., 0), 0), Category::Night);
    }
    #[test]
    fn stability_and_repeats() {
        let mut prefs = Preferences::default();
        let mut engine = Engine::default();
        engine.sync(&mut prefs, Some((reading(12., 0), 100)), 101, 10, 12);
        let first = engine.current.clone().unwrap();
        engine.sync(&mut prefs, Some((reading(12., 0), 100)), 300, 10, 12);
        assert_eq!(engine.current, Some(first));
    }
    #[test]
    fn still_change_and_interpolation() {
        let mut prefs = Preferences::default();
        let mut engine = Engine::default();
        engine.sync(&mut prefs, Some((reading(3., 0), 100)), 100, 10, 12);
        engine.sync(&mut prefs, Some((reading(3., 0), 90000)), 90000, 11, 12);
        assert_eq!(engine.current.as_ref().unwrap().category, Category::Still);
        let changed = Reading {
            temperature_3h_ago: Some(3.),
            ..reading(13., 0)
        };
        engine.sync(&mut prefs, Some((changed, 90010)), 90010, 11, 12);
        assert_eq!(engine.current.as_ref().unwrap().category, Category::Change);
        assert!(!engine.current.as_ref().unwrap().subtitle.contains("{"));
    }
    #[test]
    fn no_weather_is_classic_fallback() {
        let mut prefs = Preferences::default();
        let mut engine = Engine::default();
        engine.sync(&mut prefs, None, 10, 1, 12);
        assert!(engine.current.is_none());
    }
}
