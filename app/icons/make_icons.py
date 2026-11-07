"""Generuje ikonę BurnCraft (własny projekt: wiązka lasera nad płytą).

Uruchamianie: python app/icons/make_icons.py — zapisuje icon.png (512 px)
i icon.ico (16–256 px) obok skryptu. Wymaga Pillow.
"""
import pathlib

from PIL import Image, ImageDraw

HERE = pathlib.Path(__file__).parent
S = 512


def draw() -> Image.Image:
    im = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    d = ImageDraw.Draw(im)
    d.rounded_rectangle([16, 16, S - 16, S - 16], radius=96, fill=(30, 33, 38, 255))
    # płyta
    d.rounded_rectangle([96, 352, S - 96, 400], radius=12, fill=(176, 132, 82, 255))
    # głowica lasera
    d.rounded_rectangle([196, 96, 316, 176], radius=18, fill=(200, 205, 214, 255))
    # wiązka
    d.polygon([(244, 176), (268, 176), (262, 352), (250, 352)], fill=(255, 92, 60, 255))
    # punkt cięcia
    d.ellipse([228, 330, 284, 374], fill=(255, 196, 80, 255))
    # wycięty kontur
    d.line([(132, 376), (228, 376)], fill=(40, 30, 20, 255), width=8)
    return im


def main():
    im = draw()
    im.save(HERE / "icon.png")
    im.save(HERE / "icon.ico", sizes=[(16, 16), (24, 24), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)])
    print("icon.png, icon.ico")


if __name__ == "__main__":
    main()
