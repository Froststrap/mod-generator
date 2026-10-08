// SPDX-FileCopyrightText: 2026 Froststrap
//
// SPDX-License-Identifier: MPL-2.0

use std::{collections::{HashMap, HashSet}, path::PathBuf, process::ExitCode};

use clap::Parser;
use mod_generator::{
    color::Gradient,
    data_types::Bootstrapper,
    error::{Error, Result},
    font::RecolorOptions,
    process::process_directory,
};

fn parse_image_pair(s: &str) -> Result<(String, PathBuf), Error> {
    let (glyph, path) = s
        .split_once(':')
        .ok_or_else(|| Error::ImageMap(format!("'{s}' (missing colon)")))?;
    Ok((glyph.trim().to_string(), PathBuf::from(path.trim())))
}

#[derive(Parser, Debug)]
#[command(name = "mod_generator")]
#[command(version = "3.0")]
#[command(about = "Generates mods", long_about = None)]
struct AppArgs {
    #[arg(long)]
    path: PathBuf,
    #[arg(long)]
    color: Gradient,
    #[arg(long, default_value_t = 0, allow_negative_numbers = true)]
    angle: i32,
    #[arg(long)]
    bands: Option<u16>,
    #[arg(long, value_enum, ignore_case = true, default_value_t = Bootstrapper::default())]
    bootstrapper: Bootstrapper,
    #[arg(long)]
    mod_name: Option<String>,
    #[arg(long, value_delimiter = ',', value_parser = parse_image_pair)]
    image_map: Vec<(String, PathBuf)>,
    #[arg(long, value_delimiter = ',')]
    skip_glyphs: Vec<String>,
    #[arg(long)]
    skip_color_matching: bool,
    #[arg(long, default_value_t = 64)]
    max_colors: u8,
}

fn run() -> Result<bool> {
    let args = AppArgs::parse();

    if !args.image_map.is_empty() {
        let names: Vec<&str> = args.image_map.iter().map(|(g, _)| g.as_str()).collect();
        println!("Loaded image map for glyphs: {}", names.join(", "));
    }
    if !args.skip_glyphs.is_empty() {
        println!("Will skip coloring these glyphs: {}", args.skip_glyphs.join(", "));
    }

    let opts = RecolorOptions {
        gradient: args.color,
        angle: args.angle,
        bands: args.bands,
        image_map: args.image_map.into_iter().collect::<HashMap<_, _>>(),
        skip_glyphs: args.skip_glyphs.into_iter().collect::<HashSet<_>>(),
        skip_color_matching: args.skip_color_matching,
        max_colors: args.max_colors,
    };

    let summary = process_directory(&args.path, &opts, args.bootstrapper, args.mod_name.as_deref())?;
    Ok(summary.failed == 0)
}

fn main() -> ExitCode {
    match run() {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}
