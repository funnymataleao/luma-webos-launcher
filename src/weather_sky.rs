//! NOPE WeatherSkyLayer on GPU. Blurs/shadows/glows are baked PNGs, not frame-time filters.
//! At most 30 updates/s while Home is visible; Reduce Motion holds all layers still.
use crate::{
    renderer::{Color, Draw},
    state::Rect,
    weather::Reading,
};
use std::{f32::consts::PI, path::Path};

pub const HOME_WIDGET_OPACITY: f32 = 0.90;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mood {
    Clear,
    Partly,
    Cloudy,
    Fog,
    Wind,
    Rain,
    Storm,
    Snow,
    Night,
    NightCloudy,
}
impl Mood {
    pub const ALL: [Self; 10] = [
        Self::Clear,
        Self::Partly,
        Self::Cloudy,
        Self::Fog,
        Self::Wind,
        Self::Rain,
        Self::Storm,
        Self::Snow,
        Self::Night,
        Self::NightCloudy,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Self::Clear => "clear",
            Self::Partly => "partly",
            Self::Cloudy => "cloudy",
            Self::Fog => "fog",
            Self::Wind => "wind",
            Self::Rain => "rain",
            Self::Storm => "storm",
            Self::Snow => "snow",
            Self::Night => "night",
            Self::NightCloudy => "nightCloudy",
        }
    }
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|m| m.name() == name)
    }
    /// WMO weather codes, actual sun flag and wind velocity from Open-Meteo.
    pub fn from_reading(r: Reading) -> Self {
        match r.code {
            95 | 96 | 99 => Self::Storm,
            71 | 73 | 75 | 77 | 85 | 86 => Self::Snow,
            51 | 53 | 55 | 56 | 57 | 61 | 63 | 65 | 66 | 67 | 80 | 81 | 82 => Self::Rain,
            45 | 48 => Self::Fog,
            _ if r.wind_speed >= 35. => Self::Wind,
            0 if r.is_day => Self::Clear,
            1 | 2 if r.is_day => Self::Partly,
            0 if !r.is_day => Self::Night,
            _ if !r.is_day => Self::NightCloudy,
            _ => Self::Cloudy,
        }
    }
    pub fn colors(self) -> (Color, Color) {
        // Swift WeatherMood.colors, exact normalized linear channel constants.
        let (top, bottom) = match self {
            Self::Clear => ([0.12, 0.40, 0.84], [0.30, 0.58, 0.92]),
            Self::Partly => ([0.14, 0.38, 0.78], [0.34, 0.56, 0.86]),
            Self::Cloudy => ([0.28, 0.36, 0.48], [0.45, 0.53, 0.64]),
            Self::Fog => ([0.32, 0.36, 0.42], [0.52, 0.56, 0.62]),
            Self::Wind => ([0.08, 0.34, 0.44], [0.22, 0.54, 0.62]),
            Self::Rain => ([0.17, 0.23, 0.33], [0.30, 0.37, 0.48]),
            Self::Storm => ([0.11, 0.09, 0.22], [0.27, 0.22, 0.42]),
            Self::Snow => ([0.36, 0.48, 0.64], [0.58, 0.68, 0.80]),
            Self::Night => ([0.05, 0.08, 0.24], [0.18, 0.20, 0.45]),
            Self::NightCloudy => ([0.06, 0.08, 0.18], [0.20, 0.23, 0.36]),
        };
        (
            [top[0], top[1], top[2], HOME_WIDGET_OPACITY],
            [bottom[0], bottom[1], bottom[2], HOME_WIDGET_OPACITY],
        )
    }
    fn cloud_shade(self) -> (f32, f32) {
        match self {
            Self::Clear | Self::Partly => (0.18, 0.62),
            Self::Cloudy | Self::Wind => (0.14, 0.56),
            Self::Fog => (0.16, 0.5),
            Self::Snow => (0.12, 0.5),
            Self::Rain => (0.08, 0.38),
            _ => (0.06, 0.3),
        }
    }
}
fn sprite(draw: &mut Vec<Draw>, root: &Path, id: &str, rect: Rect, alpha: f32, tint: Color) {
    if rect.w <= 0. || rect.h <= 0. {
        return;
    }
    draw.push(Draw::TintedImage(
        root.join("weather-sky")
            .join(format!("{id}.png"))
            .to_string_lossy()
            .into_owned(),
        rect,
        0.,
        false,
        [tint[0], tint[1], tint[2], (alpha * tint[3]).clamp(0., 1.)],
    ));
}
fn white() -> Color {
    [1., 1., 1., 1.]
}
fn scatter(index: usize, salt: u32) -> f32 {
    let mut h = (index as u32).wrapping_mul(0x9E37_79B9) ^ salt.wrapping_mul(0x85EB_CA6B);
    h ^= h >> 16;
    h = h.wrapping_mul(0x7FEB_352D);
    h ^= h >> 15;
    h = h.wrapping_mul(0x846C_A68B);
    h ^= h >> 16;
    (h >> 8) as f32 / (1u32 << 24) as f32
}
fn frac(value: f32, travel: f32) -> f32 {
    value.rem_euclid(travel)
}

pub fn draw(
    draw: &mut Vec<Draw>,
    root: &Path,
    rect: Rect,
    radius: f32,
    mood: Mood,
    seconds: f32,
    reduced_motion: bool,
) {
    let t = if reduced_motion { 0. } else { seconds };
    let (top, bottom) = mood.colors();
    draw.push(Draw::RoundedGradient(rect, radius, top, bottom));
    draw.push(Draw::Clip(Some(rect.inset(1.))));
    draw.push(Draw::RoundClip(Some((
        rect.inset(1.),
        (radius - 1.).max(0.),
    ))));
    let s = rect.w / 360.;
    // Particles keep a TV size on big plates: more of them, not bigger ones.
    let p = s.min(1.4);
    let more = ((rect.w * rect.h) / (360. * 322.)).sqrt().clamp(1., 3.);
    let clouds = match mood {
        Mood::Partly => (3, 1.),
        Mood::Cloudy | Mood::Rain => (4, 1.),
        Mood::Fog => (3, 0.5),
        Mood::Wind => (3, 3.),
        Mood::Storm => (4, 1.6),
        Mood::Snow => (3, 0.7),
        Mood::NightCloudy => (3, 0.8),
        _ => (0, 1.),
    };
    if matches!(mood, Mood::Night | Mood::NightCloudy) {
        let count = (if mood == Mood::Night { 26. } else { 16. } * more) as usize;
        for i in 0..count {
            // Hash-scattered so the stars never line up in rows or diagonals.
            let x = rect.x + scatter(i, 1) * rect.w;
            let y = rect.y + scatter(i, 2) * 0.62 * rect.h;
            let alpha = 0.35 + 0.65 * (0.5 + 0.5 * (t * 1.6 + i as f32).sin());
            let r = if i % 4 == 0 { 5.5 } else { 3.8 } * p;
            sprite(
                draw,
                root,
                "dot",
                Rect::new(x - r, y - r, r * 2., r * 2.),
                alpha,
                white(),
            );
        }
        if mood == Mood::Night {
            let phase = frac(t, 7.);
            if phase < 0.9 {
                let progress = phase / 0.9;
                let x = rect.x + rect.w * (0.15 + 0.5 * progress);
                let y = rect.y + rect.h * (0.08 + 0.22 * progress);
                sprite(
                    draw,
                    root,
                    "wind",
                    Rect::new(x - 56. * s, y - 15. * s, 65. * s, 15. * s),
                    (progress * PI).sin(),
                    white(),
                );
            }
        }
        // No sky crescent: the card's condition icon already shows the moon,
        // and a second one behind it read as an overlap.
    }
    for i in 0..clouds.0 {
        let depth = if clouds.0 == 1 {
            1.
        } else {
            i as f32 / (clouds.0 - 1) as f32
        };
        // Thin, translucent and scattered: clouds are atmosphere behind the
        // text, never a solid shape in front of the sun.
        let w = rect.w * (0.2 + depth * 0.26);
        let h = w / 2.;
        let velocity = (4. + depth * 11.) * clouds.1 * s;
        let x = rect.x + frac(t * velocity + scatter(i, 8) * (rect.w + w), rect.w + w) - w;
        let bob = (t * 0.5 + i as f32 * 1.7).sin() * 2. * s;
        let y = rect.y + rect.h * (0.02 + scatter(i, 7) * 0.34) + bob;
        let (far, near) = mood.cloud_shade();
        let light = far + (near - far) * depth;
        let alpha = 0.16 + depth * 0.26;
        // Far clouds stay close to the sky colour, near ones are much lighter.
        let tint = [
            bottom[0] + (1. - bottom[0]) * light,
            bottom[1] + (1. - bottom[1]) * light,
            bottom[2] + (1. - bottom[2]) * light,
            1.,
        ];
        let shape = ["cloud", "cloud2", "cloud3", "cloud4"][i % 4];
        // Sprites carry a 20-unit margin around the 240x120 body.
        let m = w * 20. / 240.;
        sprite(
            draw,
            root,
            shape,
            Rect::new(x - m, y - m, w + 2. * m, h + 2. * m),
            alpha,
            tint,
        );
    }
    match mood {
        Mood::Clear | Mood::Partly => {
            for i in 0..(14. * more) as usize {
                let travel = rect.h + 20. * s;
                let rise = frac(
                    t * (8. + scatter(i, 33) * 8.) + scatter(i, 32) * travel,
                    travel,
                );
                let x = rect.x + scatter(i, 31) * rect.w + (t * 0.6 + i as f32).sin() * 8. * s;
                let y = rect.y + rect.h + 10. * s - rise;
                let alpha = 0.12 + 0.22 * (0.5 + 0.5 * (t * 1.3 + i as f32 * 2.).sin());
                let r = if i % 3 == 0 { 5.2 } else { 3.4 } * p;
                sprite(
                    draw,
                    root,
                    "dot",
                    Rect::new(x - r, y - r, 2. * r, 2. * r),
                    alpha,
                    white(),
                );
            }
        }
        Mood::Fog => {
            for i in 0..4 {
                let w = rect.w * 1.5;
                let x = rect.x
                    + (rect.w - w) / 2.
                    + (t * (0.12 + i as f32 * 0.04) + i as f32 * 1.9).sin() * rect.w * 0.18;
                let y = rect.y + rect.h * (0.26 + i as f32 * 0.19);
                sprite(
                    draw,
                    root,
                    "fog",
                    Rect::new(x, y, w, rect.h * 0.16),
                    0.26,
                    white(),
                );
            }
        }
        Mood::Wind => {
            for i in 0..6 {
                let len = rect.w * (0.22 + (i % 3) as f32 * 0.06);
                let x = rect.x
                    + frac(
                        t * (150. + (i % 3) as f32 * 50.) * s + i as f32 * 97. * s,
                        rect.w + len * 2.,
                    )
                    - len;
                let y = rect.y
                    + rect.h * (0.3 + ((i * 41) % 60) as f32 / 100.)
                    + (t * 2. + i as f32).sin() * 6. * s;
                sprite(
                    draw,
                    root,
                    "wind",
                    Rect::new(x, y, len, 24. * s),
                    0.75,
                    white(),
                );
            }
        }
        Mood::Rain | Mood::Storm => {
            let drops = (if mood == Mood::Storm { 60. } else { 40. } * more) as usize;
            let speed = if mood == Mood::Storm { 340. } else { 260. };
            for i in 0..drops {
                let travel = rect.h + 30. * s;
                let x = rect.x + scatter(i, 11) * rect.w;
                let y = rect.y
                    + frac(
                        t * speed * s * (0.85 + scatter(i, 13) * 0.3) + scatter(i, 12) * travel,
                        travel,
                    )
                    - 14. * s;
                sprite(
                    draw,
                    root,
                    "rain",
                    Rect::new(x - 2. * p, y, 8. * p, 22. * p),
                    0.50,
                    white(),
                );
            }
            if mood == Mood::Storm {
                let phase = frac(t, 5.5);
                let flash = if phase < 0.1 {
                    1.
                } else if phase > 0.2 && phase < 0.28 {
                    0.7
                } else {
                    0.
                };
                if flash > 0. {
                    draw.push(Draw::Shape(rect, radius, 0., [1., 1., 1., 0.22 * flash]));
                    let cycle = (t / 5.5) as usize;
                    let x = rect.x + rect.w * (0.55 + (cycle % 4) as f32 * 0.08);
                    sprite(
                        draw,
                        root,
                        "bolt",
                        Rect::new(x, rect.y + rect.h * 0.2, 55. * s, rect.h * 0.5),
                        flash,
                        white(),
                    );
                }
            }
        }
        Mood::Snow => {
            for i in 0..(34. * more) as usize {
                let travel = rect.h + 10. * s;
                let fall = frac(
                    t * (26. + scatter(i, 23) * 16.) * s + scatter(i, 22) * travel,
                    travel,
                );
                let x = rect.x + scatter(i, 21) * rect.w + (t + i as f32).sin() * 6. * s;
                let r = if i % 3 == 0 { 5.5 } else { 4. } * p;
                sprite(
                    draw,
                    root,
                    "snow",
                    Rect::new(x - r, rect.y + fall - r, 2. * r, 2. * r),
                    if i % 3 == 0 { 0.75 } else { 0.5 },
                    white(),
                );
            }
        }
        _ => {}
    }
    draw.push(Draw::RoundClip(None));
    draw.push(Draw::Clip(None));
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn all_ten_moods_are_distinct_and_match_wmo_and_sun() {
        let make = |code, is_day, wind_speed| Reading {
            temperature: 16.,
            code,
            is_day,
            wind_speed,
            temperature_3h_ago: None,
            hours: [None; 5],
        };
        let cases = [
            (0, true, 0., Mood::Clear),
            (2, true, 0., Mood::Partly),
            (3, true, 0., Mood::Cloudy),
            (45, true, 0., Mood::Fog),
            (0, true, 40., Mood::Wind),
            (61, true, 0., Mood::Rain),
            (95, true, 0., Mood::Storm),
            (71, true, 0., Mood::Snow),
            (0, false, 0., Mood::Night),
            (2, false, 0., Mood::NightCloudy),
        ];
        for (code, day, wind, want) in cases {
            assert_eq!(Mood::from_reading(make(code, day, wind)), want);
        }
        for mood in Mood::ALL {
            assert_eq!(Mood::from_name(mood.name()), Some(mood));
        }
    }
    #[test]
    fn card_covers_exactly_two_grid_columns_and_ends_at_right_edge() {
        use crate::state::{HOME_STRIP_LEFT, HOME_STRIP_RIGHT, HOME_TILE_PITCH, HOME_TILE_WIDTH};
        let two_columns = HOME_TILE_WIDTH + HOME_TILE_PITCH;
        assert!((two_columns - 566.4).abs() < 0.001);
        assert!(
            (HOME_STRIP_LEFT + 4. * HOME_TILE_PITCH + two_columns - HOME_STRIP_RIGHT).abs() < 0.001
        );
    }
    #[test]
    fn backgrounds_are_exact_nope_rgb_at_ninety_percent() {
        assert_eq!(Mood::Clear.colors().0, [0.12, 0.40, 0.84, 0.90]);
        assert_eq!(Mood::Night.colors().1, [0.18, 0.20, 0.45, 0.90]);
    }
}
