"""Assemble already-rendered PNGs into a multi-resolution Windows ICO."""
import sys
from pathlib import Path

from PIL import Image

out = Path(sys.argv[1])
sizes = [16, 24, 32, 48, 64, 128, 256]
images = [Image.open(out / f"icon-{size}.png").convert("RGBA") for size in sizes]
images[-1].save(
    out / "icon.ico",
    format="ICO",
    sizes=[(size, size) for size in sizes],
    append_images=images[:-1],
)
