#!/usr/bin/env python3
"""Headless Chromium test of the browser build (docs/testing.md "Browser").

Opens a served web build (`cargo xtask serve --game-dir ...`), stores the settings in
`localStorage`, drives the game through the page hook `acr_drive` (the driver's scene
requests), zooms the camera into one unit and writes, into --out:

- `console.jsonl`: every console message and page error;
- `answers.jsonl`: the hook's answers;
- `frames.json`: per zoom step the page's frame intervals (requestAnimationFrame) and the
  game's own frame work (`acr_frame_ms`), plus the `textures` answers;
- `z*.png`: screenshots of the page at each zoom step;
- `summary.json`: the numbers and the WebGL renderer.

Needs Python Playwright (`pip install playwright && playwright install chromium`).
"""

import argparse
import base64
import json
import pathlib
import statistics
import sys
import time

from playwright.sync_api import sync_playwright

SETTINGS = """controls=modern
models=all
camera=free
gore=full
shadows=on
lod=on
material_detail=on
water=detailed
ground=detailed
ground_cover=on
sound_volume=0
music_volume=0
"""

HOOKS = """
window.__acrFrames = [];
(function tick(last) {
  requestAnimationFrame((t) => {
    if (last !== undefined) window.__acrFrames.push(t - last);
    tick(t);
  });
})();
"""


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--url", default="http://127.0.0.1:8090/?game=/game/")
    ap.add_argument("--out", required=True)
    ap.add_argument("--unit", default="EngM18")
    ap.add_argument("--size", default="1600x900")
    ap.add_argument("--zooms", default="1,2,3,4,5,6,7,8,9")
    ap.add_argument("--step-s", type=float, default=2.0, help="seconds per zoom step")
    ap.add_argument("--hold-s", type=float, default=8.0, help="seconds at the last zoom")
    ap.add_argument("--gpu", choices=["vulkan", "swiftshader", "gl"], default="vulkan")
    ap.add_argument("--timeout-s", type=float, default=180.0)
    a = ap.parse_args()
    out = pathlib.Path(a.out)
    out.mkdir(parents=True, exist_ok=True)
    w, h = (int(v) for v in a.size.split("x"))
    console = open(out / "console.jsonl", "w")
    answers_f = open(out / "answers.jsonl", "w")
    errors = []

    def log_console(kind, text):
        console.write(json.dumps({"t": round(time.time(), 3), "type": kind, "text": text}) + "\n")
        console.flush()
        if kind in ("error", "pageerror"):
            errors.append(text)

    args = ["--ignore-gpu-blocklist", "--enable-gpu-rasterization", "--disable-features=Vulkan" if a.gpu == "gl" else "",
            {"vulkan": "--use-angle=vulkan", "swiftshader": "--use-angle=swiftshader", "gl": "--use-angle=gl"}[a.gpu],
            "--enable-unsafe-swiftshader" if a.gpu == "swiftshader" else "--enable-features=Vulkan"]
    with sync_playwright() as p:
        browser = p.chromium.launch(headless=True, args=[x for x in args if x])
        page = browser.new_page(viewport={"width": w, "height": h})
        page.on("console", lambda m: log_console(m.type, m.text))
        page.on("pageerror", lambda e: log_console("pageerror", str(e)))
        page.add_init_script(
            "localStorage.setItem('acr-settings', %s);" % json.dumps(base64.b64encode(SETTINGS.encode()).decode()) + HOOKS
        )
        page.goto(a.url)
        page.wait_for_function("() => document.querySelector('canvas') && !document.getElementById('picker')", timeout=a.timeout_s * 1000)
        renderer = page.evaluate("""() => {
            const gl = document.createElement('canvas').getContext('webgl2');
            if (!gl) return 'no webgl2';
            const e = gl.getExtension('WEBGL_debug_renderer_info');
            return e ? gl.getParameter(e.UNMASKED_RENDERER_WEBGL) : gl.getParameter(gl.RENDERER);
        }""")
        page.evaluate("() => window.__acr = import('./acr_game.js')")

        def drive(req, timeout=a.timeout_s):
            line = json.dumps(req)
            page.evaluate("async (l) => (await window.__acr).acr_drive(l)", line)
            end = time.time() + timeout
            while time.time() < end:
                got = page.evaluate("async () => (await window.__acr).acr_answers()")
                if got:
                    for g in got:
                        answers_f.write(g + "\n")
                    answers_f.flush()
                    ans = json.loads(got[-1])
                    if not ans.get("ok"):
                        log_console("driver", line + " -> " + got[-1])
                    return ans
                time.sleep(0.05)
            log_console("driver", "timeout: " + line)
            raise SystemExit("hook timeout on " + line)

        def sample(seconds):
            page.evaluate("async () => { window.__acrFrames = []; (await window.__acr).acr_frame_ms(); }")
            time.sleep(seconds)
            raf = page.evaluate("() => window.__acrFrames.splice(0)")
            work = list(page.evaluate("async () => Array.from((await window.__acr).acr_frame_ms())"))
            return raf, work

        t0 = time.time()
        start = drive({"cmd": "start", "blank": {"ground": "grass", "hud": False, "players": 1}})
        cx, cy = start.get("centre", [7680, 7680])
        drive({"cmd": "spawn", "type": a.unit, "x": cx, "y": cy, "dir": 64, "owner": 0})
        drive({"cmd": "camera", "x": cx, "y": cy, "zoom": 1})
        time.sleep(3.0)
        load_s = time.time() - t0
        steps = []
        zooms = [float(z) for z in a.zooms.split(",")]
        for i, z in enumerate(zooms):
            drive({"cmd": "camera", "x": cx, "y": cy, "zoom": z, "yaw_deg": 20, "pitch_deg": 12, "lift": 40, "free": True})
            raf, work = sample(a.step_s + (a.hold_s if i == len(zooms) - 1 else 0))
            tex = drive({"cmd": "textures"}).get("textures")
            name = "z%g.png" % z
            page.screenshot(path=str(out / name))
            steps.append({
                "zoom": z, "png": name, "textures": tex,
                "raf_frames": len(raf), "raf_mean_ms": round(statistics.mean(raf), 2) if raf else None,
                "raf_max_ms": round(max(raf), 2) if raf else None,
                "work_mean_ms": round(statistics.mean(work), 2) if work else None,
                "work_max_ms": round(max(work), 2) if work else None,
                "raf": [round(v, 2) for v in raf], "work": [round(v, 2) for v in work],
            })
        browser.close()
    raf_all = [v for s in steps for v in s["raf"]]
    work_all = [v for s in steps for v in s["work"]]
    summary = {
        "url": a.url, "unit": a.unit, "size": a.size, "gpu": a.gpu, "renderer": renderer,
        "load_s": round(load_s, 1), "console_errors": len(errors), "first_errors": errors[:10],
        "raf_max_ms": max(raf_all) if raf_all else None,
        "raf_mean_ms": round(statistics.mean(raf_all), 2) if raf_all else None,
        "work_max_ms": max(work_all) if work_all else None,
        "work_mean_ms": round(statistics.mean(work_all), 2) if work_all else None,
        "frames_over_33ms": sum(1 for v in raf_all if v > 33.3),
        "steps": [{k: v for k, v in s.items() if k not in ("raf", "work")} for s in steps],
    }
    (out / "frames.json").write_text(json.dumps(steps, indent=1))
    (out / "summary.json").write_text(json.dumps(summary, indent=1))
    print(json.dumps({k: v for k, v in summary.items() if k != "steps"}))
    for s in summary["steps"]:
        print(json.dumps(s))
    return 0


if __name__ == "__main__":
    sys.exit(main())
