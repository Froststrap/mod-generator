// SPDX-FileCopyrightText: 2026 Froststrap
//
// SPDX-License-Identifier: MPL-2.0

use std::{
    collections::HashMap,
    fs,
    io::BufWriter,
    path::{Path, PathBuf},
    sync::LazyLock,
};

use image::{
    ImageEncoder, RgbaImage,
    codecs::png::{CompressionType, FilterType as PngFilter, PngEncoder},
    imageops::{self, FilterType},
};
use rayon::prelude::*;
use regex::Regex;

use crate::{
    color::{Gradient, Rgb},
    error::{Error, IoContext, Result},
};

const LOGO_SPRITE: &str = "icons/logo/block";
const SPINNER_SPRITE: &str = "icons/graphic/loadingspinner";

const BLACKLIST_PREFIXES: [&str; 5] = [
    "chat_bubble/",
    "component_assets/",
    "icons/controls/voice/",
    "icons/graphic/",
    "squircles/",
];
const BLACKLIST_KEYWORDS: [&str; 2] = ["goldrobux", "icons/common/play"];
const BLACKLIST_STRICT: [&str; 1] = ["gradient/gradient_0_100"];
const BLACKLIST_EXCEPTION: &str = "icons/graphic/lock";

const IMAGE_SET_TABLES: [&str; 3] = ["assets_1x", "assets_2x", "assets_3x"];

static ENTRY: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"\['([^']+)'\]\s*=\s*\{\s*ImageRectOffset\s*=\s*Vector2\.new\((\d+),\s*(\d+)\)\s*,\s*ImageRectSize\s*=\s*Vector2\.new\((\d+),\s*(\d+)\)\s*,\s*ImageSet\s*=\s*'([^']+)'",
    )
    .expect("valid sprite regex")
});

#[derive(Debug, Clone)]
pub struct Fill {
    stops: Vec<(f32, Rgb)>,
    angle: f32,
}

impl Fill {
    pub fn new(gradient: &Gradient, angle: f32) -> Self {
        Self {
            stops: gradient.0.iter().map(|s| (s.offset, s.rgb)).collect(),
            angle,
        }
    }

    fn sample(&self, t: f32) -> Rgb {
        let s = &self.stops;
        let (first, last) = (s[0], s[s.len() - 1]);
        if t <= first.0 {
            return first.1;
        }
        if t >= last.0 {
            return last.1;
        }

        let (left, right) = s
            .windows(2)
            .find(|w| t >= w[0].0 && t <= w[1].0)
            .map_or((first, last), |w| (w[0], w[1]));

        let span = right.0 - left.0;
        let local = if span > 0.0 {
            ((t - left.0) / span).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let mix =
            |i: usize| (left.1[i] as f32 + (right.1[i] as f32 - left.1[i] as f32) * local) as u8;
        [mix(0), mix(1), mix(2)]
    }

    fn apply(&self, img: &mut RgbaImage, x0: u32, y0: u32, w: u32, h: u32) {
        let solid = (self.stops.len() == 1).then(|| self.stops[0].1);

        let rad = (self.angle as f64 - 90.0).to_radians();
        let (cos, sin) = (rad.cos(), rad.sin());
        let (fw, fh) = ((w as f64 - 1.0), (h as f64 - 1.0));
        let corners = [0.0, fw * cos, fh * sin, fw * cos + fh * sin];
        let min = corners.iter().copied().fold(f64::INFINITY, f64::min);
        let max = corners.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        let denom = if (max - min).abs() < 1e-6 {
            1.0
        } else {
            max - min
        };

        let w = w.min(img.width().saturating_sub(x0));
        let h = h.min(img.height().saturating_sub(y0));

        for y in 0..h {
            for x in 0..w {
                let px = img.get_pixel_mut(x0 + x, y0 + y);
                if px[3] == 0 {
                    continue;
                }
                let rgb = solid.unwrap_or_else(|| {
                    let t = ((x as f64 * cos + y as f64 * sin - min) / denom) as f32;
                    self.sample(t.clamp(0.0, 1.0))
                });
                px[0] = rgb[0];
                px[1] = rgb[1];
                px[2] = rgb[2];
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct Sprite {
    pub name: String,
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

fn load(path: &Path) -> Result<RgbaImage> {
    Ok(image::open(path)
        .map_err(|e| Error::image(path, e))?
        .to_rgba8())
}

fn save(img: &RgbaImage, path: &Path) -> Result<()> {
    let tmp = path.with_extension("png.tmp");
    let file = fs::File::create(&tmp).at(&tmp)?;
    PngEncoder::new_with_quality(
        BufWriter::new(file),
        CompressionType::Fast,
        PngFilter::Adaptive,
    )
    .write_image(
        img.as_raw(),
        img.width(),
        img.height(),
        image::ExtendedColorType::Rgba8,
    )
    .map_err(|e| Error::image(path, e))?;
    fs::rename(&tmp, path).at(path)
}

pub fn recolor_image(path: &Path, fill: &Fill) -> Result<()> {
    let mut img = load(path)?;
    let (w, h) = img.dimensions();
    fill.apply(&mut img, 0, 0, w, h);
    save(&img, path)
}

pub fn recolor_images(paths: &[PathBuf], fill: &Fill) -> Vec<Result<()>> {
    paths.par_iter().map(|p| recolor_image(p, fill)).collect()
}

fn is_blacklisted(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    if name.starts_with(BLACKLIST_EXCEPTION) {
        return false;
    }
    BLACKLIST_PREFIXES.iter().any(|p| name.starts_with(p))
        || BLACKLIST_KEYWORDS.iter().any(|k| name.contains(k))
        || BLACKLIST_STRICT.iter().any(|s| name == *s)
}

fn matching_brace(text: &str, open: usize) -> Option<usize> {
    let mut depth = 0usize;
    for (i, b) in text.bytes().enumerate().skip(open) {
        match b {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

pub fn parse_image_set_data(lua_path: &Path) -> Result<Vec<(PathBuf, Vec<Sprite>)>> {
    let text = fs::read_to_string(lua_path).at(lua_path)?;
    let sheets_dir = lua_path
        .parent()
        .and_then(Path::parent)
        .map(|p| p.join("SpriteSheets"))
        .ok_or_else(|| Error::image(lua_path, "image set data has no parent directory"))?;

    let mut sheets: Vec<(PathBuf, Vec<Sprite>)> = Vec::new();
    let mut index: HashMap<String, usize> = HashMap::new();

    for table in IMAGE_SET_TABLES {
        let header = format!("function make_{table}() {table} = {{");
        let Some(start) = text.find(&header) else {
            continue;
        };
        let Some(open) = text[start..].find('{').map(|i| start + i) else {
            continue;
        };
        let Some(close) = matching_brace(&text, open) else {
            continue;
        };

        for cap in ENTRY.captures_iter(&text[open + 1..close]) {
            let num = |i: usize| cap[i].parse::<u32>().unwrap_or(0);
            let sprite = Sprite {
                name: cap[1].to_string(),
                x: num(2),
                y: num(3),
                w: num(4),
                h: num(5),
            };
            let set = cap[6].to_string();
            let slot = *index.entry(set.clone()).or_insert_with(|| {
                sheets.push((sheets_dir.join(format!("{set}.png")), Vec::new()));
                sheets.len() - 1
            });
            sheets[slot].1.push(sprite);
        }
    }

    Ok(sheets)
}

fn replace_sprite(sheet: &mut RgbaImage, sprite: &Sprite, custom: &Path) -> Result<bool> {
    if sprite.w == 0 || sprite.h == 0 {
        return Ok(false);
    }
    let image = load(custom)?;
    let scale =
        (sprite.w as f32 / image.width() as f32).min(sprite.h as f32 / image.height() as f32);
    let sw = ((image.width() as f32 * scale).round() as u32).clamp(1, sprite.w);
    let sh = ((image.height() as f32 * scale).round() as u32).clamp(1, sprite.h);
    let resized = imageops::resize(&image, sw, sh, FilterType::Triangle);

    let (ox, oy) = (
        sprite.x + (sprite.w - sw) / 2,
        sprite.y + (sprite.h - sh) / 2,
    );
    for (x, y, px) in resized.enumerate_pixels() {
        let (tx, ty) = (ox + x, oy + y);
        if px[3] > 0 && tx < sheet.width() && ty < sheet.height() {
            sheet.put_pixel(tx, ty, *px);
        }
    }
    Ok(true)
}

fn recolor_sheet(
    path: &Path,
    sprites: &[Sprite],
    fill: &Fill,
    logo: Option<&Path>,
    spinner: Option<&Path>,
) -> Result<bool> {
    if !path.is_file() {
        return Ok(false);
    }
    let mut sheet = load(path)?;
    let mut modified = false;

    for (custom, target) in [(logo, LOGO_SPRITE), (spinner, SPINNER_SPRITE)] {
        if let Some(custom) = custom.filter(|p| p.is_file())
            && let Some(sprite) = sprites.iter().find(|s| s.name.eq_ignore_ascii_case(target))
        {
            modified |= replace_sprite(&mut sheet, sprite, custom)?;
        }
    }

    for sprite in sprites {
        if sprite.w == 0 || sprite.h == 0 || is_blacklisted(&sprite.name) {
            continue;
        }
        if (logo.is_some() && sprite.name.eq_ignore_ascii_case(LOGO_SPRITE))
            || (spinner.is_some() && sprite.name.eq_ignore_ascii_case(SPINNER_SPRITE))
        {
            continue;
        }
        fill.apply(&mut sheet, sprite.x, sprite.y, sprite.w, sprite.h);
        modified = true;
    }

    if modified {
        save(&sheet, path)?;
    }
    Ok(modified)
}

pub fn recolor_sprite_sheets(
    image_set_data: &Path,
    fill: &Fill,
    logo: Option<&Path>,
    spinner: Option<&Path>,
) -> Result<Vec<Result<bool>>> {
    let sheets = parse_image_set_data(image_set_data)?;
    log::info!(
        "Found {} sprites across {} sheets",
        sheets.iter().map(|(_, s)| s.len()).sum::<usize>(),
        sheets.len()
    );

    Ok(sheets
        .par_iter()
        .map(|(path, sprites)| recolor_sheet(path, sprites, fill, logo, spinner))
        .collect())
}
