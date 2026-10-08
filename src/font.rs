// SPDX-FileCopyrightText: 2026 Froststrap
//
// SPDX-License-Identifier: MPL-2.0

use std::{
    collections::{BTreeMap, HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
};

use kurbo::BezPath;
use rayon::prelude::*;
use read_fonts::{FontRef, ReadError, TableProvider, types::Tag};
use skrifa::{
    GlyphId, MetadataProvider,
    instance::{LocationRef, Size},
    outline::{DrawSettings, OutlineGlyphCollection},
};
use write_fonts::{
    FontBuilder,
    from_obj::{ToOwnedObj, ToOwnedTable},
    tables::{
        colr::{BaseGlyph, Colr, Layer as ColrLayer},
        cpal::{ColorRecord, Cpal},
        glyf::{GlyfLocaBuilder, Glyph, SimpleGlyph},
        head::Head,
        hhea::Hhea,
        hmtx::{Hmtx, LongMetric},
        maxp::Maxp,
        post::Post,
    },
    types::{GlyphId16, Version16Dot16},
};

use crate::{
    color::{Gradient, Rgb},
    error::{Error, IoContext, Result},
    geometry::{BBox, Contour, Flattener, IntContour, Slicing, gradient_layers, to_int_contour},
    image_layers,
};

const NEVER_COLOR: [&str; 3] = [".notdef", ".null", "space"];

const DROP_TABLES: [Tag; 10] = [
    Tag::new(b"glyf"),
    Tag::new(b"loca"),
    Tag::new(b"head"),
    Tag::new(b"maxp"),
    Tag::new(b"hhea"),
    Tag::new(b"hmtx"),
    Tag::new(b"post"),
    Tag::new(b"COLR"),
    Tag::new(b"CPAL"),
    Tag::new(b"DSIG"),
];

#[derive(Debug, Clone)]
pub struct RecolorOptions {
    pub gradient: Gradient,
    pub angle: i32,
    pub bands: Option<u16>,
    pub image_map: HashMap<String, PathBuf>,
    pub skip_glyphs: HashSet<String>,
    pub skip_color_matching: bool,
    pub max_colors: u8,
}

impl RecolorOptions {
    pub fn n_bands(&self) -> u16 {
        let wanted = self.bands.unwrap_or((self.gradient.0.len() * 8) as u16);
        wanted.max(2)
    }
}

#[derive(Debug)]
pub enum LayerColor {
    Band(u16),
    Solid(Rgb),
}

#[derive(Debug)]
pub struct Layer {
    pub color: LayerColor,
    pub contours: Vec<IntContour>,
}

struct Ctx<'a> {
    outlines: OutlineGlyphCollection<'a>,
    opts: &'a RecolorOptions,
    units: u16,
    slicing: Slicing,
    n_bands: u16,
}

fn dbg_err<E: std::fmt::Debug>(path: &Path) -> impl Fn(E) -> Error + '_ {
    move |e| Error::font(path, format!("{e:?}"))
}

fn outline(outlines: &OutlineGlyphCollection<'_>, gid: u16) -> Vec<Contour> {
    let Some(glyph) = outlines.get(GlyphId::from(gid)) else { return vec![] };
    let mut pen = Flattener::default();
    let settings = DrawSettings::unhinted(Size::unscaled(), LocationRef::default());
    if glyph.draw(settings, &mut pen).is_err() {
        return vec![];
    }
    pen.finish()
}

fn glyph_layers(ctx: &Ctx<'_>, gid: u16, name: &str) -> Vec<Layer> {
    let outline = outline(&ctx.outlines, gid);
    let bbox = BBox::of(&outline).unwrap_or_else(|| BBox::em(ctx.units));
    let mut source = outline;

    if let Some(img) = ctx.opts.image_map.get(name).filter(|p| p.is_file()) {
        if ctx.opts.skip_color_matching {
            let mask = image_layers::mask_contours(img, bbox);
            if !mask.is_empty() {
                source = mask;
            }
        } else {
            let native = image_layers::color_layers(img, bbox, ctx.opts.max_colors);
            if !native.is_empty() {
                return native
                    .into_iter()
                    .filter_map(|(rgb, cs)| {
                        let ints: Vec<_> = cs.iter().filter_map(to_int_contour).collect();
                        (!ints.is_empty())
                            .then_some(Layer { color: LayerColor::Solid(rgb), contours: ints })
                    })
                    .collect();
            }
        }
    }

    if source.is_empty() {
        return vec![];
    }
    gradient_layers(&source, ctx.slicing, ctx.n_bands)
        .into_iter()
        .map(|(band, contours)| Layer { color: LayerColor::Band(band), contours })
        .collect()
}

fn simple_glyph(contours: &[IntContour]) -> std::result::Result<SimpleGlyph, String> {
    let mut path = BezPath::new();
    for c in contours {
        path.move_to((c[0].0 as f64, c[0].1 as f64));
        for p in &c[1..] {
            path.line_to((p.0 as f64, p.1 as f64));
        }
        path.close_path();
    }
    SimpleGlyph::from_bezpath(&path).map_err(|e| format!("{e:?}"))
}

pub fn recolor_font(path: &Path, opts: &RecolorOptions) -> Result<PathBuf> {
    let data = fs::read(path).at(path)?;
    let font = FontRef::new(&data).map_err(|e| Error::font(path, e))?;
    let re = |e: ReadError| Error::font(path, e);

    let units = font.head().map_err(re)?.units_per_em();
    let n = font.maxp().map_err(re)?.num_glyphs();
    let hmtx = font.hmtx().map_err(re)?;
    let names = font.glyph_names();
    let n_bands = opts.n_bands();

    let jobs: Vec<(u16, String)> = (0..n)
        .filter_map(|gid| {
            let name = names
                .get(GlyphId::from(gid))
                .map(|n| n.to_string())
                .unwrap_or_else(|| format!("glyph{gid:05}"));
            (!NEVER_COLOR.contains(&name.as_str()) && !opts.skip_glyphs.contains(&name))
                .then_some((gid, name))
        })
        .collect();

    println!(
        "{}: processing {} glyphs with {n_bands} bands each...",
        path.display(),
        jobs.len()
    );

    let ctx = Ctx {
        outlines: font.outline_glyphs(),
        opts,
        units,
        slicing: Slicing::from_angle(opts.angle),
        n_bands,
    };
    let results: Vec<(u16, Vec<Layer>)> = jobs
        .par_iter()
        .map(|(gid, name)| (*gid, glyph_layers(&ctx, *gid, name)))
        .filter(|(_, layers)| !layers.is_empty())
        .collect();

    let mut palette: Vec<Rgb> = opts.gradient.palette(n_bands);
    let mut solid_idx: HashMap<Rgb, u16> = HashMap::new();
    let mut dedupe: HashMap<(u16, Vec<IntContour>), u16> = HashMap::new();
    let mut new_glyphs: Vec<(u16, i16, Vec<IntContour>)> = Vec::new();
    let mut colr_map: BTreeMap<u16, Vec<(u16, u16)>> = BTreeMap::new();

    for (gid, layers) in results {
        let advance = hmtx.advance(GlyphId::from(gid)).unwrap_or(0);
        let mut recs = Vec::with_capacity(layers.len());
        for layer in layers {
            let pidx = match layer.color {
                LayerColor::Band(b) => b,
                LayerColor::Solid(rgb) => *solid_idx.entry(rgb).or_insert_with(|| {
                    palette.push(rgb);
                    (palette.len() - 1) as u16
                }),
            };
            let key = (advance, layer.contours);
            let sub = match dedupe.get(&key) {
                Some(&g) => g,
                None => {
                    let new_gid = u16::try_from(n as usize + new_glyphs.len())
                        .map_err(|_| Error::font(path, "too many glyphs after banding (> 65535)"))?;
                    let lsb = key.1.iter().flatten().map(|p| p.0).min().unwrap_or(0);
                    new_glyphs.push((advance, lsb, key.1.clone()));
                    dedupe.insert(key, new_gid);
                    new_gid
                }
            };
            recs.push((sub, pidx));
        }
        colr_map.insert(gid, recs);
    }

    let total = u16::try_from(n as usize + new_glyphs.len())
        .map_err(|_| Error::font(path, "too many glyphs after banding (> 65535)"))?;

    let glyf_r = font.glyf().map_err(re)?;
    let loca_r = font.loca(None).map_err(re)?;
    let mut builder = GlyfLocaBuilder::new();
    for gid in 0..n {
        let g: Glyph = match loca_r.get_glyf(GlyphId::from(gid), &glyf_r).map_err(re)? {
            Some(g) => g.to_owned_obj(glyf_r.offset_data()),
            None => Glyph::Empty,
        };
        builder.add_glyph(&g).map_err(|e| Error::font(path, e))?;
    }
    let (mut max_points, mut max_contours) = (0usize, 0usize);
    for (_, _, contours) in &new_glyphs {
        max_points = max_points.max(contours.iter().map(Vec::len).sum());
        max_contours = max_contours.max(contours.len());
        let sg = simple_glyph(contours).map_err(|e| Error::font(path, e))?;
        builder.add_glyph(&Glyph::Simple(sg)).map_err(|e| Error::font(path, e))?;
    }
    let (glyf_w, loca_w, loca_format) = builder.build();

    let mut head: Head = font.head().map_err(re)?.to_owned_table();
    head.index_to_loc_format = loca_format as i16;

    let mut maxp: Maxp = font.maxp().map_err(re)?.to_owned_table();
    maxp.num_glyphs = total;
    maxp.max_points = maxp.max_points.map(|m| m.max(max_points as u16));
    maxp.max_contours = maxp.max_contours.map(|m| m.max(max_contours as u16));

    let metrics: Vec<LongMetric> = (0..n)
        .map(|g| {
            let id = GlyphId::from(g);
            LongMetric::new(hmtx.advance(id).unwrap_or(0), hmtx.side_bearing(id).unwrap_or(0))
        })
        .chain(new_glyphs.iter().map(|(a, l, _)| LongMetric::new(*a, *l)))
        .collect();
    let hmtx_w = Hmtx::new(metrics, vec![]);

    let mut hhea: Hhea = font.hhea().map_err(re)?.to_owned_table();
    hhea.number_of_h_metrics = total;

    let mut post: Post = font.post().map_err(re)?.to_owned_table();
    post.version = Version16Dot16::VERSION_3_0;
    post.num_glyphs = None;
    post.glyph_name_index = None;
    post.string_data = None;

    let n_colors = palette.len() as u16;
    let records: Vec<ColorRecord> =
        palette.iter().map(|c| ColorRecord::new(c[2], c[1], c[0], 255)).collect();
    let cpal = Cpal::new(n_colors, 1, n_colors, Some(records), vec![0]);


    let mut base_records = Vec::with_capacity(colr_map.len());
    let mut layer_records = Vec::new();
    for (base, recs) in &colr_map {
        base_records.push(BaseGlyph::new(
            GlyphId16::new(*base),
            layer_records.len() as u16,
            recs.len() as u16,
        ));
        for (sub, pidx) in recs {
            layer_records.push(ColrLayer::new(GlyphId16::new(*sub), *pidx));
        }
    }

    let colr = Colr::new(
        base_records.len() as u16,
        Some(base_records),
        Some(layer_records.clone()),
        layer_records.len() as u16,
    );
    
    let mut out = FontBuilder::new();
    out.add_table(&glyf_w).map_err(dbg_err(path))?;
    out.add_table(&loca_w).map_err(dbg_err(path))?;
    out.add_table(&head).map_err(dbg_err(path))?;
    out.add_table(&maxp).map_err(dbg_err(path))?;
    out.add_table(&hhea).map_err(dbg_err(path))?;
    out.add_table(&hmtx_w).map_err(dbg_err(path))?;
    out.add_table(&post).map_err(dbg_err(path))?;
    out.add_table(&cpal).map_err(dbg_err(path))?;
    if !colr_map.is_empty() {
        out.add_table(&colr).map_err(dbg_err(path))?;
    }
    for rec in font.table_directory.table_records() {
        let tag = rec.tag();
        if DROP_TABLES.contains(&tag) {
            continue;
        }
        if let Some(d) = font.table_data(tag) {
            out.add_raw(tag, d.as_bytes());
        }
    }

    let dest = path.with_extension("otf");
    fs::write(&dest, out.build()).at(&dest)?;
    println!(
        "Processed (gradient, {} stops, angle={}, bands={n_bands}): {}",
        opts.gradient.0.len(),
        opts.angle.rem_euclid(360),
        dest.display()
    );
    Ok(dest)
}
