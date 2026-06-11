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

import os
import shutil
import sys
from pathlib import Path


def is_linux():
    return sys.platform.startswith("linux")


def is_windows():
    return sys.platform.startswith("win")


def get_default_bootstrapper_for_platform():
    if is_linux():
        return "Sober"
    return None


def _get_font_dir(bootstrapper, mod_name=None):
    if is_linux() and bootstrapper == "Sober":
        return (
            Path.home()
            / ".var"
            / "app"
            / "org.vinegarhq.Sober"
            / "data"
            / "sober"
            / "asset_overlay"
            / "ExtraContent"
            / "LuaPackages"
            / "Packages"
            / "_Index"
            / "BuilderIcons"
            / "BuilderIcons"
            / "Font"
        )

    if is_windows():
        local_appdata = os.environ.get("LOCALAPPDATA")
        if not local_appdata:
            return None

        base_path = Path(local_appdata) / bootstrapper / "Modifications"
        
        if bootstrapper.lower() == "froststrap" and mod_name:
            base_path = base_path / mod_name
            
        return (
            base_path
            / "ExtraContent"
            / "LuaPackages"
            / "Packages"
            / "_Index"
            / "BuilderIcons"
            / "BuilderIcons"
            / "Font"
        )
    return None


def _get_root_dir(bootstrapper, mod_name=None):
    font_dir = _get_font_dir(bootstrapper, mod_name=mod_name)
    if not font_dir:
        return None
    return font_dir.parent


def copy_font_to_bootstrapper(bootstrapper, output_font_path, mod_name=None):
    dest_dir = _get_font_dir(bootstrapper, mod_name=mod_name)
    if not dest_dir:
        return False
    dest_dir.mkdir(parents=True, exist_ok=True)
    shutil.copy2(output_font_path, dest_dir / output_font_path.name)
    return True


def write_buildericons_json(bootstrapper, mod_name=None):
    font_dir = _get_font_dir(bootstrapper, mod_name=mod_name)
    if not font_dir:
        return False
    root = font_dir.parent
    (root / "BuilderIcons.json").write_text(
        """{
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
        }""",
        encoding="utf-8",
    )
    return True