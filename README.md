The mod generator repository for [Froststrap](https://github.com/Froststrap/Froststrap)

>[!NOTE]
> Now with gradient support! (as 2.0)


### Requirements
 - Rust 2024
 
### Usage
The mod generator builds a complete colour mod: it downloads the current Roblox assets, recolours the UI textures, sprite sheets and BuilderIcons fonts, and writes a mod folder (and optionally a zip) ready to drop into your bootstrapper's modifications folder.

To get the most seamless experience, it is recommended to use [Froststrap](https://github.com/Froststrap/Froststrap) for zero hassle and extra options such as previewing. Froststrap links this crate directly as a library.

You can also use it standalone by [building it from source](#building-from-source) and running it in a terminal. Prebuilt releases are paused for now; the existing releases are the old Python version, which older Froststrap builds still depend on.

#### `generate`: build a complete mod
 - `--color`: a hex colour (`FF0000`), a list of hex colours spread evenly as a gradient (`FF0000,0000FF`), or explicit stops (`0:FF0000,0.5:00FF00,1:0000FF`).
 - `--angle`: gradient angle in degrees.
 - `--bands`: number of gradient bands in the fonts; more bands is smoother but slower.
 - `--output`: directory to write the mod to (default `mod`).
 - `--zip`: [OPTIONAL] also write the mod as a zip to this path.
 - `--cursors`, `--shiftlock`, `--emote-wheel`: [OPTIONAL] also recolour the cursors, shift-lock cursor and emote wheel.
 - `--logo`, `--spinner`: [OPTIONAL] images to replace the Roblox logo and loading spinner with.
 - `--image-map`: [OPTIONAL] comma-separated map of glyph to image, e.g. `tilt:C:/img.png,another:img2.png`.
 - `--skip-glyphs`: [OPTIONAL] comma-separated glyphs to leave uncoloured, e.g. `play-large,play-small`.
 - `--mappings`: [OPTIONAL] JSON file of textures to recolour, replacing the built-in set.
 - `--cache-dir`: [OPTIONAL] where downloaded packages and generated fonts are cached (defaults to your platform's cache directory).

```
mod-generator generate --color 0:FF0000,1:0000FF --angle 90 --cursors --zip MyMod.zip
```

#### `fonts`: recolour BuilderIcons fonts only
 - `--path`: directory containing the fonts, normally `ExtraContent\LuaPackages\Packages\_Index\BuilderIcons\BuilderIcons\Font\` inside a Roblox version folder.
 - `--color`, `--angle`, `--bands`, `--image-map`, `--skip-glyphs`: as above.
 - `--bootstrapper`: [OPTIONAL] `Bloxstrap`, `Fishstrap`, `Froststrap`, `Luczystrap`, `Lunastrap` or `Sober`, to install the fonts into that bootstrapper's mods.
 - `--skip-color-matching`: [OPTIONAL] don't colour-match glyphs to the images given in `--image-map`.
 - `--max-colors`: [OPTIONAL] maximum number of colours to take from images given in `--image-map`.

```
mod-generator fonts --bootstrapper Fishstrap --path "C:\Users\User\AppData\Local\Froststrap\Versions\version-5aed1822f52c4b03\ExtraContent\LuaPackages\Packages\_Index\BuilderIcons\BuilderIcons\Font" --color FF0000
```

### Building from source
```
git clone https://github.com/Froststrap/mod-generator.git
cd mod-generator
cargo build --release
```

The binary is written to `target/release/mod-generator`.

To build only the library with its C ABI (as Froststrap does), run:
```
cargo build --release --lib --no-default-features --features ffi
```
