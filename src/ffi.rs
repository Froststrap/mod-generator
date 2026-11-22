// SPDX-FileCopyrightText: 2026 Froststrap
//
// SPDX-License-Identifier: MPL-2.0

use std::{
    collections::{HashMap, HashSet},
    ffi::{CStr, CString, c_char, c_int},
    path::{Path, PathBuf},
    sync::OnceLock,
};

use crate::{
    color::Gradient,
    error::Result,
    font::{RecolorOptions, parse_image_pair},
    process::recolor_directory,
    texture::{
        Fill, recolor_images as recolor_image_list, recolor_sprite_sheets as recolor_sheets,
    },
};

const DEFAULT_MAX_COLORS: u8 = 64;

/// Levels are 1 = error, 2 = warn, 3 = info, 4 = debug, 5 = trace.
pub type LoggerCallback =
    extern "C" fn(level: c_int, target: *const c_char, message: *const c_char);

static CALLBACK: OnceLock<LoggerCallback> = OnceLock::new();

struct HostLogger;

impl log::Log for HostLogger {
    fn enabled(&self, _: &log::Metadata) -> bool {
        CALLBACK.get().is_some()
    }

    fn log(&self, record: &log::Record) {
        let Some(callback) = CALLBACK.get() else {
            return;
        };
        let level = match record.level() {
            log::Level::Error => 1,
            log::Level::Warn => 2,
            log::Level::Info => 3,
            log::Level::Debug => 4,
            log::Level::Trace => 5,
        };
        let target = CString::new(record.target()).unwrap_or_default();
        let message =
            CString::new(record.args().to_string().replace('\0', "\\0")).unwrap_or_default();
        callback(level, target.as_ptr(), message.as_ptr());
    }

    fn flush(&self) {}
}

static LOGGER: HostLogger = HostLogger;

#[unsafe(no_mangle)]
pub extern "C" fn set_logger(callback: LoggerCallback) {
    let _ = CALLBACK.set(callback);
    let _ = log::set_logger(&LOGGER);
    log::set_max_level(log::LevelFilter::Debug);
}

unsafe fn read_str<'a>(ptr: *const c_char) -> Option<&'a str> {
    if ptr.is_null() {
        return None;
    }
    let s = unsafe { CStr::from_ptr(ptr) }.to_str().ok()?.trim();
    (!s.is_empty()).then_some(s)
}

fn split_list(s: Option<&str>) -> impl Iterator<Item = &str> {
    s.into_iter()
        .flat_map(|s| s.split(','))
        .map(str::trim)
        .filter(|s| !s.is_empty())
}

fn count_failures<T>(results: &[Result<T>]) -> i32 {
    results
        .iter()
        .filter_map(|r| r.as_ref().err())
        .inspect(|e| log::error!("{e}"))
        .count() as i32
}

fn parse_fill(color: Option<&str>, angle: f32) -> Option<Fill> {
    let color = color.or_else(|| {
        log::error!("no color given");
        None
    })?;
    match color.parse::<Gradient>() {
        Ok(gradient) => Some(Fill::new(&gradient, angle)),
        Err(e) => {
            log::error!("{e}");
            None
        }
    }
}

fn build_font_options(
    color: &str,
    angle: i32,
    bands: u16,
    image_map: Option<&str>,
    skip_glyphs: Option<&str>,
) -> Result<RecolorOptions> {
    Ok(RecolorOptions {
        gradient: color.parse::<Gradient>()?,
        angle,
        bands: (bands != 0).then_some(bands),
        image_map: split_list(image_map)
            .map(parse_image_pair)
            .collect::<Result<HashMap<_, _>>>()?,
        skip_glyphs: split_list(skip_glyphs)
            .map(str::to_owned)
            .collect::<HashSet<_>>(),
        skip_color_matching: false,
        max_colors: DEFAULT_MAX_COLORS,
    })
}

/// Recolors every `.ttf` under `path`, writing a colored `.otf` next to each one.
///
/// `color` is `RRGGBB[,RRGGBB...]` or `offset:RRGGBB[,offset:RRGGBB...]`.
/// `bands` of 0 picks the default. `image_map` (`glyph:path,...`) and
/// `skip_glyphs` (`glyph,...`) may be null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn recolor_fonts(
    path: *const c_char,
    color: *const c_char,
    angle: i32,
    bands: u16,
    image_map: *const c_char,
    skip_glyphs: *const c_char,
) -> i32 {
    let (Some(path), Some(color)) = (unsafe { read_str(path) }, unsafe { read_str(color) }) else {
        log::error!("recolor_fonts called without a path or color");
        return -1;
    };

    let opts = match build_font_options(
        color,
        angle,
        bands,
        unsafe { read_str(image_map) },
        unsafe { read_str(skip_glyphs) },
    ) {
        Ok(opts) => opts,
        Err(e) => {
            log::error!("{e}");
            return -1;
        }
    };

    match recolor_directory(Path::new(path), &opts) {
        Ok(results) => {
            let failed = count_failures(&results);
            log::info!("Recolored {} fonts ({failed} failed)", results.len());
            failed
        }
        Err(e) => {
            log::error!("{e}");
            -1
        }
    }
}

/// Recolors `count` PNGs in place. Null or empty entries in `paths` are skipped.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn recolor_images(
    paths: *const *const c_char,
    count: usize,
    color: *const c_char,
    angle: f32,
) -> i32 {
    let Some(fill) = parse_fill(unsafe { read_str(color) }, angle) else {
        return -1;
    };
    if paths.is_null() && count != 0 {
        log::error!("recolor_images called without paths");
        return -1;
    }

    let paths: Vec<PathBuf> = (0..count)
        .filter_map(|i| unsafe { read_str(*paths.add(i)) })
        .map(PathBuf::from)
        .collect();

    let results = recolor_image_list(&paths, &fill);
    let failed = count_failures(&results);
    log::info!("Recolored {} images ({failed} failed)", results.len());
    failed
}

/// Recolors the sprite sheets described by `image_set_data` (`GetImageSetData.lua`),
/// optionally replacing the Roblox logo and loading spinner with custom images.
/// `custom_logo` and `custom_spinner` may be null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn recolor_sprite_sheets(
    image_set_data: *const c_char,
    color: *const c_char,
    angle: f32,
    custom_logo: *const c_char,
    custom_spinner: *const c_char,
) -> i32 {
    let Some(data) = (unsafe { read_str(image_set_data) }) else {
        log::error!("recolor_sprite_sheets called without image set data");
        return -1;
    };
    let Some(fill) = parse_fill(unsafe { read_str(color) }, angle) else {
        return -1;
    };
    let logo = unsafe { read_str(custom_logo) }.map(Path::new);
    let spinner = unsafe { read_str(custom_spinner) }.map(Path::new);

    match recolor_sheets(Path::new(data), &fill, logo, spinner) {
        Ok(results) => {
            let failed = count_failures(&results);
            let changed = results.iter().filter(|r| matches!(r, Ok(true))).count();
            log::info!(
                "Recolored {changed} of {} sprite sheets ({failed} failed)",
                results.len()
            );
            failed
        }
        Err(e) => {
            log::error!("{e}");
            -1
        }
    }
}
