The mod generator repository for [Froststrap](https://github.com/RealMeddsam/Froststrap)

### Requirements
 - Python 3.10+
 - [uv](https://docs.astral.sh/uv/)
 
### Project Dependencies
 - fonttools
 
### Usage
you can use this to create font files for a desired colour, note it does not support image generation for the few images that Roblox still annoyingly uses.

To get the most seamless experience, it is recommended to use [Froststrap](https://github.com/RealMeddsam/Froststrap) for zero hassle and extra options such as previewing.

However, you can use this standalone from Froststrap. To run the mod generator you simply download the [release](https://github.com/Froststrap/mod-generator/releases/latest), and run the exe file in the terminal, with the following launch arguments:
 - `path`: path to the font file, normally located in `%localappdata%\Froststrap\Versions\version-version guid\ExtraContent\LuaPackages\Packages\_Index\BuilderIcons\BuilderIcons\Font\`.
 - `color`: colour to use for the font, in hex code format.
 - `bootstrapper`: [OPTIONAL] name of the bootstrapper to use, accepted bootstrappers are `Bloxstrap`, `Fishstrap`, `Froststrap`, `Luczystrap`, or `Lunastrap`. This is used to automatically put the mod into the desired bootstrapper.

### Example Usage
To create a font file for the colour #FF0000 and add mod to the bootstrapper Fishstrap, an example of the command would be:
```
python mod_generator.exe --bootstrapper Fishstrap --path C:\Users\User\AppData\Local\Froststrap\Versions\version-5aed1822f52c4b03\ExtraContent\LuaPackages\Packages\_Index\BuilderIcons\BuilderIcons\Font --color FF0000
```

**Note:** The version GUID can and will change as time goes on, so manually check whats the current version GUID for you.


### Building from source
TO build form source, you simply clone this repository via:
```
git clone https://github.com/Froststrap/mod-generator.git
```

then install dependencies via:
```
uv pip install -r requirements.txt
```

you can then run the file:
```
python src/main.py [ARGUMENTS]
```

To build the project into an executable, run:
```
uv run pyinstaller --onefile --name mod_generator src/main.py
```

with the generated exe file being in the `dist/` folder.
