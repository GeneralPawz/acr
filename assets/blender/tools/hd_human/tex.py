"""4k skin maps for the HD human prototype (python3 + numpy + PIL, local-only outputs).

usage: python3 -I tex.py <mh_diffuse_2k.png> <out_dir>
- albedo_4k.png: the MakeHuman CC0 young male diffuse, Lanczos to 4k, with faint mottling.
- normal_4k.png: tangent-space micro detail (pores, fine creases) from seeded noise.
- rough_4k.png: roughness from the pore height (pores rougher, ridges a bit glossier).
"""
import sys
import numpy as np
from PIL import Image

src, out = sys.argv[1], sys.argv[2]
N = 4096
rng = np.random.default_rng(306)


def blur(a, sigma):
    """Periodic gaussian blur by FFT (tileable)."""
    fy = np.fft.fftfreq(a.shape[0])[:, None]
    fx = np.fft.fftfreq(a.shape[1])[None, :]
    g = np.exp(-2 * (np.pi * sigma) ** 2 * (fx ** 2 + fy ** 2))
    return np.real(np.fft.ifft2(np.fft.fft2(a) * g))


def norm(a):
    a = a - a.mean()
    return a / (a.std() + 1e-9)


# Pores: sparse dark pits, plus fine skin grain and a weak crease field.
pits = np.zeros((N, N), np.float32)
idx = rng.integers(0, N, size=(2, 900_000))
pits[idx[0], idx[1]] = 1.0
pits = norm(blur(pits, 1.6))
grain = norm(blur(rng.standard_normal((N, N)), 1.0))
crease = norm(blur(rng.standard_normal((N, N)), 6.0))
crease = -np.abs(crease)  # thin valleys
h = -0.55 * pits + 0.35 * grain + 0.25 * norm(crease)
h = h.astype(np.float32)

strength = 1.6
dx = (np.roll(h, -1, 1) - np.roll(h, 1, 1)) * 0.5 * strength
dy = (np.roll(h, -1, 0) - np.roll(h, 1, 0)) * 0.5 * strength
nz = np.ones_like(h)
n = np.stack([-dx, dy, nz], -1)
n /= np.linalg.norm(n, axis=-1, keepdims=True)
Image.fromarray(((n * 0.5 + 0.5) * 255).clip(0, 255).astype(np.uint8)).save(f"{out}/normal_4k.png")

r = 0.52 - 0.06 * np.tanh(h * 0.8)
Image.fromarray((r * 255).clip(0, 255).astype(np.uint8)).save(f"{out}/rough_4k.png")

a = np.asarray(Image.open(src).convert("RGB").resize((N, N), Image.LANCZOS), np.float32) / 255.0
mott = norm(blur(rng.standard_normal((N, N)), 24.0))[..., None]
tone = np.array([1.0, 0.97, 0.96])
a = a * (1.0 + 0.035 * mott * tone) * (1.0 - 0.04 * np.clip(-h, 0, 3)[..., None] / 3)
Image.fromarray((a * 255).clip(0, 255).astype(np.uint8)).save(f"{out}/albedo_4k.png")
print("maps", N, "h std", float(h.std()))
