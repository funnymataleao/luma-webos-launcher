//! Logical layout is independent of drawable density. Vector coverage and
//! glyph-origin snapping use the drawable's physical pixel scale in shaders.
use serde::Serialize;
use std::str::FromStr;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct Size {
    pub width: u32,
    pub height: u32,
}
impl Size {
    pub const HD: Self = Self {
        width: 1920,
        height: 1080,
    };
    pub const UHD: Self = Self {
        width: 3840,
        height: 2160,
    };
    pub const fn new(width: u32, height: u32) -> Self {
        Self { width, height }
    }
    pub fn scale(self) -> f32 {
        (self.width as f32 / 1920.).min(self.height as f32 / 1080.)
    }
    pub fn is_4k(self) -> bool {
        self.width >= 3840 && self.height >= 2160
    }
}
impl FromStr for Size {
    type Err = String;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "1080p" => return Ok(Self::HD),
            "4k" => return Ok(Self::UHD),
            _ => {}
        }
        let (w, h) = value
            .split_once('x')
            .ok_or("Resolution must be 1080p, 4k, or WIDTHxHEIGHT")?;
        let width = w.parse::<u32>().map_err(|e| e.to_string())?;
        let height = h.parse::<u32>().map_err(|e| e.to_string())?;
        if !(640..=7680).contains(&width) || !(360..=4320).contains(&height) {
            return Err("Requested window must be between 640x360 and 7680x4320".into());
        }
        Ok(Self { width, height })
    }
}

#[derive(Clone, Copy, Debug, Serialize)]
pub struct Viewport {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub scale: f32,
}
impl Viewport {
    pub fn new(size: Size) -> Self {
        let scale = size.scale();
        let width = (1920. * scale).round() as i32;
        let height = (1080. * scale).round() as i32;
        Self {
            x: (size.width as i32 - width) / 2,
            y: (size.height as i32 - height) / 2,
            width,
            height,
            scale,
        }
    }
    pub fn text_scale(self) -> f32 {
        self.height as f32 / 1080.
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_letterboxed_drawable_uses_its_fitted_density() {
        let viewport = Viewport::new(Size::new(3840, 2400));
        assert_eq!(viewport.scale, 2.);
        assert_eq!(viewport.y, 120);
        assert_eq!(viewport.width, 3840);
        assert_eq!(viewport.height, 2160);
    }
    #[test]
    fn requested_resolution_is_validated_separately_from_the_actual_drawable() {
        assert_eq!("4k".parse::<Size>().unwrap(), Size::UHD);
        assert_eq!("1280x720".parse::<Size>().unwrap(), Size::new(1280, 720));
        for invalid in ["0x0", "100000x2160", "3840", "-1x1080"] {
            assert!(invalid.parse::<Size>().is_err());
        }
        assert!(!Size::HD.is_4k());
        assert!(Size::UHD.is_4k());
    }
}
