The mod generator repository for [Froststrap](https://github.com/RealMeddsam/Froststrap)

>[!NOTE]
> Now with gradient support! (as of mod generator 2.0)


### Requirements
 - Python 3.10+
 - [uv](https://docs.astral.sh/uv/)
 
### Project Dependencies
 - fonttools
 - numpy
 - pyclipper
 - pyinstaller
 - pillow
 
### Usage
you can use this to create font files for a desired colour, note it does not support image generation for the few images that Roblox still annoyingly uses.

To get the most seamless experience, it is recommended to use [Froststrap](https://github.com/RealMeddsam/Froststrap) for zero hassle and extra options such as previewing.

However, you can use this standalone from Froststrap. To run the mod generator you simply download the [release](https://github.com/Froststrap/mod-generator/releases/latest), and run the exe file in the terminal, with the following launch arguments:
 - `path`: path to the font file, normally located in `%localappdata%\Froststrap\Versions\version-version guid\ExtraContent\LuaPackages\Packages\_Index\BuilderIcons\BuilderIcons\Font\`.
 - `colors`: colors to use for the font, in hex code format. With gradient support, you list all the hex codes in order from ascending to descending order to be displayed on the glyph.
 - `angle`: angle to rotate the gradient, in degrees.
 - `bands`: number of bands to use for the gradient, higher amount of bands means higher quality but longer time to generate.
 - `image-map`: [OPTIONAL] comma-separated map of glyph to image, to match the glyph to the assigned image. Example: `uniF200:C:/img.png,another:img2.png`
 - `bootstrapper`: [OPTIONAL] name of the bootstrapper to use, accepted bootstrappers are `Bloxstrap`, `Fishstrap`, `Froststrap`, `Luczystrap`, or `Lunastrap`. This is used to automatically put the mod into the desired bootstrapper.
 - `skip-glyphs`: [OPTIONAL] comma-separated list of names of the glyphs you would like to skip or don't color. Example: `uniE001,uniE002,uniF123`
 - `skip-color-matching`: [OPTIONAL] Disable trying to color match the glyphs with images given in `--image-map`.
 - `max-colors`: [OPTIONAL] Maximum number of colors to inference from images given in `--image-map`.

### Example Usage
To create a font file for the colour #FF0000 and add mod to the bootstrapper Fishstrap, an example of the command would be:
```
mod_generator.exe --bootstrapper Fishstrap --path C:\Users\User\AppData\Local\Froststrap\Versions\version-5aed1822f52c4b03\ExtraContent\LuaPackages\Packages\_Index\BuilderIcons\BuilderIcons\Font --color FF0000
```

**Note:** The version GUID can and will change as time goes on, so manually check whats the current version GUID for you.


### Building from source
To build from source, you simply clone this repository via:
```
git clone https://github.com/Froststrap/mod-generator.git
```

then install dependencies via:
```
uv sync
```

you can then run the file:
```
uv run src/main.py [ARGUMENTS]
```

To build the project into an executable, run:
```
uv run pyinstaller --onefile --name mod_generator src/main.py
```

with the generated exe file being in the `dist/` folder.
