#!/usr/bin/env python3
"""Size and rate of the view patches in a running game (docs/webui.md "The HUD", risk note of #316).

Opens the served web build with `?ui=svelte`, starts the blank grass field and plays phases (idle, the
mouse sweeping across the top bar's hints, across the map, the "M" drop-down opened and hovered), counting the
messages the page receives (`JSON.parse` of each `view` message) and the page's frame intervals
(`requestAnimationFrame`) per phase. Prints one JSON object; `--ui engine` gives the frame intervals
of the engine's own HUD to compare with.

    ~/browser/venv/bin/python tools/browser/ui_patches.py --url 'http://127.0.0.1:8090/?game=/game/'
"""

import argparse
import json
import sys
import time

from playwright.sync_api import sync_playwright

sys.path.insert(0, __file__.rsplit("/", 1)[0])
import ui_parity as u  # noqa: E402

HOOK = """
window.__patches = [];
window.__frames = [];
const parse = JSON.parse;
JSON.parse = function (text, ...rest) {
  const v = parse.call(this, text, ...rest);
  if (typeof text === 'string' && v && v.t === 'view') {
    window.__patches.push({ at: performance.now(), bytes: text.length, topics: Object.keys(v).filter((k) => k !== 't' && k !== 'rev') });
  }
  return v;
};
let last = performance.now();
const tick = (now) => { window.__frames.push(now - last); last = now; requestAnimationFrame(tick); };
requestAnimationFrame(tick);
"""


def phase(page, name, seconds, move):
    page.page.evaluate("() => { window.__patches.length = 0; window.__frames.length = 0; }")
    t0 = time.time()
    n = 0
    while time.time() - t0 < seconds:
        move(n)
        n += 1
        time.sleep(0.03)
    patches = page.page.evaluate("() => window.__patches")
    frames = sorted(page.page.evaluate("() => window.__frames"))
    span = time.time() - t0
    sizes = sorted(p["bytes"] for p in patches)
    hud = [p for p in patches if "hud" in p["topics"]]
    return {
        "phase": name,
        "seconds": round(span, 1),
        "patches": len(patches),
        "patches_per_s": round(len(patches) / span, 1),
        "bytes_per_s": round(sum(sizes) / span),
        "patch_bytes_median": sizes[len(sizes) // 2] if sizes else 0,
        "patch_bytes_max": sizes[-1] if sizes else 0,
        "with_hud": len(hud),
        "frame_ms_median": round(frames[len(frames) // 2], 1) if frames else None,
        "frame_ms_p95": round(frames[int(len(frames) * 0.95)], 1) if frames else None,
        "frame_ms_max": round(frames[-1], 1) if frames else None,
    }


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--url", default="http://127.0.0.1:8090/?game=/game/")
    ap.add_argument("--ui", choices=["svelte", "engine"], default="svelte")
    ap.add_argument("--seconds", type=float, default=6.0)
    ap.add_argument("--timeout-s", type=float, default=240.0)
    a = ap.parse_args()
    with sync_playwright() as p:
        browser = p.chromium.launch(headless=True, args=u.CHROMIUM_ARGS["swiftshader"])
        page = u.Page(browser, "%s%sui=%s" % (a.url, "&" if "?" in a.url else "?", a.ui), [1024, 768], a.timeout_s, init=HOOK)
        page.drive(u.BLANK)
        time.sleep(1.5)
        out = []
        mouse = page.page.mouse
        out.append(phase(page, "idle", a.seconds, lambda n: mouse.move(512, 400)))
        # Along the six resource values: a hint under the mouse, which moves with it.
        out.append(phase(page, "sweep over the top bar", a.seconds, lambda n: mouse.move(330 + (n * 7) % 640, 17)))
        out.append(phase(page, "sweep over the map", a.seconds, lambda n: mouse.move(100 + (n * 9) % 800, 200 + (n * 5) % 300)))
        page.drive({"cmd": "click", "pos": [275, 17]})
        out.append(phase(page, "drop-down open, mouse over its rows", a.seconds, lambda n: mouse.move(276, 60 + (n * 3) % 220)))
        browser.close()
    json.dump(out, sys.stdout, indent=1)
    print()


if __name__ == "__main__":
    main()
