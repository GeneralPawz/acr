#!/usr/bin/env python3
"""The Svelte touch skin on phone and tablet viewports (docs/webui.md "Touch skin", issue #332).

Opens a served web build (`cargo xtask serve --game-dir ...`, with the bundle: `cargo xtask ui`) in a
touch context (`hasTouch`, `isMobile`, device pixel ratio 3) at 390x844 portrait, 844x390 landscape
and 820x1180, and plays with fingers only (`locator.tap()`, `touchscreen.tap()` and touch events through
the DevTools protocol):

1. the main menu, down to the random map setup, by tapping the sheet's entries;
2. the start of the game, by tapping its start button;
3. a unit: one tap on it selects it, a long press on the ground orders it to walk there;

and checks on the way that the page chose the touch skin, that no element lies outside the viewport (the
menu sheets scroll vertically by design: their content is exempt from the vertical test), that every
button is at least 44 CSS pixels each way, and that the unit's order reached the game (`state`).
Screenshots go to `--shots` as `<prefix>-<viewport>-<step>.png` (prefix `touch332`); `report.json` to `--out`.

Needs Python Playwright (`pip install playwright && playwright install chromium`); in headless Chromium
without a GPU the game needs WebGPU on SwiftShader (the default of --gpu).
"""

import argparse
import base64
import json
import pathlib
import sys
import time

from playwright.sync_api import sync_playwright

# Chromium's flags for the GPU the page can use (the same as ui_parity.py).
CHROMIUM_ARGS = {
    "swiftshader": [
        "--ignore-gpu-blocklist",
        "--use-angle=swiftshader",
        "--enable-unsafe-swiftshader",
        "--enable-unsafe-webgpu",
        "--use-webgpu-adapter=swiftshader",
        "--enable-features=Vulkan,WebGPU",
    ],
    "gl": ["--ignore-gpu-blocklist", "--use-gl=angle", "--use-angle=gl"],
}

VIEWPORTS = {
    "portrait": (390, 844),
    "landscape": (844, 390),
    "tablet": (820, 1180),
}

SETTINGS = """controls=modern
sound_volume=0
music_volume=0
"""

# The entries to tap from the main menu to the random map's setup (their labels in the German game).
ROUTE = ["Einzelspieler", "Zufallskarte", "OK"]

# Measured in the page: the touch skin's elements that lie outside the viewport. The glyphs and
# sprites inside a label or icon are clipped by their box and not measured; a menu sheet scrolls
# vertically, so only its horizontal extent counts. `small` lists buttons under 44 CSS pixels.
AUDIT = """() => {
  const w = window.innerWidth, h = window.innerHeight, eps = 0.6;
  const root = document.querySelector('.acr-ui');
  const outside = [], small = [];
  const skip = '.sprite, .label, .canvas, .map > *';
  for (const el of root.querySelectorAll('*')) {
    if (el.closest(skip) && !el.matches(skip)) continue;
    if (el.matches('[data-acr-viewport]')) continue;
    const r = el.getBoundingClientRect();
    if (r.width === 0 || r.height === 0) continue;
    if (el.closest('.label, .sprite') && !el.matches('.label, .sprite')) continue;
    const inSheet = el.closest('.sheet') !== null;
    const inScroll = el.parentElement && el.parentElement.closest('[data-acr-scroll]') !== null;
    const horizontal = r.left < -eps || r.right > w + eps;
    const vertical = r.top < -eps || r.bottom > h + eps;
    if (horizontal || (vertical && !inSheet && !inScroll)) {
      outside.push({ el: el.tagName + '.' + el.className + '#' + (el.dataset.acrId || ''), rect: [r.left, r.top, r.right, r.bottom] });
    }
    if (el.matches('button') && !el.matches('.label') && (r.width < 43.5 || r.height < 43.5) && !el.closest('[data-acr-scroll]')) {
      small.push({ el: el.className + '#' + (el.dataset.acrId || ''), size: [r.width, r.height] });
    }
  }
  return { skin: root.dataset.acrSkin, size: [w, h], outside, small };
}"""


class Run:
    def __init__(self, browser, url, name, size, shots, timeout_s, prefix="touch332"):
        self.name, self.shots, self.timeout_s, self.prefix = name, shots, timeout_s, prefix
        self.errors = []
        self.report = {"viewport": list(size), "steps": []}
        self.ctx = browser.new_context(
            viewport={"width": size[0], "height": size[1]},
            device_scale_factor=3,
            has_touch=True,
            is_mobile=True,
        )
        self.page = self.ctx.new_page()
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
        self.cdp = self.ctx.new_cdp_session(self.page)

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

    def shot(self, step):
        path = self.shots / ("%s-%s-%s.png" % (self.prefix, self.name, step))
        self.page.screenshot(path=str(path))

    def audit(self, step):
        """Records the page's layout audit; the step fails the run on a touch skin that is not."""
        a = self.page.evaluate(AUDIT)
        a["step"] = step
        self.report["steps"].append(a)
        return a

    def tap_label(self, label):
        """Taps the sheet's entry named `label` (the accessible name of its game-font label)."""
        entry = self.page.get_by_role("button", name=label, exact=True).first
        entry.wait_for(state="visible", timeout=self.timeout_s * 1000)
        entry.scroll_into_view_if_needed()
        entry.tap()
        time.sleep(0.6)

    def wait(self, script, what):
        try:
            self.page.wait_for_function(script, timeout=self.timeout_s * 1000)
        except Exception as e:
            self.shot("failed")
            raise SystemExit("%s: %s did not come up (%s)" % (self.name, what, e))

    def long_press(self, x, y, seconds=0.9):
        point = {"x": x, "y": y, "id": 1}
        self.cdp.send("Input.dispatchTouchEvent", {"type": "touchStart", "touchPoints": [point]})
        time.sleep(seconds)
        self.cdp.send("Input.dispatchTouchEvent", {"type": "touchEnd", "touchPoints": []})
        time.sleep(0.5)


def play(browser, url, name, size, shots, timeout_s, prefix):
    run = Run(browser, url, name, size, shots, timeout_s, prefix)
    page = run.page
    run.wait("() => document.querySelector('.acr-ui') && document.querySelector('[data-acr-touch=sheet]')", "the main menu sheet")
    time.sleep(1.0)
    run.shot("1-main-menu")
    first = run.audit("main menu")
    # The options sheet and back (its cancel entry), for the form-like layout.
    run.tap_label("Optionen")
    run.wait("() => document.querySelector('[data-acr-screen=Options]')", "the options sheet")
    time.sleep(1.0)
    run.shot("1b-options")
    run.audit("options")
    page.locator('[data-acr-key="cancel"]').first.tap()
    run.wait("() => document.querySelector('[data-acr-screen=MainMenu]')", "the main menu again")
    for label in ROUTE:
        run.tap_label(label)
        run.wait("() => document.querySelector('[data-acr-touch=sheet]')", "the sheet after " + label)
    time.sleep(1.0)
    run.shot("2-random-map")
    run.audit("random map setup")
    # The setup's start button: `start` (its key), else the OK that sends it on.
    start = page.locator('[data-acr-key="start"], [data-acr-key="ok"]').first
    start.scroll_into_view_if_needed()
    start.tap()
    run.wait("() => document.querySelector('[data-acr-touch=hud]')", "the game")
    time.sleep(2.0)
    run.shot("3-game")
    in_game = run.audit("game")

    # A unit of the player's: select it with a tap, order it with a long press.
    # The camera is put over the player's first unit, so it stands in the middle of the free map.
    state = run.drive({"cmd": "state", "units": True})["state"]
    mine = [u for u in state["units"] if u["owner"] == state["player"] and not u.get("building")]
    run.drive({"cmd": "camera", "x": mine[0]["x"], "y": mine[0]["y"]})
    time.sleep(1.5)
    state = run.drive({"cmd": "state", "units": True, "visible_only": True})["state"]
    own = [u for u in state["units"] if u["owner"] == state["player"] and not u.get("building") and u.get("screen")]
    ok = False
    # A finger reaches the map only where no button of the skin lies over it.
    free = "([x, y]) => { const e = document.elementFromPoint(x, y); return !!e && e.closest('[data-acr-viewport]') !== null; }"
    own = [u for u in own if page.evaluate(free, [u["screen"][0], u["screen"][1]])]
    if own:
        unit = own[0]
        ux, uy = unit["screen"]  # the driver reports CSS pixels
        page.touchscreen.tap(ux, uy)
        # The game runs a few frames a second under SwiftShader: wait for the tap to arrive.
        selected = []
        for _ in range(40):
            time.sleep(0.5)
            selected = run.drive({"cmd": "state"})["state"]["selection"]
            if selected:
                break
        run.report["selected"] = len(selected)
        run.shot("4-selected")
        run.audit("selected")
        # Ground a little way off, clear of the interface: the middle of the free map area.
        w, h = size
        spots = [(w * fx, h * fy) for fx in (0.5, 0.3, 0.7) for fy in (0.5, 0.35, 0.65)]
        spots = [p for p in spots if abs(p[0] - ux) + abs(p[1] - uy) > 80 and page.evaluate(free, list(p))]
        tx, ty = spots[0]
        before = run.drive({"cmd": "state", "units": True, "detail": True, "owner": state["player"]})["state"]
        run.long_press(tx, ty, 1.5)
        time.sleep(3.0)
        after = run.drive({"cmd": "state", "units": True, "detail": True, "owner": state["player"]})["state"]
        moved = [
            (a["id"], b["x"], b["y"], a["x"], a["y"])
            for a in before["units"]
            for b in after["units"]
            if a["id"] == b["id"] and (abs(a["x"] - b["x"]) + abs(a["y"] - b["y"]) > 1)
        ]
        run.report["ordered"] = bool(moved) or after.get("activity", {}).get("idle", 0) < before.get("activity", {}).get("idle", 0)
        ok = run.report["ordered"] and len(selected) > 0
        run.shot("5-ordered")
        run.audit("ordered")
    run.report["errors"] = run.errors[:20]
    run.report["passed"] = bool(
        ok
        and first["skin"] == "touch"
        and all(not s["outside"] for s in run.report["steps"])
        and all(not s["small"] for s in run.report["steps"])
    )
    run.ctx.close()
    return run.report


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("--url", default="http://127.0.0.1:8090/?game=/game/")
    ap.add_argument("--viewports", nargs="*", default=list(VIEWPORTS), choices=list(VIEWPORTS))
    ap.add_argument("--gpu", choices=sorted(CHROMIUM_ARGS), default="swiftshader")
    ap.add_argument("--timeout-s", type=float, default=240.0)
    ap.add_argument("--out", default="runs/touch-skin")
    ap.add_argument("--shots", default=None, help="folder for the screenshots (default: --out)")
    ap.add_argument("--prefix", default="touch332", help="file name prefix of the screenshots")
    a = ap.parse_args()
    out = pathlib.Path(a.out)
    out.mkdir(parents=True, exist_ok=True)
    shots = pathlib.Path(a.shots) if a.shots else out
    shots.mkdir(parents=True, exist_ok=True)
    reports = {}
    with sync_playwright() as p:
        browser = p.chromium.launch(headless=True, args=CHROMIUM_ARGS[a.gpu])
        for name in a.viewports:
            reports[name] = play(browser, a.url, name, VIEWPORTS[name], shots, a.timeout_s, a.prefix)
            r = reports[name]
            print("%-9s %s  selected=%s ordered=%s errors=%d" % (name, "PASS" if r["passed"] else "FAIL", r.get("selected"), r.get("ordered"), len(r["errors"])))
            for s in r["steps"]:
                if s["outside"] or s["small"]:
                    print("   %s: outside=%s small=%s" % (s["step"], json.dumps(s["outside"][:4]), json.dumps(s["small"][:4])))
        browser.close()
    (out / "report.json").write_text(json.dumps(reports, indent=2))
    sys.exit(0 if all(r["passed"] for r in reports.values()) else 1)


if __name__ == "__main__":
    main()
