# App icons

`hayai.svg` is the source artwork. The ICO file contains
16, 24, 32, 48, 64, 128, and 256 pixel images for Windows scaling.

Regenerate after editing the SVG:

```powershell
uv run --with resvg-py --with pillow python scripts/generate-icons.py
```

Hayai's build script embeds `hayai.ico` as Windows resource 1. The native tray
loads that shared resource; the installer script also uses this ICO automatically.
Windows builds require the MSVC toolchain and Windows 10/11 SDK resource compiler.

The Python rendering packages are development tools only.

`icons/scry.svg` is Scry Search's app icon, used on the file-search setup screen.
It is copied from the [Scry Search repository](https://github.com/Ariaryy/scry-search)
under the MIT license; see `icons/scry-LICENSE.txt`.

The same command also bakes 128px `hayai.png`, `icons/update.png`, and
`icons/scry.png` from their SVG sources. These embedded PNGs are decoded before
the launcher's first frame; fixed notification artwork no longer needs a runtime
SVG raster job. Detail cards use the 128px sources, while result rows cache
separate DPI-sized renditions displayed at the standard 22 logical pixels.
