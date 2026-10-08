// SPDX-FileCopyrightText: 2026 Froststrap
//
// SPDX-License-Identifier: MPL-2.0

use std::{collections::BTreeMap, path::Path};

use image::imageops::FilterType;

use crate::{
    clip::{self, Fill},
    color::Rgb,
    geometry::{BBox, Contour},
};

const OPAQUE: u8 = 80;

struct Raster {
    w: usize,
    h: usize,
    px: Vec<[u8; 4]>,
}

fn load(path: &Path) -> Option<Raster> {
    let mut img = image::open(path).ok()?;
    let target = if img.width().max(img.height()) < 128 { 128 } else { 256 };
    if img.width() > target || img.height() > target {
        img = img.resize(target, target, FilterType::Lanczos3);
    }
    let rgba = img.to_rgba8();
    let (w, h) = rgba.dimensions();
    Some(Raster { w: w as usize, h: h as usize, px: rgba.pixels().map(|p| p.0).collect() })
}

struct Grid {
    scale: f64,
    top_x: f64,
    top_y: f64,
}

impl Grid {
    fn new(w: usize, h: usize, bbox: BBox) -> Self {
        let scale = (bbox.w.max(1.0) / w.max(1) as f64).min(bbox.h.max(1.0) / h.max(1) as f64);
        Self {
            scale,
            top_x: bbox.x_min + (bbox.w - w as f64 * scale) / 2.0,
            top_y: bbox.y_max - (bbox.h - h as f64 * scale) / 2.0,
        }
    }

    fn rect(&self, row: usize, x0: usize, x1: usize, pad: f64) -> Contour {
        let bot = self.top_y - (row + 1) as f64 * self.scale;
        let top = self.top_y - row as f64 * self.scale;
        let l = self.top_x + x0 as f64 * self.scale;
        let r = self.top_x + x1 as f64 * self.scale;
        vec![(l, bot - pad), (r + pad, bot - pad), (r + pad, top + pad), (l, top + pad)]
    }
}

pub fn mask_contours(path: &Path, bbox: BBox) -> Vec<Contour> {
    let Some(r) = load(path) else { return vec![] };
    let grid = Grid::new(r.w, r.h, bbox);

    let mut rects = Vec::new();
    for y in 0..r.h {
        let mut x = 0;
        while x < r.w {
            if r.px[y * r.w + x][3] >= OPAQUE {
                let start = x;
                while x < r.w && r.px[y * r.w + x][3] >= OPAQUE {
                    x += 1;
                }
                rects.push(grid.rect(y, start, x, 0.5));
            } else {
                x += 1;
            }
        }
    }
    if rects.is_empty() {
        return vec![];
    }
    let merged = clip::union(&rects);
    if merged.is_empty() { rects } else { merged }
}

fn quantize(r: &Raster, max_colors: u8) -> Option<(Vec<Rgb>, Vec<u8>)> {
    let pixels: Vec<imagequant::RGBA> =
        r.px.iter().map(|p| imagequant::RGBA::new(p[0], p[1], p[2], 255)).collect();
    let mut liq = imagequant::new();
    liq.set_max_colors(u32::from(max_colors).clamp(2, 256)).ok()?;
    let mut img = liq.new_image(pixels, r.w, r.h, 0.0).ok()?;
    let mut res = liq.quantize(&mut img).ok()?;
    res.set_dithering_level(0.0).ok()?;
    let (pal, idx) = res.remapped(&mut img).ok()?;
    Some((pal.iter().map(|c| [c.r, c.g, c.b]).collect(), idx))
}

pub fn color_layers(path: &Path, bbox: BBox, max_colors: u8) -> Vec<(Rgb, Vec<Contour>)> {
    let Some(mut r) = load(path) else { return vec![] };
    for p in &mut r.px {
        p[3] = if p[3] >= 128 { 255 } else { 0 };
    }
    let Some((palette, idx)) = quantize(&r, max_colors) else { return vec![] };
    let grid = Grid::new(r.w, r.h, bbox);

    let opaque = |x: usize, y: usize| r.px[y * r.w + x][3] >= OPAQUE;

    let mut by_color: BTreeMap<u8, Vec<Contour>> = BTreeMap::new();
    for y in 0..r.h {
        let mut x = 0;
        while x < r.w {
            if opaque(x, y) {
                let c = idx[y * r.w + x];
                let start = x;
                while x < r.w && opaque(x, y) && idx[y * r.w + x] == c {
                    x += 1;
                }
                by_color.entry(c).or_default().push(grid.rect(y, start, x, 0.0));
            } else {
                x += 1;
            }
        }
    }

    let all: Vec<Contour> = by_color.values().flatten().cloned().collect();
    let mut silhouette = clip::union(&all);
    if silhouette.is_empty() {
        return vec![];
    }

    let smooth = grid.scale * 0.5;
    if smooth * 1000.0 >= 1.0 {
        silhouette = clip::inflate(&silhouette, smooth);
        silhouette = clip::inflate(&silhouette, -smooth);
    }
    if silhouette.is_empty() {
        return vec![];
    }

    let grow = grid.scale * 1.5;
    by_color
        .into_iter()
        .filter_map(|(c, rects)| {
            let merged = clip::union(&rects);
            if merged.is_empty() {
                return None;
            }
            let dilated = clip::inflate(&merged, grow);
            let clipped = clip::intersect(&dilated, &silhouette, Fill::NonZero);
            (!clipped.is_empty()).then(|| (palette[c as usize], clipped))
        })
        .collect()
}
