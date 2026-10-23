"""Generative-3D harvesting helpers: reference images with Codex, models with the Meshy API.

    acr-genai ref <out.png> "<description>" [--style IMG]
        A photorealistic reference picture on a white background via `codex exec` (its image
        tool). --style attaches an image whose design to follow (e.g. the game's cover art).
    acr-genai meshy <image.png> <out.glb> [--poly 25000] [--tex 2k]
        Meshy image-to-3D (PBR, remeshed triangles), polls, downloads the glb and writes
        <out>.json (seconds, credits, task id). The key is read from ~/.config/meshy/api_key and
        never printed.
    acr-genai meshy-balance
        Remaining Meshy credits.

Outputs belong under C:/Users/verwalter/WIP/ACR-analysis/genai/<source>/ (never in the repo).
Rodin has no API: the acr:asset-harvester agent drives it in the browser (skill acr-genai).
"""

import argparse
import base64
import json
import mimetypes
import pathlib
import subprocess
import sys
import time

import requests

MESHY = "https://api.meshy.ai/openapi"
STYLE_TAIL = ("plain white background, even studio light, photorealistic like a museum or reenactor photo, "
              "whole object visible, three-quarter view, no text.")


def meshy_headers():
    key = (pathlib.Path.home() / ".config" / "meshy" / "api_key").read_text().strip()
    return {"Authorization": "Bearer " + key}


def ref(out, description, style):
    out = pathlib.Path(out).resolve()
    out.parent.mkdir(parents=True, exist_ok=True)
    prompt = (f"Use your image generation tool to create one image and save it as {out.name} in the current "
              f"directory: {description}. {STYLE_TAIL} Reply only with the saved file path.")
    if style:
        prompt = "Follow the design of the attached image. " + prompt
    cmd = ["codex", "exec", "--skip-git-repo-check", "-s", "workspace-write", prompt]
    if style:
        cmd += ["-i", str(pathlib.Path(style).resolve())]  # after the prompt: -i takes several files
    r = subprocess.run(cmd, cwd=out.parent, capture_output=True, text=True, timeout=900)
    if not out.exists():
        sys.exit(f"no image written; codex said: {r.stdout[-400:]} {r.stderr[-400:]}")
    print(out)


def meshy(image, out, poly, tex):
    h = meshy_headers()
    img = pathlib.Path(image)
    mime = mimetypes.guess_type(img.name)[0] or "image/png"
    uri = f"data:{mime};base64," + base64.b64encode(img.read_bytes()).decode()
    body = {"image_url": uri, "ai_model": "latest", "should_remesh": True, "topology": "triangle",
            "target_polycount": poly, "enable_pbr": True, "should_texture": True, "texture_resolution": tex}
    t0 = time.time()
    r = requests.post(f"{MESHY}/v1/image-to-3d", headers=h, json=body, timeout=120)
    r.raise_for_status()
    tid = r.json()["result"]
    while True:
        try:
            t = requests.get(f"{MESHY}/v1/image-to-3d/{tid}", headers=h, timeout=30).json()
        except requests.RequestException:
            time.sleep(5)
            continue
        if t.get("status") in ("SUCCEEDED", "FAILED", "CANCELED"):
            break
        time.sleep(10)
    if t["status"] != "SUCCEEDED":
        sys.exit(f"meshy task {tid}: {t.get('status')} {t.get('task_error')}")
    out = pathlib.Path(out)
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_bytes(requests.get(t["model_urls"]["glb"], timeout=300).content)
    meta = {"service": "meshy", "image": str(img), "task_id": tid, "seconds": round(time.time() - t0),
            "credits": t.get("consumed_credits"), "poly": poly, "texture": tex}
    out.with_suffix(".json").write_text(json.dumps(meta, indent=1))
    print(json.dumps(meta))


def balance():
    r = requests.get(f"{MESHY}/v1/balance", headers=meshy_headers(), timeout=30)
    print(r.json())


def main():
    ap = argparse.ArgumentParser()
    sub = ap.add_subparsers(dest="cmd", required=True)
    a = sub.add_parser("ref")
    a.add_argument("out")
    a.add_argument("description")
    a.add_argument("--style", default="")
    m = sub.add_parser("meshy")
    m.add_argument("image")
    m.add_argument("out")
    m.add_argument("--poly", type=int, default=25000)
    m.add_argument("--tex", default="2k")
    sub.add_parser("meshy-balance")
    x = ap.parse_args()
    if x.cmd == "ref":
        ref(x.out, x.description, x.style)
    elif x.cmd == "meshy":
        meshy(x.image, x.out, x.poly, x.tex)
    else:
        balance()


if __name__ == "__main__":
    main()
