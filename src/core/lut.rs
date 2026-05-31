// SPDX-License-Identifier: GPL-3.0-or-later

//! Minimal Adobe `.cube` LUT parser (1D and 3D), producing GPU-ready RGBA f32 data.
//!
//! A 3D LUT of size N is stored as an RGBA f32 table of N*N*N entries, in the
//! canonical .cube ordering: red varies fastest, then green, then blue.
//! For GPU upload it can be laid out as a 2D texture of width=N, height=N*N
//! (N slices of NxN stacked vertically), which matches the existing
//! `texMeshData` (RGBA32F) upload pattern used by the Qt RHI preview path.
//!
//! A 1D LUT of size N is stored as N RGBA f32 entries (one per input level),
//! applied per channel.

#[derive(Clone, Debug, PartialEq)]
pub enum LutKind {
    Dim1,
    Dim3,
}

#[derive(Clone, Debug)]
pub struct Lut {
    pub kind: LutKind,
    pub size: usize,
    pub domain_min: [f32; 3],
    pub domain_max: [f32; 3],
    /// RGBA f32. For 3D: size^3 entries (r fastest, then g, then b).
    /// For 1D: size entries.
    pub data: Vec<f32>,
}

impl Lut {
    /// Parse an Adobe `.cube` file from its text contents.
    pub fn parse_cube(text: &str) -> Result<Lut, String> {
        let mut size_3d: Option<usize> = None;
        let mut size_1d: Option<usize> = None;
        let mut domain_min = [0.0f32, 0.0, 0.0];
        let mut domain_max = [1.0f32, 1.0, 1.0];
        let mut table: Vec<[f32; 3]> = Vec::new();

        for raw in text.lines() {
            let line = raw.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let upper = line.to_ascii_uppercase();
            if upper.starts_with("TITLE") {
                continue;
            } else if upper.starts_with("LUT_3D_SIZE") {
                size_3d = Some(Self::parse_last_usize(line)?);
            } else if upper.starts_with("LUT_1D_SIZE") {
                size_1d = Some(Self::parse_last_usize(line)?);
            } else if upper.starts_with("DOMAIN_MIN") {
                domain_min = Self::parse_triplet(line)?;
            } else if upper.starts_with("DOMAIN_MAX") {
                domain_max = Self::parse_triplet(line)?;
            } else {
                // Data row: three floats
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() == 3 {
                    let r = parts[0].parse::<f32>().map_err(|e| format!("bad value '{}': {e}", parts[0]))?;
                    let g = parts[1].parse::<f32>().map_err(|e| format!("bad value '{}': {e}", parts[1]))?;
                    let b = parts[2].parse::<f32>().map_err(|e| format!("bad value '{}': {e}", parts[2]))?;
                    table.push([r, g, b]);
                }
                // ignore any other unknown keyword lines
            }
        }

        if table.is_empty() {
            return Err("No LUT data found".into());
        }

        let (kind, size) = if let Some(n) = size_3d {
            if table.len() != n * n * n {
                return Err(format!("LUT_3D_SIZE {n} expects {} entries, found {}", n * n * n, table.len()));
            }
            (LutKind::Dim3, n)
        } else if let Some(n) = size_1d {
            if table.len() != n {
                return Err(format!("LUT_1D_SIZE {n} expects {n} entries, found {}", table.len()));
            }
            (LutKind::Dim1, n)
        } else {
            return Err("Missing LUT_3D_SIZE / LUT_1D_SIZE".into());
        };

        // Flatten to RGBA f32 (alpha = 1.0).
        let mut data = Vec::with_capacity(table.len() * 4);
        for px in &table {
            data.push(px[0]);
            data.push(px[1]);
            data.push(px[2]);
            data.push(1.0);
        }

        Ok(Lut { kind, size, domain_min, domain_max, data })
    }

    fn parse_last_usize(line: &str) -> Result<usize, String> {
        line.split_whitespace()
            .last()
            .and_then(|s| s.parse::<usize>().ok())
            .ok_or_else(|| format!("Cannot parse size from '{line}'"))
    }

    fn parse_triplet(line: &str) -> Result<[f32; 3], String> {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 4 {
            Ok([
                parts[1].parse::<f32>().map_err(|e| e.to_string())?,
                parts[2].parse::<f32>().map_err(|e| e.to_string())?,
                parts[3].parse::<f32>().map_err(|e| e.to_string())?,
            ])
        } else {
            Err(format!("Cannot parse triplet from '{line}'"))
        }
    }

    /// RGBA f32 data ready for a 2D texture of width = `tex_width()`, height = `tex_height()`.
    /// For a 3D LUT this is the cube laid out as N vertically-stacked NxN slices.
    /// For a 1D LUT this is a 1xN texture (width 1, height N).
    pub fn rgba_f32(&self) -> &[f32] {
        &self.data
    }

    pub fn tex_width(&self) -> usize {
        match self.kind {
            LutKind::Dim3 => self.size,
            LutKind::Dim1 => 1,
        }
    }

    pub fn tex_height(&self) -> usize {
        match self.kind {
            LutKind::Dim3 => self.size * self.size,
            LutKind::Dim1 => self.size,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_minimal_3d() {
        // 2x2x2 identity-ish cube
        let cube = "\
LUT_3D_SIZE 2
0.0 0.0 0.0
1.0 0.0 0.0
0.0 1.0 0.0
1.0 1.0 0.0
0.0 0.0 1.0
1.0 0.0 1.0
0.0 1.0 1.0
1.0 1.0 1.0
";
        let lut = Lut::parse_cube(cube).unwrap();
        assert_eq!(lut.kind, LutKind::Dim3);
        assert_eq!(lut.size, 2);
        assert_eq!(lut.data.len(), 8 * 4);
        assert_eq!(lut.tex_width(), 2);
        assert_eq!(lut.tex_height(), 4);
        // first entry RGBA
        assert_eq!(&lut.data[0..4], &[0.0, 0.0, 0.0, 1.0]);
        // second entry red=1
        assert_eq!(&lut.data[4..8], &[1.0, 0.0, 0.0, 1.0]);
    }

    #[test]
    fn parse_1d() {
        let cube = "\
# comment
TITLE \"test\"
LUT_1D_SIZE 3
0.0 0.0 0.0
0.5 0.5 0.5
1.0 1.0 1.0
";
        let lut = Lut::parse_cube(cube).unwrap();
        assert_eq!(lut.kind, LutKind::Dim1);
        assert_eq!(lut.size, 3);
        assert_eq!(lut.data.len(), 3 * 4);
        assert_eq!(lut.tex_width(), 1);
        assert_eq!(lut.tex_height(), 3);
    }

    #[test]
    fn parse_domain() {
        let cube = "\
LUT_3D_SIZE 2
DOMAIN_MIN 0.0 0.0 0.0
DOMAIN_MAX 1.0 1.0 1.0
0 0 0
0 0 0
0 0 0
0 0 0
0 0 0
0 0 0
0 0 0
0 0 0
";
        let lut = Lut::parse_cube(cube).unwrap();
        assert_eq!(lut.domain_min, [0.0, 0.0, 0.0]);
        assert_eq!(lut.domain_max, [1.0, 1.0, 1.0]);
    }

    #[test]
    fn wrong_count_errors() {
        let cube = "LUT_3D_SIZE 2\n0 0 0\n1 1 1\n";
        assert!(Lut::parse_cube(cube).is_err());
    }

    #[test]
    fn empty_errors() {
        assert!(Lut::parse_cube("# just a comment\n").is_err());
    }
}
