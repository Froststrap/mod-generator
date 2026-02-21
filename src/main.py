# Froststrap
# Copyright (c) Froststrap Team
#
# This file is part of Froststrap and is distributed under the terms of the
# GNU Affero General Public License, version 3 or later.
#
# SPDX-License-Identifier: AGPL-3.0-or-later

import argparse
import os
import sys
from pathlib import Path
from platform import (
    copy_font_to_bootstrapper,
    get_default_bootstrapper_for_platform,
    write_buildericons_json,
)

from fontTools.ttLib import TTFont
from fontTools.ttLib.tables.C_O_L_R_ import LayerRecord, table_C_O_L_R_
from fontTools.ttLib.tables.C_P_A_L_ import Color, table_C_P_A_L_

BOOTSTRAPPERS = [
    "Bloxstrap",
    "Fishstrap",
    "Froststrap",
    "Luczystrap",
    "Lunastrap",
    "Sober",
]

SUPPORTED_EXTENSIONS = (".ttf", ".otf")


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


def recolor_font(file_path, rgb_color, bootstrapper=None, mod_name=None):
    input_path = Path(file_path)
    output_path = (
        input_path.with_suffix(".otf")
        if input_path.suffix.lower() != ".otf"
        else input_path
    )

    try:
        font = TTFont(input_path)

        if "COLR" in font:
            del font["COLR"]

        r, g, b = rgb_color
        cpal = table_C_P_A_L_()
        cpal.version = 0
        cpal.palettes = [[Color(b, g, r, 255)]]
        cpal.numPaletteEntries = 1
        font["CPAL"] = cpal

        colr = table_C_O_L_R_()
        colr.version = 0
        colr.ColorLayers = {}
        for glyph in font.getGlyphOrder():
            if glyph != ".notdef":
                layer = LayerRecord()
                layer.name = glyph
                layer.colorID = 0
                colr.ColorLayers[glyph] = [layer]
        font["COLR"] = colr

        font.save(output_path)
        print(f"Processed: {output_path}")

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


def process_directory(target_dir, rgb_color, bootstrapper=None, mod_name=None):
    if not os.path.isdir(target_dir):
        print(f"Invalid directory: {target_dir}")
        sys.exit(1)

    count = 0
    for root, _, files in os.walk(target_dir):
        for file in files:
            if file.lower().endswith(SUPPORTED_EXTENSIONS):
                recolor_font(
                    os.path.join(root, file), 
                    rgb_color, 
                    bootstrapper=bootstrapper, 
                    mod_name=mod_name
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
    parser.add_argument("--color", required=True)
    parser.add_argument("--bootstrapper")
    parser.add_argument("--mod-name")
    args = parser.parse_args()

    try:
        color = hex_to_rgb(args.color)
    except ValueError as e:
        print(e)
        sys.exit(1)

    bootstrapper = get_default_bootstrapper_for_platform()
    if args.bootstrapper:
        bootstrapper = canonicalize_bootstrapper(args.bootstrapper)

    process_directory(
        args.path, 
        color, 
        bootstrapper=bootstrapper, 
        mod_name=args.mod_name
    )