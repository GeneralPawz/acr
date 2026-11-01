#!/usr/bin/env python3
"""Parity of the Svelte UI against the engine's screens (docs/webui.md "Parity", tracking issue #316).

Opens a served web build (`cargo xtask serve --game-dir ...`, with the bundle: `cargo xtask ui`)
twice per screen, `?ui=engine` and `?ui=svelte`, at 1024x768 and device pixel ratio 1, brings the
screen up, and writes into `runs/ui-parity/<screen>/` (or --out):

- `engine.png`, `svelte.png`, `diff.png`: the two screenshots and `cargo xtask ui-diff`'s picture;
- `report.json`: `pixels` (percent of the pixels within +-8 per channel), `layout` (the largest
  difference in pixels between a `[data-acr-id]` element and the `ui` tree's rect of the same id),
  the ids compared and the verdict.

Passes when pixels >= --min-pixels (99.5) and layout <= --max-layout (1). Exit status 1 otherwise.
A screen joins `ui/src/screens/registry.ts` only after this passes for it.

Needs Python Playwright (`pip install playwright && playwright install chromium`). In headless
Chromium without a GPU the game needs WebGPU on SwiftShader (the default of --gpu).
"""

import argparse
import base64
import json
import pathlib
import re
import subprocess
import sys
import time

from playwright.sync_api import sync_playwright

SETTINGS = """controls=modern
sound_volume=0
music_volume=0
"""

# How to bring a screen up from the main menu: labels to click, in order. The screens the UI
# claims later add their routes with their issues.
ROUTES = {
    "MainMenu": [],
    "PlayerName": ["Einzelspieler", "Zufallskarte"],
    "Options": ["Optionen"],
    "Campaigns": ["Einzelspieler", "Kampagne", "OK"],
    "SingleMission": ["Einzelspieler", "Einzelmission", "OK"],
    "Placeholder": ["Mehrspieler", "Internetspiel"],
    "CampaignMissions": ["Einzelspieler", "Kampagne", "OK", "Mission spielen!"],
}

# Routes whose screen is another route's (`Screen::name`): the campaign's missions are a SingleMission.
SCREEN_OF = {"CampaignMissions": "SingleMission"}

CHROMIUM_ARGS = {
    "swiftshader": [
        "--ignore-gpu-blocklist",
        "--use-angle=swiftshader",
        "--enable-unsafe-swiftshader",
        "--enable-unsafe-webgpu",
        "--use-webgpu-adapter=swiftshader",
        "--enable-features=Vulkan,WebGPU",
    ],
    "vulkan": ["--enable-unsafe-webgpu", "--enable-features=Vulkan,WebGPU", "--ignore-gpu-blocklist"],
}

# Each widget's rect (`data-acr-rect`, canvas units, from Rust's view model) as the page places it:
# through the stage's box, in window pixels (device pixel ratio 1). The widget elements themselves
# are boxless: their parts are positioned absolutely inside the stage.
DOM_RECTS = """() => {
    const stage = document.querySelector('[data-acr-screen]');
    if (!stage) return {};
    const box = stage.getBoundingClientRect();
    const scale = box.width / stage.offsetWidth;
    return Object.fromEntries([...document.querySelectorAll('[data-acr-id]')].map((e) => {
        const [x, y, w, h] = e.dataset.acrRect.split(',').map(Number);
        return [e.dataset.acrId, [box.x + x * scale, box.y + y * scale, w * scale, h * scale]];
    }));
}"""


class Page:
    """One browser page on the game, with the driver hook."""

    def __init__(self, browser, url, size, timeout_s):
        self.timeout_s = timeout_s
        self.errors = []
        self.page = browser.new_page(viewport={"width": size[0], "height": size[1]}, device_scale_factor=1)
        self.page.on("console", lambda m: m.type == "error" and self.errors.append(m.text))
        self.page.on("pageerror", lambda e: self.errors.append(str(e)))
        self.page.add_init_script(
            "localStorage.setItem('acr-settings', %s);" % json.dumps(base64.b64encode(SETTINGS.encode()).decode())
        )
        self.page.goto(url)
        self.page.wait_for_function(
            "() => document.querySelector('canvas') && !document.getElementById('picker')",
            timeout=timeout_s * 1000,
        )
        self.page.evaluate("() => window.__acr = import('./acr_game.js')")

    def drive(self, request):
        self.page.evaluate("async (l) => (await window.__acr).acr_drive(l)", json.dumps(request))
        end = time.time() + self.timeout_s
        while time.time() < end:
            got = self.page.evaluate("async () => (await window.__acr).acr_answers()")
            if got:
                answer = json.loads(got[-1])
                if not answer.get("ok"):
                    raise SystemExit("driver %s -> %s" % (json.dumps(request), got[-1]))
                return answer
            time.sleep(0.05)
        raise SystemExit("driver timeout on " + json.dumps(request))

    def tree(self):
        return self.drive({"cmd": "ui", "all": True})["ui"]

    def show(self, screen, route):
        """Brings `screen` up and waits until it is built and the page is quiet."""
        for label in route:
            self.drive({"cmd": "click", "label": label})
            time.sleep(0.5)
        end = time.time() + self.timeout_s
        while time.time() < end:
            tree = self.tree()
            if tree["screen"] == screen and tree["built"] and not tree["busy"]:
                break
            time.sleep(0.2)
        else:
            raise SystemExit("the screen %s did not come up (at %s)" % (screen, tree["screen"]))
        # Out of the way of the screen: the engine draws its cursor, the page the system's.
        self.page.mouse.move(self.page.viewport_size["width"] - 1, self.page.viewport_size["height"] - 1)
        time.sleep(1.0)
        return self.tree()


def shoot(browser, url, ui, screen, route, size, timeout_s, out):
    page = Page(browser, "%s%sui=%s" % (url, "&" if "?" in url else "?", ui), size, timeout_s)
    tree = page.show(screen, route)
    if ui == "svelte":
        page.page.wait_for_selector("[data-acr-id]", state="attached", timeout=timeout_s * 1000)
        page.page.evaluate("async () => { await document.fonts.ready; await new Promise(requestAnimationFrame); }")
        time.sleep(0.5)
    path = out / ("%s.png" % ui)
    page.page.screenshot(path=str(path))
    rects = page.page.evaluate(DOM_RECTS) if ui == "svelte" else {}
    errors = page.errors
    page.page.close()
    return tree, rects, errors


def layout_difference(tree, rects):
    """The largest pixel difference per id the DOM and the tree both have, and the ids only one has."""
    # The tree's rect is the part inside the widget's clip window; widgets outside it (the main
    # menu's other pages) have none and are left out.
    expected = {w["id"]: w["rect"] for w in tree["widgets"] if w.get("visible", True)}
    worst, per_id = 0.0, {}
    for ident, dom in rects.items():
        if ident not in expected:
            continue
        d = max(abs(a - b) for a, b in zip(dom, expected[ident]))
        per_id[ident] = round(d, 2)
        worst = max(worst, d)
    return worst, per_id, sorted(set(rects) - set(expected)), sorted(set(rects) & set(expected))


def pixel_share(a, b, diff, root):
    run = subprocess.run(
        ["cargo", "xtask", "ui-diff", str(a), str(b), "--out", str(diff), "--min", "0"],
        cwd=root, capture_output=True, text=True,
    )
    match = re.search(r"([0-9.]+)% of the pixels", run.stdout)
    if run.returncode != 0 or not match:
        raise SystemExit("ui-diff failed: " + run.stdout + run.stderr)
    return float(match.group(1))


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--url", default="http://127.0.0.1:8090/?game=/game/")
    ap.add_argument("--screens", required=True, help="comma separated screen names (Screen::name), e.g. MainMenu")
    ap.add_argument("--out", default="runs/ui-parity")
    ap.add_argument("--size", default="1024x768")
    ap.add_argument("--gpu", choices=sorted(CHROMIUM_ARGS), default="swiftshader")
    ap.add_argument("--min-pixels", type=float, default=99.5)
    ap.add_argument("--max-layout", type=float, default=1.0)
    ap.add_argument("--timeout-s", type=float, default=240.0)
    a = ap.parse_args()
    root = pathlib.Path(__file__).resolve().parents[2]
    size = [int(v) for v in a.size.split("x")]
    screens = [s for s in a.screens.split(",") if s]
    unknown = [s for s in screens if s not in ROUTES]
    if unknown:
        raise SystemExit("no route to %s; known: %s" % (", ".join(unknown), ", ".join(ROUTES)))

    ok = True
    with sync_playwright() as p:
        browser = p.chromium.launch(headless=True, args=CHROMIUM_ARGS[a.gpu])
        for screen in screens:
            out = pathlib.Path(a.out) / screen
            out.mkdir(parents=True, exist_ok=True)
            tree, _, engine_errors = shoot(browser, a.url, "engine", SCREEN_OF.get(screen, screen), ROUTES[screen], size, a.timeout_s, out)
            _, rects, svelte_errors = shoot(browser, a.url, "svelte", SCREEN_OF.get(screen, screen), ROUTES[screen], size, a.timeout_s, out)
            pixels = pixel_share(out / "engine.png", out / "svelte.png", out / "diff.png", root)
            layout, per_id, extra, compared = layout_difference(tree, rects)
            passed = pixels >= a.min_pixels and layout <= a.max_layout and bool(compared) and not svelte_errors
            ok &= passed
            report = {
                "screen": screen, "size": size, "pixels": pixels, "layout": layout,
                "min_pixels": a.min_pixels, "max_layout": a.max_layout,
                "compared": compared, "layout_by_id": per_id,
                "tree_rects": {w["id"]: w["rect"] for w in tree["widgets"]}, "dom_rects": rects, "dom_ids_without_widget": extra,
                "console_errors": {"engine": engine_errors, "svelte": svelte_errors},
                "passed": passed,
            }
            (out / "report.json").write_text(json.dumps(report, indent=1))
            print("%s: pixels %.3f%%, layout %.2f px over %d widgets: %s" % (
                screen, pixels, layout, len(compared), "ok" if passed else "FAILED"))
        browser.close()
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
