//! Semantic typography for the 1920x1080 television canvas.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextRole {
    Hero,
    Title,
    Headline,
    Body,
    Callout,
    Caption,
}
#[derive(Debug, Clone, Copy)]
pub struct TypeToken {
    pub size: f32,
    pub weight: u16,
    pub line_height: f32,
    pub tracking: f32,
}
impl TextRole {
    pub const fn token(self) -> TypeToken {
        match self {
            Self::Hero => TypeToken {
                size: 108.,
                weight: 800,
                line_height: 1.05,
                tracking: -0.025,
            },
            Self::Title => TypeToken {
                size: 52.,
                weight: 700,
                line_height: 1.18,
                tracking: -0.02,
            },
            Self::Headline => TypeToken {
                size: 38.,
                weight: 700,
                line_height: 1.22,
                tracking: -0.015,
            },
            Self::Body => TypeToken {
                size: 31.,
                weight: 500,
                line_height: 1.45,
                tracking: 0.,
            },
            Self::Callout => TypeToken {
                size: 29.,
                weight: 500,
                line_height: 1.4,
                tracking: 0.,
            },
            Self::Caption => TypeToken {
                size: 25.,
                weight: 600,
                line_height: 1.4,
                tracking: 0.005,
            },
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn readable_tv_role_scale() {
        for role in [
            TextRole::Hero,
            TextRole::Title,
            TextRole::Headline,
            TextRole::Body,
            TextRole::Callout,
            TextRole::Caption,
        ] {
            let t = role.token();
            assert!(t.size >= 25. && t.weight >= 500);
        }
    }
}

/// An independently selectable licensed typeface.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize, Default,
)]
#[serde(rename_all = "lowercase")]
pub enum HeadingFont {
    #[default]
    Archivo,
    Inter,
    Unbounded,
    Montserrat,
    Manrope,
    Onest,
    Rubik,
    Exo2,
    Raleway,
    Oswald,
    Russoone,
    Jost,
    Playfairdisplay,
    Prata,
    Fraunces,
    Spacegrotesk,
    Syne,
    Bebasneue,
    Nunito,
}
impl HeadingFont {
    pub const ALL: [Self; 19] = [
        Self::Archivo,
        Self::Inter,
        Self::Unbounded,
        Self::Montserrat,
        Self::Manrope,
        Self::Onest,
        Self::Rubik,
        Self::Exo2,
        Self::Raleway,
        Self::Oswald,
        Self::Russoone,
        Self::Jost,
        Self::Playfairdisplay,
        Self::Prata,
        Self::Fraunces,
        Self::Spacegrotesk,
        Self::Syne,
        Self::Bebasneue,
        Self::Nunito,
    ];
    pub const fn id(self) -> &'static str {
        match self {
            Self::Archivo => "archivo",
            Self::Inter => "inter",
            Self::Unbounded => "unbounded",
            Self::Montserrat => "montserrat",
            Self::Manrope => "manrope",
            Self::Onest => "onest",
            Self::Rubik => "rubik",
            Self::Exo2 => "exo2",
            Self::Raleway => "raleway",
            Self::Oswald => "oswald",
            Self::Russoone => "russoone",
            Self::Jost => "jost",
            Self::Playfairdisplay => "playfairdisplay",
            Self::Prata => "prata",
            Self::Fraunces => "fraunces",
            Self::Spacegrotesk => "spacegrotesk",
            Self::Syne => "syne",
            Self::Bebasneue => "bebasneue",
            Self::Nunito => "nunito",
        }
    }
    pub const fn name(self) -> &'static str {
        match self {
            Self::Archivo => "Archivo Expanded",
            Self::Inter => "Inter",
            Self::Unbounded => "Unbounded",
            Self::Montserrat => "Montserrat",
            Self::Manrope => "Manrope",
            Self::Onest => "Onest",
            Self::Rubik => "Rubik",
            Self::Exo2 => "Exo 2",
            Self::Raleway => "Raleway",
            Self::Oswald => "Oswald",
            Self::Russoone => "Russo One",
            Self::Jost => "Jost",
            Self::Playfairdisplay => "Playfair Display",
            Self::Prata => "Prata",
            Self::Fraunces => "Fraunces",
            Self::Spacegrotesk => "Space Grotesk",
            Self::Syne => "Syne",
            Self::Bebasneue => "Bebas Neue",
            Self::Nunito => "Nunito",
        }
    }
    pub const fn scale(self) -> f32 {
        match self {
            Self::Archivo => 1.0606,
            Self::Inter => 1.0000,
            Self::Unbounded => 0.9701,
            Self::Montserrat => 1.0393,
            Self::Manrope => 1.0105,
            Self::Onest => 1.0291,
            Self::Rubik => 1.0393,
            Self::Exo2 => 1.0544,
            Self::Raleway => 1.0247,
            Self::Oswald => 0.8982,
            Self::Russoone => 1.0393,
            Self::Jost => 1.0393,
            Self::Playfairdisplay => 1.0276,
            Self::Prata => 0.9094,
            Self::Fraunces => 1.0393,
            Self::Spacegrotesk => 1.0393,
            Self::Syne => 1.1193,
            Self::Bebasneue => 1.0393,
            Self::Nunito => 1.0320,
        }
    }
}
/// An independently selectable licensed typeface.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize, Default,
)]
#[serde(rename_all = "lowercase")]
pub enum TextFont {
    #[default]
    Inter,
    Montserrat,
    Manrope,
    Onest,
    Rubik,
    Jost,
    Golostext,
    Ibmplexsans,
    Notosans,
    Sourcesans3,
    Roboto,
    Ptsans,
    Opensans,
    Nunitosans,
    Nunito,
}
impl TextFont {
    pub const ALL: [Self; 15] = [
        Self::Inter,
        Self::Manrope,
        Self::Onest,
        Self::Golostext,
        Self::Rubik,
        Self::Montserrat,
        Self::Ibmplexsans,
        Self::Notosans,
        Self::Sourcesans3,
        Self::Roboto,
        Self::Ptsans,
        Self::Opensans,
        Self::Nunitosans,
        Self::Jost,
        Self::Nunito,
    ];
    pub const fn id(self) -> &'static str {
        match self {
            Self::Inter => "inter",
            Self::Montserrat => "montserrat",
            Self::Manrope => "manrope",
            Self::Onest => "onest",
            Self::Rubik => "rubik",
            Self::Jost => "jost",
            Self::Golostext => "golostext",
            Self::Ibmplexsans => "ibmplexsans",
            Self::Notosans => "notosans",
            Self::Sourcesans3 => "sourcesans3",
            Self::Roboto => "roboto",
            Self::Ptsans => "ptsans",
            Self::Opensans => "opensans",
            Self::Nunitosans => "nunitosans",
            Self::Nunito => "nunito",
        }
    }
    pub const fn name(self) -> &'static str {
        match self {
            Self::Inter => "Inter",
            Self::Montserrat => "Montserrat",
            Self::Manrope => "Manrope",
            Self::Onest => "Onest",
            Self::Rubik => "Rubik",
            Self::Jost => "Jost",
            Self::Golostext => "Golos Text",
            Self::Ibmplexsans => "IBM Plex Sans",
            Self::Notosans => "Noto Sans",
            Self::Sourcesans3 => "Source Sans 3",
            Self::Roboto => "Roboto",
            Self::Ptsans => "PT Sans",
            Self::Opensans => "Open Sans",
            Self::Nunitosans => "Nunito Sans",
            Self::Nunito => "Nunito",
        }
    }
    pub const fn scale(self) -> f32 {
        match self {
            Self::Inter => 1.0000,
            Self::Montserrat => 1.0559,
            Self::Manrope => 1.0109,
            Self::Onest => 1.0359,
            Self::Rubik => 1.0498,
            Self::Jost => 1.1400,
            Self::Golostext => 1.0300,
            Self::Ibmplexsans => 1.0579,
            Self::Notosans => 1.0185,
            Self::Sourcesans3 => 1.1400,
            Self::Roboto => 1.0333,
            Self::Ptsans => 1.0918,
            Self::Opensans => 1.0201,
            Self::Nunitosans => 1.1279,
            Self::Nunito => 1.1279,
        }
    }
}
