#!/usr/bin/env python3
"""Compose les images du README à partir des captures de la CI.

Usage : python3 ci/readme_shots.py <dossier shots/> <dossier de sortie>

Les captures (1920 × 1080, branche `ci-results`) contiennent tout l'écran ;
la fenêtre est découpée, arrondie, ombrée et posée sur un fond dégradé.
"""
import sys
from pathlib import Path

from PIL import Image, ImageChops, ImageDraw, ImageFilter

RADIUS = 11
TOP, BOTTOM = (238, 235, 255), (214, 228, 255)  # lavande → bleu pâle


def window(path: Path) -> Image.Image:
    """Découpe la fenêtre de l'app (zone non noire sous la barre des menus)."""
    im = Image.open(path).convert("RGB")
    zone = im.crop((0, 30, im.width, im.height - 95))
    bg = Image.new("RGB", zone.size, zone.getpixel((5, 5)))
    box = ImageChops.difference(zone, bg).convert("L").point(lambda v: 255 if v > 12 else 0).getbbox()
    if box is None:
        raise SystemExit(f"fenêtre introuvable dans {path}")
    left, top, right, bottom = box
    win = im.crop((left, top + 30, right, bottom + 30)).convert("RGBA")
    mask = Image.new("L", win.size, 0)
    ImageDraw.Draw(mask).rounded_rectangle((0, 0, win.width - 1, win.height - 1), RADIUS, fill=255)
    win.putalpha(mask)
    return win


def backdrop(size) -> Image.Image:
    w, h = size
    grad = Image.new("RGBA", size)
    draw = ImageDraw.Draw(grad)
    for y in range(h):
        t = y / max(h - 1, 1)
        draw.line([(0, y), (w, y)], fill=tuple(round(a + (b - a) * t) for a, b in zip(TOP, BOTTOM)) + (255,))
    return grad


def place(canvas: Image.Image, win: Image.Image, xy) -> None:
    x, y = xy
    shadow = Image.new("RGBA", canvas.size, (0, 0, 0, 0))
    alpha = win.getchannel("A").point(lambda v: v * 0.35)
    shadow.paste((20, 16, 60, 255), (x, y + 18), alpha)
    shadow = shadow.filter(ImageFilter.GaussianBlur(26))
    canvas.alpha_composite(shadow)
    canvas.alpha_composite(win, (x, y))


def compose(wins, path: Path, offsets, pad=70) -> None:
    w = max(x + win.width for win, (x, _) in zip(wins, offsets)) + pad
    h = max(y + win.height for win, (_, y) in zip(wins, offsets)) + pad
    canvas = backdrop((w, h))
    for win, xy in zip(wins, offsets):
        place(canvas, win, xy)
    canvas.convert("RGB").save(path, optimize=True)
    print(path, canvas.size)


def main() -> None:
    shots, out = Path(sys.argv[1]), Path(sys.argv[2])
    out.mkdir(parents=True, exist_ok=True)
    win = lambda name: window(shots / f"{name}.png")

    # Clair et sombre côte à côte, le sombre légèrement décalé.
    compose([win("light"), win("dark")], out / "hero.png", [(70, 60), (560, 110)])
    compose([win("settings"), win("limits")], out / "reglages.png", [(70, 60), (650, 60)])
    compose(
        [win("lang-ja-light-0"), win("lang-ar-light-0"), win("lang-zh-Hans-dark-0")],
        out / "langues.png",
        [(70, 60), (650, 60), (1230, 60)],
    )
    compose([win("lang-ar-query")], out / "recherche-arabe.png", [(70, 60)])


if __name__ == "__main__":
    main()
