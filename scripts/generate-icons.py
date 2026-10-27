from pathlib import Path
from io import BytesIO
import resvg_py
from PIL import Image

# uv run --with resvg-py --with pillow python scripts/generate-icons.py
root = Path(__file__).resolve().parent.parent
asset = root / 'assets' / 'hayai'
png = resvg_py.svg_to_bytes(svg_path=str(asset.with_suffix('.svg')), width=1024, height=1024)
image = Image.open(BytesIO(png)).convert('RGBA')
image.save(asset.with_suffix('.ico'), sizes=[(n, n) for n in (16, 24, 32, 48, 64, 128, 256)])

# Fixed UI artwork is baked ahead of time so opening the launcher never waits
# for SVG rasterization. 128px also covers 56px detail cards at 200% DPI.
for source, destination in (
    (root / 'assets' / 'hayai.svg', root / 'assets' / 'hayai.png'),
    (root / 'assets' / 'icons' / 'update.svg', root / 'assets' / 'icons' / 'update.png'),
    (root / 'assets' / 'icons' / 'thirdparty' / 'scry' / 'scry.svg', root / 'assets' / 'icons' / 'thirdparty' / 'scry' / 'scry.png'),
    (root / 'assets' / 'icons' / 'settings.svg', root / 'assets' / 'icons' / 'settings.png'),
):
    destination.write_bytes(resvg_py.svg_to_bytes(svg_path=str(source), width=128, height=128))
