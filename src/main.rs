// Froststrap
// Copyright (c) Froststrap Team
//
// This file is part of Froststrap and is distributed under the terms of the
// Mozilla Public License 2.0.
//
// SPDX-License-Identifier: MPL-2.0

use clap::Parser;
use std::path::PathBuf;
use mod_generator::data_types::{Bootstrapper,FontDir};

fn parse_image_pair(s: &str) -> Result<(String, PathBuf), String> {
    let (glyph, path) = s
        .split_once(':')
        .ok_or_else(|| format!("invalid image-map entry '{s}' (missing colon)"))?;
    Ok((glyph.trim().to_string(), PathBuf::from(path.trim())))
}

#[derive(Parser, Debug)]
#[command(name = "mod_generator")]
#[command(version = "3.0")]
#[command(about = "Generates mods", long_about = None)]
struct AppArgs {
    #[arg(long)]
    path: String,
    #[arg(long)]
    color: String,
    #[arg(long)]
    angle: u16,
    #[arg(long)]
    bands: u8,
    #[arg(long, value_enum, ignore_case = true, default_value_t = Bootstrapper::default())]
    bootstrapper: Bootstrapper,
    #[arg(long)]
    mod_name: String,
    #[arg(long, value_delimiter = ',', value_parser = parse_image_pair)]
    image_map: Vec<(String, PathBuf)>,
    #[arg(long, value_delimiter = ',')]
    skip_glyphs: Vec<String>,
    #[arg(long)]
    skip_color_matching: bool,
    #[arg(long)]
    max_colors: u8,    
}

pub fn main() {
    let cli = AppArgs::parse();

    println!("cli={cli:#?}");
    println!("Hello!");
    println!("FontDir={:?}", FontDir::get(Bootstrapper::Sober, Some("bumtimks".into())).unwrap())
}
