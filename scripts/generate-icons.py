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
