# Froststrap
# Copyright (c) Froststrap Team
#
# This file is part of Froststrap and is distributed under the terms of the
# GNU Affero General Public License, version 3 or later.
#
# SPDX-License-Identifier: AGPL-3.0-or-later
#
# Description: Tool to batch-convert TTF fonts to single-color COLR OTF and
# optionally copy them / write BuilderIcons.json for various bootstrappers.

import argparse
import os
import shutil
import sys
from pathlib import Path

from fontTools.ttLib import TTFont
from fontTools.ttLib.tables.C_O_L_R_ import LayerRecord, table_C_O_L_R_
from fontTools.ttLib.tables.C_P_A_L_ import Color, table_C_P_A_L_

BOOTSTRAPPERS = [
    "Bloxstrap",
    "Fishstrap",
    "Froststrap",
    "Luczystrap",
    "Lunastrap",
]


def hex_to_rgb(hex_str):
    """Converts a hex string to an (r, g, b) tuple."""
    hex_str = hex_str.strip().lstrip("#")

    if len(hex_str) != 6:
        raise ValueError("Hex color must be 6 characters long (e.g. FF0000).")

    try:
        r = int(hex_str[0:2], 16)
        g = int(hex_str[2:4], 16)
        b = int(hex_str[4:6], 16)
        return (r, g, b)
    except ValueError:
        raise ValueError("Invalid hex characters provided.")


def canonicalize_bootstrapper(name):
    """
    Map a user-supplied bootstrapper string to a canonical name from BOOTSTRAPPERS
    """
    if not name:
        return None
    name_lower = name.strip().lower()
    for canonical in BOOTSTRAPPERS:
        if canonical.lower() == name_lower:
            return canonical
    raise ValueError(
        f"Invalid bootstrapper '{name}'. Allowed values: {', '.join(BOOTSTRAPPERS)}"
    )


def convert_ttf_to_colr(file_path, rgb_color, bootstrapper=None):
    """
    Convert a single TTF to a single-color COLR OTF and optionally copy it into
    the bootstrapper's Modifications font directory.

    - path: path to input .ttf
    - color: hex color code
    - bootstrapper: bootstrapper name or None
    """
    r, g, b = rgb_color
    alpha = 255

    try:
        input_path = Path(file_path)
        output_path = input_path.with_suffix(".otf")

        print(f"Processing: {input_path.name} -> COLR ({r},{g},{b})")

        font = TTFont(input_path)
        glyph_order = font.getGlyphOrder()

        # Setup the Palette (CPAL)
        cpal = table_C_P_A_L_()
        cpal.version = 0

        my_color = Color(red=r, green=g, blue=b, alpha=alpha)
        cpal.palettes = [[my_color]]
        cpal.numPaletteEntries = 1
        font["CPAL"] = cpal

        # Setup the Layers (COLR)
        colr = table_C_O_L_R_()
        colr.version = 0
        layer_map = {}

        for glyph_name in glyph_order:
            if glyph_name == ".notdef":
                continue

            layer = LayerRecord()
            layer.name = glyph_name
            layer.colorID = 0

            layer_map[glyph_name] = [layer]

        colr.ColorLayers = layer_map
        font["COLR"] = colr

        font.save(output_path)
        print(f" -> Success! Saved to: {output_path}")

        # Copy the generated .otf to the bootstrapper's Modifications font dir if requested
        if bootstrapper:
            local_appdata = os.environ.get("LOCALAPPDATA")
            if not local_appdata:
                print(
                    " -> Warning: LOCALAPPDATA not found; cannot copy to bootstrapper path."
                )
            else:
                dest_dir = os.path.join(
                    local_appdata,
                    bootstrapper,
                    "Modifications",
                    "ExtraContent",
                    "LuaPackages",
                    "Packages",
                    "_Index",
                    "BuilderIcons",
                    "BuilderIcons",
                    "Font",
                )
                try:
                    os.makedirs(dest_dir, exist_ok=True)
                    dest_path = os.path.join(dest_dir, output_path.name)
                    shutil.copy2(str(output_path), dest_path)
                    print(f" -> Copied to bootstrapper path: {dest_path}")
                except Exception as e:
                    print(f" -> Error copying to bootstrapper path: {e}")

    except Exception as e:
        print(f" -> Error processing {file_path}: {e}")


def _derive_buildericons_dir_from_path(target_dir):
    """
    Try to derive the BuilderIcons/BuilderIcons directory from a given --path.
    Returns a string path or None if not found.
    """
    p = Path(target_dir)

    if p.name.lower() == "font" and p.parent.name.lower() == "buildericons":
        return str(p.parent)

    if p.name.lower() == "buildericons":
        if p.parent and p.parent.name.lower() == "buildericons":
            return str(p)
        return str(p)

    marker_parts = [
        "ExtraContent",
        "LuaPackages",
        "Packages",
        "_Index",
        "BuilderIcons",
        "BuilderIcons",
    ]

    try:
        ancestors = [p] + list(p.parents)
        for anc in ancestors:
            parts = list(anc.parts)
            if len(parts) >= len(marker_parts):
                tail = parts[-len(marker_parts) :]
                if [t.lower() for t in tail] == [m.lower() for m in marker_parts]:
                    return str(anc)
    except Exception:
        pass

    return None


def _write_buildericons_json(json_dir):
    """Create BuilderIcons.json with the required content in json_dir."""
    try:
        os.makedirs(json_dir, exist_ok=True)
        json_path = os.path.join(json_dir, "BuilderIcons.json")
        json_content = """{
            "name": "Builder Icons",
            "loadStrategy": "sameFamilyOnly",
            "faces": [
                {
                "name": "Regular",
                "weight": 400,
                "style": "normal",
                "assetId": "rbxasset://LuaPackages/Packages/_Index/BuilderIcons/BuilderIcons/Font/BuilderIcons-Regular.otf"
                },
                {
                "name": "Bold",
                "weight": 700,
                "style": "normal",
                "assetId": "rbxasset://LuaPackages/Packages/_Index/BuilderIcons/BuilderIcons/Font/BuilderIcons-Filled.otf"
                }
            ]
            }"""
        with open(json_path, "w", encoding="utf-8") as jf:
            jf.write(json_content)
        print(f" -> Wrote BuilderIcons.json to: {json_path}")
    except Exception as e:
        print(f" -> Error writing BuilderIcons.json to {json_dir}: {e}")


def process_directory(target_dir, rgb_color, bootstrapper=None):
    if not os.path.isdir(target_dir):
        print(f"Error: The directory '{target_dir}' does not exist.")
        sys.exit(1)

    print(f"Scanning directory: {target_dir}")
    print(f"Applying Color: RGB{rgb_color}")
    if bootstrapper:
        print(f"Bootstrapper target: {bootstrapper}")
    print("-" * 40)

    count = 0
    for root, dirs, files in os.walk(target_dir):
        for file in files:
            if file.lower().endswith(".ttf"):
                full_path = os.path.join(root, file)
                convert_ttf_to_colr(full_path, rgb_color, bootstrapper=bootstrapper)
                count += 1

    if count == 0:
        print("No .ttf files found in this directory.")
    else:
        print(f"\nBatch processing complete. Processed {count} files.")

    if bootstrapper:
        local_appdata = os.environ.get("LOCALAPPDATA")
        if not local_appdata:
            print(
                " -> Warning: LOCALAPPDATA not found; cannot write BuilderIcons.json to bootstrapper location."
            )
        else:
            json_dir = os.path.join(
                local_appdata,
                bootstrapper,
                "Modifications",
                "ExtraContent",
                "LuaPackages",
                "Packages",
                "_Index",
                "BuilderIcons",
                "BuilderIcons",
            )
            _write_buildericons_json(json_dir)
    else:
        derived = _derive_buildericons_dir_from_path(target_dir)
        if derived:
            _write_buildericons_json(derived)
        else:
            print(
                " -> Warning: could not determine BuilderIcons directory from --path; skipping BuilderIcons.json creation."
            )


if __name__ == "__main__":
    parser = argparse.ArgumentParser(
        description="Batch convert TTF fonts to Solid Color OTF (COLR v0).",
        formatter_class=argparse.RawTextHelpFormatter,
    )

    parser.add_argument(
        "--path", required=True, help="Path to the directory containing .ttf files."
    )

    parser.add_argument(
        "--color",
        required=True,
        help="Hex color code (e.g. #00008B).",
    )

    parser.add_argument(
        "--bootstrapper",
        required=False,
        help=(
            "Optional bootstrapper name to copy font files to (case-insensitive).\n"
            "Accepted values:\n"
            "  " + "\n  ".join(BOOTSTRAPPERS)
        ),
    )

    args = parser.parse_args()

    final_color = None

    if args.color:
        try:
            final_color = hex_to_rgb(args.color)
        except ValueError as e:
            print(f"Argument Error: {e}")
            sys.exit(1)

    canonical_bootstrapper = None
    if getattr(args, "bootstrapper", None):
        try:
            canonical_bootstrapper = canonicalize_bootstrapper(args.bootstrapper)
            print(f"Selected bootstrapper: {canonical_bootstrapper}")
        except ValueError as e:
            print(f"Argument Error: {e}")
            sys.exit(1)

    process_directory(args.path, final_color, bootstrapper=canonical_bootstrapper)
