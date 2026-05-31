// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright © 2026 Gyroflow

//! Premiere-Pro-style color grading parameters.
//!
//! All values are stored NORMALIZED (shader-ready) so the GPU/CPU paths can
//! consume them directly without re-scaling:
//! - temperature / tint / exposure / contrast / highlights / shadows / whites
//!   / blacks / vibrance: `-1.0..1.0` (0.0 = neutral)
//! - basic_saturation / creative_saturation: `0.0..2.0` (1.0 = neutral)
//! - faded_film: `0.0..1.0` (0.0 = off)
//!
//! The UI sliders use human ranges (e.g. -100..100, 0..200) and the controller
//! divides by 100 before calling the setters, so the core only ever sees
//! normalized values.

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct ColorGradingParams {
    pub basic_enabled: bool,
    pub creative_enabled: bool,

    // Basic - color
    pub temperature: f32,
    pub tint: f32,
    pub basic_saturation: f32,

    // Basic - light
    pub exposure: f32,
    pub contrast: f32,
    pub highlights: f32,
    pub shadows: f32,
    pub whites: f32,
    pub blacks: f32,

    // Creative
    pub faded_film: f32,
    pub vibrance: f32,
    pub creative_saturation: f32,

    // LUT (.cube). Path is persisted; strength 0..1; enabled toggles it.
    pub lut_enabled: bool,
    pub lut_strength: f32,
    pub lut_path: String,

    // Parsed LUT data, kept out of serde (re-loaded from lut_path on import).
    #[serde(skip)]
    pub lut: Option<std::sync::Arc<crate::lut::Lut>>,
}

impl Default for ColorGradingParams {
    fn default() -> Self {
        Self {
            basic_enabled: false,
            creative_enabled: false,
            temperature: 0.0,
            tint: 0.0,
            basic_saturation: 1.0,
            exposure: 0.0,
            contrast: 0.0,
            highlights: 0.0,
            shadows: 0.0,
            whites: 0.0,
            blacks: 0.0,
            faded_film: 0.0,
            vibrance: 0.0,
            creative_saturation: 1.0,
            lut_enabled: false,
            lut_strength: 1.0,
            lut_path: String::new(),
            lut: None,
        }
    }
}

impl ColorGradingParams {
    /// True when no enabled section would alter the image. Used to skip the
    /// color pass entirely (identity).
    pub fn is_identity(&self) -> bool {
        !self.basic_enabled && !self.creative_enabled && !(self.lut_enabled && self.lut.is_some())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_identity() {
        let p = ColorGradingParams::default();
        assert!(!p.basic_enabled);
        assert!(!p.creative_enabled);
        assert!(p.is_identity());
        assert_eq!(p.temperature, 0.0);
        assert_eq!(p.tint, 0.0);
        assert_eq!(p.basic_saturation, 1.0);
        assert_eq!(p.exposure, 0.0);
        assert_eq!(p.contrast, 0.0);
        assert_eq!(p.creative_saturation, 1.0);
        assert_eq!(p.faded_film, 0.0);
    }

    #[test]
    fn serde_roundtrip() {
        let mut p = ColorGradingParams::default();
        p.basic_enabled = true;
        p.exposure = 0.5;
        p.basic_saturation = 1.25;
        let s = serde_json::to_string(&p).unwrap();
        let p2: ColorGradingParams = serde_json::from_str(&s).unwrap();
        assert_eq!(p2, p);
        assert!(p2.basic_enabled);
        assert_eq!(p2.exposure, 0.5);
        assert_eq!(p2.basic_saturation, 1.25);
    }

    #[test]
    fn serde_partial_is_backward_compatible() {
        // Old projects without a color_grading object, or with a partial one,
        // must deserialize to defaults for missing fields (#[serde(default)]).
        let p: ColorGradingParams = serde_json::from_str("{}").unwrap();
        assert_eq!(p, ColorGradingParams::default());

        let p2: ColorGradingParams = serde_json::from_str(r#"{"exposure":0.3}"#).unwrap();
        assert_eq!(p2.exposure, 0.3);
        assert_eq!(p2.basic_saturation, 1.0); // default preserved
    }
}
