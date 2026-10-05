# Froststrap
# Copyright (c) Froststrap Team
#
# This file is part of Froststrap and is distributed under the terms of the
# GNU Affero General Public License, version 3 or later.
#
# SPDX-License-Identifier: AGPL-3.0-or-later

import argparse
import math
import os
import sys
from pathlib import Path

import numpy as np
import pyclipper
from fontTools.colorLib.builder import buildCOLR, buildCPAL
from fontTools.pens.recordingPen import RecordingPen
from fontTools.pens.ttGlyphPen import TTGlyphPen
from fontTools.ttLib import TTFont
from PIL import Image

from platform_utils import (
    copy_font_to_bootstrapper,
    get_default_bootstrapper_for_platform,
    write_buildericons_json,
)

BOOTSTRAPPERS = [
    "Bloxstrap",
    "Fishstrap",
    "Froststrap",
    "Luczystrap",
    "Lunastrap",
    "Sober",
]

SUPPORTED_EXTENSIONS = (".ttf",)

SUB_GLYPH_CACHE = {}
BEZIER_STEPS = 12


def hex_to_rgb(hex_str):
    hex_str = hex_str.strip().lstrip("#")
    if len(hex_str) != 6:
        raise ValueError("Hex color must be 6 characters long")
    return (
        int(hex_str[0:2], 16),
        int(hex_str[2:4], 16),
        int(hex_str[4:6], 16),
    )


def canonicalize_bootstrapper(name):
    if not name:
        return None
    for b in BOOTSTRAPPERS:
        if b.lower() == name.lower():
            return b
    raise ValueError(f"Invalid bootstrapper '{name}'")


def rotate_points(points, angle_deg, cx=0, cy=0):
    """Rotate a list of (x, y) points by angle_deg around (cx, cy)."""
    rad = math.radians(angle_deg)
    cos_a = math.cos(rad)
    sin_a = math.sin(rad)
    rotated = []
    for x, y in points:
        dx = x - cx
        dy = y - cy
        rx = cx + dx * cos_a - dy * sin_a
        ry = cy + dx * sin_a + dy * cos_a
        rotated.append((rx, ry))
    return rotated


def _quad_sample(p0, p1, p2):
    out = []
    for i in range(1, BEZIER_STEPS + 1):
        t = i / BEZIER_STEPS
        mt = 1.0 - t
        out.append(
            (
                mt * mt * p0[0] + 2 * mt * t * p1[0] + t * t * p2[0],
                mt * mt * p0[1] + 2 * mt * t * p1[1] + t * t * p2[1],
            )
        )
    return out


def _cubic_sample(p0, p1, p2, p3):
    out = []
    for i in range(1, BEZIER_STEPS + 1):
        t = i / BEZIER_STEPS
        mt = 1.0 - t
        out.append(
            (
                mt**3 * p0[0]
                + 3 * mt**2 * t * p1[0]
                + 3 * mt * t**2 * p2[0]
                + t**3 * p3[0],
                mt**3 * p0[1]
                + 3 * mt**2 * t * p1[1]
                + 3 * mt * t**2 * p2[1]
                + t**3 * p3[1],
            )
        )
    return out


def _append_qcurve(result, start, pts):
    off_curves, end = pts[:-1], pts[-1]
    if not off_curves:
        result.append(end)
        return
    if len(off_curves) == 1:
        result.extend(_quad_sample(start, off_curves[0], end))
        return
    cur = start
    for i, off in enumerate(off_curves):
        nxt = (
            ((off[0] + off_curves[i + 1][0]) / 2, (off[1] + off_curves[i + 1][1]) / 2)
            if i < len(off_curves) - 1
            else end
        )
        result.extend(_quad_sample(cur, off, nxt))
        cur = nxt


def _recording_to_polygons(recording):
    contours, current = [], []

    def _commit_contour():
        if len(current) >= 3:
            if current[0] != current[-1]:
                current.append(current[0])
            contours.append(list(current))

    for op, args in recording:
        if op == "moveTo":
            _commit_contour()
            current = [args[0]]
        elif op == "lineTo":
            current.append(args[0]) if current else current.extend([args[0]])
        elif op == "qCurveTo" and current:
            _append_qcurve(current, current[-1], list(args))
        elif op == "curveTo" and current:
            current.extend(_cubic_sample(current[-1], args[0], args[1], args[2]))
        elif op in ("closePath", "endPath"):
            _commit_contour()
            current = []
    _commit_contour()
    return contours


def _get_outline_contours(font, glyph_name):
    if glyph_name not in font.getGlyphSet():
        return []
    rec = RecordingPen()
    try:
        font.getGlyphSet()[glyph_name].draw(rec)
    except Exception:
        return []
    return _recording_to_polygons(rec.value)


def _clip_contours_to_band(contours, lo, hi, min_coord, max_coord, axis="y"):
    if not contours:
        return []
    pc = pyclipper.Pyclipper()
    SCALE = 1000.0
    for poly in contours:
        if len(poly) < 3:
            continue
        vals = [pt[0] if axis == "x" else pt[1] for pt in poly]
        if max(vals) < lo or min(vals) > hi:
            continue
        cleaned = pyclipper.CleanPolygon(
            [(int(x * SCALE), int(y * SCALE)) for x, y in poly]
        )
        if len(cleaned) >= 3:
            try:
                pc.AddPath(cleaned, pyclipper.PT_SUBJECT, True)
            except pyclipper.ClipperException:
                continue
    safe_min = int((min_coord - 1000) * SCALE)
    safe_max = int((max_coord + 1000) * SCALE)
    if axis == "y":
        rect = [
            (safe_min, int(lo * SCALE)),
            (safe_max, int(lo * SCALE)),
            (safe_max, int(hi * SCALE)),
            (safe_min, int(hi * SCALE)),
        ]
    else:
        rect = [
            (int(lo * SCALE), safe_min),
            (int(hi * SCALE), safe_min),
            (int(hi * SCALE), safe_max),
            (int(lo * SCALE), safe_max),
        ]
    try:
        pc.AddPath(rect, pyclipper.PT_CLIP, True)
        return [
            [(pt[0] / SCALE, pt[1] / SCALE) for pt in poly]
            for poly in pc.Execute(
                pyclipper.CT_INTERSECTION, pyclipper.PFT_EVENODD, pyclipper.PFT_EVENODD
            )
            if len(poly) >= 3
        ]
    except pyclipper.ClipperException:
        return []


def _write_sub_glyph(icon_name, band_idx, contours, font, glyf_table, orig_aw):
    int_contours = []
    for poly in contours:
        rounded = [(round(px), round(py)) for px, py in poly]
        dedup = []
        for pt in rounded:
            if not dedup or pt != dedup[-1]:
                dedup.append(pt)
        if len(dedup) > 1 and dedup[0] == dedup[-1]:
            dedup.pop()
        if len(dedup) < 3:
            continue
        simplified = [dedup[0]]
        for i in range(1, len(dedup) - 1):
            p1, p2, p3 = simplified[-1], dedup[i], dedup[i + 1]
            cross = (p2[0] - p1[0]) * (p3[1] - p1[1]) - (p2[1] - p1[1]) * (
                p3[0] - p1[0]
            )
            if abs(cross) > 25.0:
                simplified.append(p2)
        simplified.append(dedup[-1])
        if len(simplified) < 3:
            continue
        xs = [p[0] for p in simplified]
        ys = [p[1] for p in simplified]
        area = sum(
            simplified[i][0] * simplified[(i + 1) % len(simplified)][1]
            - simplified[(i + 1) % len(simplified)][0] * simplified[i][1]
            for i in range(len(simplified))
        )
        if max(xs) - min(xs) < 3 or max(ys) - min(ys) < 3 or abs(area) < 40.0:
            continue
        int_contours.append(simplified)

    if not int_contours:
        return None

    fast_tuple = tuple(tuple(pt for pt in poly) for poly in int_contours)
    contour_hash = hash(fast_tuple)
    cache_key = f"{orig_aw}_{contour_hash}"
    if cache_key in SUB_GLYPH_CACHE:
        return SUB_GLYPH_CACHE[cache_key]

    pen = TTGlyphPen(None)
    for poly in int_contours:
        pen.moveTo(poly[0])
        for pt in poly[1:]:
            pen.lineTo(pt)
        pen.closePath()
    sub_name = f"{icon_name}.g{band_idx}"
    sub_glyph = pen.glyph()
    sub_glyph.recalcBounds(glyf_table)
    glyf_table[sub_name] = sub_glyph
    font["hmtx"].metrics[sub_name] = (orig_aw, int(getattr(sub_glyph, "xMin", 0)))

    SUB_GLYPH_CACHE[cache_key] = sub_name
    return sub_name


def interpolate_gradient(stops, t):
    """
    stops: list of (offset, (r,g,b)) tuples, offsets in [0,1], sorted.
    t: position in [0,1]
    returns (r,g,b,a)
    """
    t = max(0.0, min(1.0, t))
    if len(stops) == 1:
        r, g, b = stops[0][1]
        return r / 255.0, g / 255.0, b / 255.0, 1.0

    for i in range(len(stops) - 1):
        off1, col1 = stops[i]
        off2, col2 = stops[i + 1]
        if off1 <= t <= off2:
            if off2 == off1:
                r, g, b = col1
            else:
                ratio = (t - off1) / (off2 - off1)
                r = col1[0] + (col2[0] - col1[0]) * ratio
                g = col1[1] + (col2[1] - col1[1]) * ratio
                b = col1[2] + (col2[2] - col1[2]) * ratio
            return r / 255.0, g / 255.0, b / 255.0, 1.0

    r, g, b = stops[-1][1]
    return r / 255.0, g / 255.0, b / 255.0, 1.0


def parse_color_stops(color_arg):
    """
    Parse the --color argument.
    Returns a list of (offset, (r,g,b)) sorted by offset.
    Supports two formats:
      1) "#RRGGBB,#RRGGBB,..." -> equally spaced offsets 0..1
      2) "offset:#RRGGBB,offset:#RRGGBB,..." -> explicit offsets
    """
    parts = [p.strip() for p in color_arg.split(",") if p.strip()]
    if not parts:
        raise ValueError("No color stops provided")

    if ":" in parts[0]:
        stops = []
        for part in parts:
            if ":" not in part:
                raise ValueError(f"Invalid offset:color format: {part}")
            off_str, col_str = part.split(":", 1)
            try:
                offset = float(off_str)
            except ValueError:
                raise ValueError(f"Offset must be a number: {off_str}")
            if offset < 0 or offset > 1:
                raise ValueError(f"Offset must be between 0 and 1: {offset}")
            rgb = hex_to_rgb(col_str)
            stops.append((offset, rgb))
        stops.sort(key=lambda x: x[0])
        return stops
    else:
        hex_list = parts
        n = len(hex_list)
        if n == 1:
            return [(0.0, hex_to_rgb(hex_list[0]))]
        stops = []
        for i, col in enumerate(hex_list):
            offset = i / (n - 1)
            stops.append((offset, hex_to_rgb(col)))
        return stops


def _get_native_color_contours(image_path, units, glyph_name, glyf_table, max_colors=64):
    try:
        with Image.open(image_path) as img:
            img = img.convert("RGBA")
            orig_w, orig_h = img.size
            tar_size = 128 if max(orig_w, orig_h) < 128 else 256
            img.thumbnail((tar_size, tar_size), resample=Image.Resampling.LANCZOS)
            alpha = img.getchannel("A").point(lambda p: 255 if p >= 128 else 0)
            img.putalpha(alpha)
            rgb = img.convert("RGB")
            rgb = rgb.quantize(
                colors=max_colors, 
                method=Image.Quantize.FASTOCTREE, 
                dither=Image.Dither.NONE
            )
            img = rgb.convert("RGBA")
            img.putalpha(alpha)
    except Exception:
        return {}
    arr = np.array(img)
    h, w = arr.shape[:2]
    og = glyf_table.get(glyph_name)
    if og and hasattr(og, "xMin"):
        x_min = float(og.xMin)
        y_max = float(og.yMax)
        bbox_w = float(og.xMax - og.xMin)
        bbox_h = float(og.yMax - og.yMin)
    else:
        x_min = 0.0
        y_max = float(units)
        bbox_w = float(units)
        bbox_h = float(units)
    scale = min(max(1.0, bbox_w) / max(1, w), max(1.0, bbox_h) / max(1, h))
    top_x = x_min + (bbox_w - (w * scale)) / 2.0
    top_y = y_max - (bbox_h - (h * scale)) / 2.0
    color_rects = {}
    for py in range(h):
        px = 0
        while px < w:
            r, g, b, a = arr[py, px]
            if a >= 80:
                start_x = px
                hex_col = f"#{r:02x}{g:02x}{b:02x}"
                while (
                    px < w
                    and arr[py, px][3] >= 80
                    and f"#{arr[py, px][0]:02x}{arr[py, px][1]:02x}{arr[py, px][2]:02x}"
                    == hex_col
                ):
                    px += 1
                if hex_col not in color_rects:
                    color_rects[hex_col] = []
                fy_bot = top_y - (py + 1) * scale
                fy_top = top_y - py * scale
                x0 = top_x + start_x * scale
                x1 = top_x + px * scale
                color_rects[hex_col].append(
                    [
                        (x0, fy_bot),
                        (x1, fy_bot),
                        (x1, fy_top),
                        (x0, fy_top),
                    ]
                )
            else:
                px += 1
    final_color_contours = {}
    SCALE = 1000.0
    pc_master = pyclipper.Pyclipper()
    for rects in color_rects.values():
        for poly in rects: 
            pc_master.AddPath([(int(x * SCALE), int(y * SCALE)) for x, y in poly], pyclipper.PT_SUBJECT, True)     
    try: 
        sil = pc_master.Execute(pyclipper.CT_UNION, pyclipper.PFT_NONZERO, pyclipper.PFT_NONZERO)
        if sil:
            smooth_radius = int(scale * 0.5 * SCALE)
            if smooth_radius > 0:
                pco_smooth = pyclipper.PyclipperOffset()
                pco_smooth.AddPaths(sil, pyclipper.JT_ROUND, pyclipper.ET_CLOSEDPOLYGON)
                sil = pco_smooth.Execute(smooth_radius)

                pco_smooth.Clear()
                pco_smooth.AddPaths(sil, pyclipper.JT_ROUND, pyclipper.ET_CLOSEDPOLYGON)
                sil = pco_smooth.Execute(-smooth_radius)
    except Exception: sil = []
    if not sil: return {}
    safe_expansion = scale * 1.5
    offset_delta = safe_expansion * SCALE
    for hex_col, rects in color_rects.items():
        pc = pyclipper.Pyclipper()
        for poly in rects:
            pc.AddPath(
                [(int(x * SCALE), int(y * SCALE)) for x, y in poly],
                pyclipper.PT_SUBJECT,
                True,
            )
        try:
            unioned = pc.Execute(pyclipper.CT_UNION, pyclipper.PFT_NONZERO, pyclipper.PFT_NONZERO)
            if not unioned: continue
            pco = pyclipper.PyclipperOffset()
            pco.AddPaths(unioned, pyclipper.JT_ROUND, pyclipper.ET_CLOSEDPOLYGON)
            dilated = pco.Execute(offset_delta)
            pc_clip = pyclipper.Pyclipper()
            pc_clip.AddPaths(dilated, pyclipper.PT_SUBJECT, True)
            pc_clip.AddPaths(sil, pyclipper.PT_CLIP, True)
            final_clipped = pc_clip.Execute(pyclipper.CT_INTERSECTION, pyclipper.PFT_NONZERO, pyclipper.PFT_NONZERO)
            result = [
                [(pt[0] / SCALE, pt[1] / SCALE) for pt in p]
                for p in final_clipped
                if len(p) >= 3
            ]
            if result:
                final_color_contours[hex_col] = result
        except:
            pass
    return final_color_contours

def _get_image_contours(image_path: str, units: int, icon_name: str, glyf_table) -> list[list[tuple]]:
    try:
        with Image.open(image_path, formats=("PNG",)) as img: 
            img = img.convert("RGBA")
        orig_w, orig_h = img.size
        tar_size = 128 if max(orig_w, orig_h) < 128 else 256
        img.thumbnail((tar_size, tar_size), resample=Image.Resampling.LANCZOS)
    except Exception: 
        return []
    arr = np.array(img)
    h, w = arr.shape[:2]
    og = glyf_table.get(icon_name)
    x_min, y_max, bbox_w, bbox_h = (float(og.xMin), float(og.yMax), float(og.xMax - og.xMin), float(og.yMax - og.yMin)) if og else (0.0, float(units), float(units), float(units))
    scale = min(max(1.0, bbox_w) / max(1, w), max(1.0, bbox_h) / max(1, h))
    top_x, top_y = x_min + (bbox_w - (w * scale)) / 2.0, y_max - (bbox_h - (h * scale)) / 2.0
    contours = []
    for py in range(h):
        px = 0
        while px < w:
            if arr[py, px][3] >= 80:
                start_x = px
                while px < w and arr[py, px][3] >= 80: 
                    px += 1
                fy_bot, fy_top = top_y - (py + 1) * scale, top_y - py * scale
                x0, x1 = top_x + start_x * scale, top_x + px * scale
                contours.append([(x0, fy_bot - 0.5), (x1 + 0.5, fy_bot - 0.5), (x1 + 0.5, fy_top + 0.5), (x0, fy_top + 0.5)])
            else: px += 1     
    if not contours: 
        return []
    pc = pyclipper.Pyclipper(); SCALE = 1000.0
    for poly in contours: 
        pc.AddPath([(int(x * SCALE), int(y * SCALE)) for x, y in poly], pyclipper.PT_SUBJECT, True)
    try: 
        return [[(pt[0] / SCALE, pt[1] / SCALE) for pt in p] for p in pc.Execute(pyclipper.CT_UNION, pyclipper.PFT_NONZERO, pyclipper.PFT_NONZERO)]
    except:
        return contours

def recolor_font(
    file_path,
    color_stops,
    angle=0,
    bands=None,
    bootstrapper=None,
    mod_name=None,
    image_map=None,
    skip_glyphs=None,
    skip_color_matching=False,
    max_colors=64
):
    global SUB_GLYPH_CACHE
    SUB_GLYPH_CACHE.clear()

    input_path = Path(file_path)
    output_path = input_path.with_suffix(".otf")

    try:
        font = TTFont(input_path)

        if "COLR" in font:
            del font["COLR"]

        if bands is None:
            n_bands = max(2, len(color_stops) * 8)
        else:
            n_bands = max(2, bands)

        angle = angle % 360
        if abs(angle) < 1e-6 or abs(angle - 180) < 1e-6:
            use_rotation = False
            effective_angle = 0
        elif abs(angle - 90) < 1e-6 or abs(angle - 270) < 1e-6:
            use_rotation = False
            effective_angle = 0
        else:
            use_rotation = True
            effective_angle = angle

        master_palette = []
        for i in range(n_bands):
            t = i / (n_bands - 1) if n_bands > 1 else 0.5
            r, g, b, a = interpolate_gradient(color_stops, t)
            master_palette.append((r, g, b, a))
        font["CPAL"] = buildCPAL([master_palette])

        glyf = font["glyf"]
        original_order = list(font.getGlyphOrder())
        extra_names = []
        color_glyphs = {}

        glyphs_to_process = [
            g for g in original_order if g not in (".notdef", ".null", "space")
        ]
        total = len(glyphs_to_process)
        processed = 0

        solid_palette_cache = {}

        skip_set = set(skip_glyphs) if skip_glyphs else set()

        def get_solid_color_idx(hex_col):
            if hex_col not in solid_palette_cache:
                r, g, b = hex_to_rgb(hex_col)
                idx = len(master_palette)
                master_palette.append((r / 255.0, g / 255.0, b / 255.0, 1.0))
                solid_palette_cache[hex_col] = idx
                font["CPAL"] = buildCPAL([master_palette])
            return solid_palette_cache[hex_col]

        print(f"Processing {total} glyphs with {n_bands} bands each...")
        if skip_set:
            print(f"Skipping {len(skip_set)} glyphs: {', '.join(sorted(skip_set))}")

        for glyph_name in glyphs_to_process:
            processed += 1
            if processed % 50 == 0 or processed == total:
                print(f"  Glyph {processed}/{total} ({glyph_name})", flush=True)

            if glyph_name in skip_set:
                continue

            img_path = None
            if image_map and glyph_name in image_map:
                img_path = image_map[glyph_name]

            polys = None
            if img_path and Path(img_path).is_file():
                units_per_em = font["head"].unitsPerEm
                if not skip_color_matching:
                    native_dict = _get_native_color_contours(
                        img_path, units_per_em, glyph_name, glyf, max_colors=max_colors
                    )
                    if native_dict:
                        orig_aw = font["hmtx"].metrics[glyph_name][0]
                        layers = []
                        for hex_col, contours in native_dict.items():
                            color_idx = get_solid_color_idx(hex_col)
                            sub_name = _write_sub_glyph(
                                glyph_name, hex_col, contours, font, glyf, orig_aw
                            )
                            if sub_name:
                                layers.append((sub_name, color_idx))
                                if sub_name not in extra_names:
                                    extra_names.append(sub_name)
                        if layers:
                            color_glyphs[glyph_name] = layers
                        continue
                else: polys = _get_image_contours(img_path, units_per_em, glyph_name, glyf)
            if not polys: polys = _get_outline_contours(font, glyph_name)
            if not polys: continue

            if use_rotation:
                polys_rot = [rotate_points(poly, effective_angle) for poly in polys]
                all_y = [pt[1] for poly in polys_rot for pt in poly]
                min_coord, max_coord = min(all_y), max(all_y)
                if max_coord - min_coord < 1:
                    continue
                all_x = [pt[0] for poly in polys_rot for pt in poly]
                x_min, x_max = min(all_x), max(all_x)
            else:
                if abs(angle - 90) < 1e-6 or abs(angle - 270) < 1e-6:
                    all_coords = [pt[0] for poly in polys for pt in poly]
                    min_coord, max_coord = min(all_coords), max(all_coords)
                    if max_coord - min_coord < 1:
                        continue
                    all_y = [pt[1] for poly in polys for pt in poly]
                    x_min, x_max = min(all_y), max(all_y)
                    polys_rot = polys
                    axis = "x"
                else:
                    all_coords = [pt[1] for poly in polys for pt in poly]
                    min_coord, max_coord = min(all_coords), max(all_coords)
                    if max_coord - min_coord < 1:
                        continue
                    all_x = [pt[0] for poly in polys for pt in poly]
                    x_min, x_max = min(all_x), max(all_x)
                    polys_rot = polys
                    axis = "y"

            orig_aw = font["hmtx"].metrics[glyph_name][0]
            band_step = (max_coord - min_coord) / n_bands
            layers = []

            for band in range(n_bands):
                lo = min_coord + band * band_step
                hi = min_coord + (band + 1) * band_step
                if band < n_bands - 1:
                    hi += 50.0

                if use_rotation:
                    clipped = _clip_contours_to_band(
                        polys_rot, lo, hi, x_min, x_max, axis="y"
                    )
                    if clipped:
                        clipped = [rotate_points(poly, -effective_angle) for poly in clipped]
                else:
                    clipped = _clip_contours_to_band(
                        polys_rot, lo, hi, x_min, x_max, axis=axis
                    )

                if not clipped:
                    continue

                sub_name = _write_sub_glyph(
                    glyph_name, band, clipped, font, glyf, orig_aw
                )
                if sub_name:
                    layers.append((sub_name, band))
                    if sub_name not in extra_names:
                        extra_names.append(sub_name)

            if layers:
                color_glyphs[glyph_name] = layers

        if extra_names:
            font.setGlyphOrder(original_order + extra_names)
        if color_glyphs:
            font["COLR"] = buildCOLR(color_glyphs)

        font.save(output_path)
        print(
            f"Processed (gradient, {len(color_stops)} stops, angle={angle}°, bands={n_bands}): {output_path}"
        )

        if bootstrapper:
            copy_font_to_bootstrapper(bootstrapper, output_path, mod_name=mod_name)

    except Exception as e:
        print(f"Error processing {file_path}: {e}")


def _derive_buildericons_dir_from_path(target_dir):
    p = Path(target_dir)
    markers = [
        "ExtraContent",
        "LuaPackages",
        "Packages",
        "_Index",
        "BuilderIcons",
        "BuilderIcons",
    ]
    for anc in [p] + list(p.parents):
        parts = list(anc.parts)
        if len(parts) >= len(markers):
            if [x.lower() for x in parts[-len(markers) :]] == [
                x.lower() for x in markers
            ]:
                return str(anc)
    return None


def process_directory(
    target_dir,
    color_stops,
    angle=0,
    bands=None,
    bootstrapper=None,
    mod_name=None,
    image_map=None,
    skip_glyphs=None,
    skip_color_matching=False,
    max_colors=64
):
    if not os.path.isdir(target_dir):
        print(f"Invalid directory: {target_dir}")
        sys.exit(1)

    count = 0
    for root, _, files in os.walk(target_dir):
        for file in files:
            if not file.lower().endswith(SUPPORTED_EXTENSIONS):
                continue
            if file.lower().endswith(".otf"):
                continue
            recolor_font(
                os.path.join(root, file),
                color_stops,
                angle=angle,
                bands=bands,
                bootstrapper=bootstrapper,
                mod_name=mod_name,
                image_map=image_map,
                skip_glyphs=skip_glyphs,
                skip_color_matching=skip_color_matching,
                max_colors=max_colors
            )
            count += 1

    print(f"Processed {count} files")

    if bootstrapper:
        write_buildericons_json(bootstrapper, mod_name=mod_name)
    else:
        derived = _derive_buildericons_dir_from_path(target_dir)
        if derived:
            write_buildericons_json(derived)


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--path", required=True)
    parser.add_argument(
        "--color",
        required=True,
        help="Gradient stops: either comma-separated hex colors (equally spaced) or offset:hex pairs, e.g. '0.0:#FF0000,0.5:#00FF00,1.0:#0000FF'",
    )
    parser.add_argument(
        "--angle",
        type=int,
        default=0,
        help="Gradient direction in degrees (0 = vertical top‑to‑bottom, 90 = horizontal left‑to‑right, any angle works)",
    )
    parser.add_argument(
        "--bands",
        type=int,
        default=None,
        help="Number of colour bands (default = stops × 8)",
    )
    parser.add_argument("--bootstrapper")
    parser.add_argument("--mod-name")
    parser.add_argument(
        "--image-map",
        default=None,
        help="Comma‑separated glyph:image_path pairs, e.g. 'uniF200:C:/img.png,another:img2.png'",
    )
    parser.add_argument(
        "--skip-glyphs",
        default=None,
        help="Comma‑separated list of glyph names to skip (do not color). Example: 'uniE001,uniE002,uniF123'",
    )
    parser.add_argument(
        "--skip-color-matching", 
        action="store_true",
        help="Disable automatic color inference for images"
    )
    parser.add_argument(
        "--max-colors",
        type=int,
        default=64,
        help="Maximum number of colors to inference from images",
    )
    args = parser.parse_args()

    try:
        color_stops = parse_color_stops(args.color)
    except Exception as e:
        print(f"Invalid color specification: {e}")
        sys.exit(1)

    bootstrapper = get_default_bootstrapper_for_platform()
    if args.bootstrapper:
        bootstrapper = canonicalize_bootstrapper(args.bootstrapper)

    image_map = None
    if args.image_map:
        image_map = {}
        for pair in args.image_map.split(","):
            if ":" not in pair:
                print(
                    f"Warning: ignoring invalid image-map entry '{pair}' (missing colon)"
                )
                continue
            glyph, path = pair.split(":", 1)
            image_map[glyph.strip()] = path.strip()
        if image_map:
            print(f"Loaded image map for glyphs: {', '.join(image_map.keys())}")

    skip_glyphs = None
    if args.skip_glyphs:
        skip_glyphs = [g.strip() for g in args.skip_glyphs.split(",") if g.strip()]
        if skip_glyphs:
            print(f"Will skip coloring these glyphs: {', '.join(skip_glyphs)}")

    process_directory(
        args.path,
        color_stops,
        angle=args.angle,
        bands=args.bands,
        bootstrapper=bootstrapper,
        mod_name=args.mod_name,
        image_map=image_map,
        skip_glyphs=skip_glyphs,
        skip_color_matching=args.skip_color_matching,
        max_colors=args.max_colors
    )