// SPDX-FileCopyrightText: 2026 Froststrap
//
// SPDX-License-Identifier: MPL-2.0

use std::str::FromStr;

use crate::error::{Error, Result};

pub type Rgb = [u8; 3];

#[derive(Debug, Clone)]
pub struct ColorStop {
    pub offset: f32,
    pub rgb: Rgb,
}

#[derive(Debug, Clone)]
pub struct Gradient(pub Vec<ColorStop>);

fn parse_hex(s: &str) -> Result<Rgb> {
    let s = s.trim().trim_start_matches('#');
    if s.len() != 6 {
        return Err(Error::Color(format!("hex color must be 6 characters: '{s}'")));
    }
    let v = u32::from_str_radix(s, 16).map_err(|_| Error::Color(format!("invalid hex '{s}'")))?;
    Ok([(v >> 16) as u8, (v >> 8) as u8, v as u8])
}

impl FromStr for Gradient {
    type Err = Error;

    fn from_str(arg: &str) -> Result<Self> {
        let parts: Vec<&str> = arg.split(',').map(str::trim).filter(|p| !p.is_empty()).collect();
        if parts.is_empty() {
            return Err(Error::Color("no color stops provided".into()));
        }

        let mut stops = if parts[0].contains(':') {
            parts
                .iter()
                .map(|p| {
                    let (off, col) = p
                        .split_once(':')
                        .ok_or_else(|| Error::Color(format!("invalid offset:color format: {p}")))?;
                    let offset: f32 = off
                        .parse()
                        .map_err(|_| Error::Color(format!("offset must be a number: {off}")))?;
                    if !(0.0..=1.0).contains(&offset) {
                        return Err(Error::Color(format!("offset must be between 0 and 1: {offset}")));
                    }
                    Ok(ColorStop { offset, rgb: parse_hex(col)? })
                })
                .collect::<Result<Vec<_>>>()?
        } else {
            let n = parts.len();
            parts
                .iter()
                .enumerate()
                .map(|(i, c)| {
                    Ok(ColorStop {
                        offset: if n == 1 { 0.0 } else { i as f32 / (n - 1) as f32 },
                        rgb: parse_hex(c)?,
                    })
                })
                .collect::<Result<Vec<_>>>()?
        };
        stops.sort_by(|a, b| a.offset.total_cmp(&b.offset));
        Ok(Self(stops))
    }
}

impl Gradient {
    pub fn sample(&self, t: f32) -> Rgb {
        let s = &self.0;
        let t = t.clamp(0.0, 1.0);
        if t < s[0].offset {
            return s[0].rgb;
        }
        for w in s.windows(2) {
            let (a, b) = (&w[0], &w[1]);
            if a.offset <= t && t <= b.offset {
                if a.offset == b.offset {
                    return a.rgb;
                }
                let r = (t - a.offset) / (b.offset - a.offset);
                let mix = |i: usize| {
                    (a.rgb[i] as f32 + (b.rgb[i] as f32 - a.rgb[i] as f32) * r).round() as u8
                };
                return [mix(0), mix(1), mix(2)];
            }
        }
        s[s.len() - 1].rgb
    }

    pub fn palette(&self, n_bands: u16) -> Vec<Rgb> {
        (0..n_bands)
            .map(|i| {
                let t = if n_bands > 1 { i as f32 / (n_bands - 1) as f32 } else { 0.5 };
                self.sample(t)
            })
            .collect()
    }
}
