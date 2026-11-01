#!/usr/bin/env python3
"""The Svelte touch skin on phone and tablet viewports (docs/webui.md "Touch skin", issue #332).

Opens a served web build (`cargo xtask serve --game-dir ...`, with the bundle: `cargo xtask ui`) in a
touch context (`hasTouch`, `isMobile`, device pixel ratio 3) at 390x844 portrait, 844x390 landscape
and 820x1180, and plays with fingers only (`locator.tap()`, `touchscreen.tap()` and touch events through
the DevTools protocol):

1. the main menu, down to the options sheet and the random map setup, by tapping the sheet's entries;
   the sheets scroll with touch swipes (DevTools scroll gestures): every control of the options sheet and
   of the random map is brought into view by scrolling, the last control of the options is changed, a
   drop-down opens as a list over the sheet, and the sheets' header arrow (and the Escape key, as Android's
   back gesture sends it) goes back, two levels from the random map to the main menu;
2. the start of the game, by tapping its start button;
3. a unit: one tap on it selects it, a long press on the ground orders it to walk there;

and checks on the way that the page chose the touch skin, that no element lies outside the viewport (the
menu sheets scroll vertically by design: their content is exempt from the vertical test), that every
button is at least 44 CSS pixels each way, that the sheet is one column of at most 560 CSS pixels, and
that the unit's order reached the game (`state`).
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


# Where a control of a sheet stands against the sheet's visible area (below its header, above the viewport's
# bottom edge) and whether a finger on its middle would land on it; `dy` is how far to scroll to centre it.
REACH = """(el) => {
  const bar = document.querySelector('.bar');
  const top = bar ? bar.getBoundingClientRect().bottom : 0;
  const r = el.getBoundingClientRect();
  const hit = document.elementFromPoint(r.left + r.width / 2, Math.min(Math.max(r.top + r.height / 2, 0), window.innerHeight - 1));
  const ok = r.top >= top - 0.5 && r.bottom <= window.innerHeight + 0.5 && hit !== null && el.contains(hit);
  return { ok, dy: r.top + r.height / 2 - (top + window.innerHeight) / 2 };
}"""

# The sheet's layout: its scroll area, whether it overflows, and the column of its entries.
SHEET = """() => {
  const s = document.querySelector('.scroll'), items = document.querySelector('.items');
  const r = items.getBoundingClientRect();
  return { scrollTop: s.scrollTop, overflow: s.scrollHeight - s.clientHeight, width: r.width, left: r.left, right: window.innerWidth - r.right,
           columns: new Set([...items.children].map((c) => Math.round(c.getBoundingClientRect().left))).size,
           hit: document.querySelector('[data-acr-hit]') !== null };
}"""

CONTROLS = ".items [data-acr-id]"
# The controls that change a setting (not the sheet's OK and not a disabled field).
CHANGEABLE = '.items [data-acr-id]:not(:disabled):not([data-acr-key="ok"]):not([data-acr-key="start"])'


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

    def swipe(self, dy):
        """A finger drag that scrolls the page content by `dy` CSS pixels (positive: further down)."""
        w, h = self.page.evaluate("() => [window.innerWidth, window.innerHeight]")
        x, y0 = w * 0.9, h * 0.6
        steps = 12
        send = lambda kind, y: self.cdp.send(
            "Input.dispatchTouchEvent", {"type": kind, "touchPoints": [{"x": x, "y": y, "id": 7}] if kind != "touchEnd" else []}
        )
        send("touchStart", y0)
        for i in range(1, steps + 1):
            send("touchMove", y0 - dy * i / steps)
            time.sleep(0.012)
        send("touchEnd", 0)
        time.sleep(0.15)

    def reach(self, locator):
        """Scrolls the sheet with touch swipes until the control is in view and a finger would land on it."""
        el = locator.element_handle()
        for _ in range(40):
            got = self.page.evaluate(REACH, el)
            if got["ok"]:
                return True
            h = self.page.evaluate("() => window.innerHeight")
            self.swipe(max(-h * 0.4, min(h * 0.4, got["dy"])) or 1)
        return self.page.evaluate(REACH, el)["ok"]

    def reach_all(self, step):
        """Every control of the sheet is reachable by scrolling; the ids that are not go to the report."""
        lost = []
        for i in range(self.page.locator(CONTROLS).count()):
            el = self.page.locator(CONTROLS).nth(i)
            if not self.reach(el):
                lost.append(el.get_attribute("data-acr-id"))
        self.report.setdefault("unreachable", []).extend("%s:%s" % (step, x) for x in lost)
        sheet = self.page.evaluate(SHEET)
        sheet["step"] = step
        self.report.setdefault("sheets", []).append(sheet)
        return sheet

    def back_arrow(self, screen):
        """Taps the header's back arrow and waits for `screen` (None: for the main menu's first page)."""
        arrow = self.page.locator("[data-acr-back]")
        arrow.wait_for(state="visible", timeout=self.timeout_s * 1000)
        box = arrow.bounding_box()
        self.report.setdefault("back_arrow", []).append([box["width"], box["height"]])
        arrow.tap()
        time.sleep(0.8)
        self.wait("() => document.querySelector('[data-acr-screen=%s]') && document.querySelector('[data-acr-touch=sheet]')" % screen, "the " + screen + " sheet")

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
    # The options sheet: scrolled with swipes down to its last control, which is changed; a drop-down opens
    # as a list over the sheet and a choice closes it.
    run.tap_label("Optionen")
    run.wait("() => document.querySelector('[data-acr-screen=Options]')", "the options sheet")
    time.sleep(1.0)
    run.shot("1b-options")
    run.audit("options")
    options = run.reach_all("options")
    last = page.locator(CHANGEABLE).last
    run.reach(last)
    run.shot("1c-options-bottom")
    tree = lambda: json.dumps(run.drive({"cmd": "ui"})["ui"]["widgets"], sort_keys=True)
    before = tree()
    last.tap()
    time.sleep(0.8)
    run.report["last_control_changed"] = tree() != before
    combos = page.locator(".items button.combo")
    combos.last.scroll_into_view_if_needed()
    run.reach(combos.last)
    combos.last.tap()
    page.wait_for_selector(".picker [role=option]", timeout=run.timeout_s * 1000)
    run.shot("1d-picker")
    picker = page.evaluate("() => { const o = document.querySelector('.picker .options').getBoundingClientRect(); return [o.left, o.right, window.innerWidth]; }")
    run.report["picker_full_width"] = picker[0] <= 0.5 and picker[1] >= picker[2] - 0.5
    page.locator(".picker [role=option]").last.tap()
    page.wait_for_selector(".picker", state="detached", timeout=run.timeout_s * 1000)
    # Escape is what Android's back gesture sends: it goes back like the arrow.
    page.keyboard.press("Escape")
    run.wait("() => document.querySelector('[data-acr-screen=MainMenu]')", "the main menu after Escape")
    run.tap_label("Optionen")
    run.wait("() => document.querySelector('[data-acr-screen=Options]')", "the options sheet again")
    run.back_arrow("MainMenu")
    # The campaigns: a list, its description and picture on parchment.
    run.tap_label("Einzelspieler")
    run.wait("() => document.querySelector('[data-acr-touch=sheet]')", "the single player entries")
    page.locator('[data-acr-key="campaigns"]').tap()
    run.wait("() => document.querySelector('[data-acr-screen]:not([data-acr-screen=MainMenu])')", "the campaigns")
    if page.evaluate("() => document.querySelector('[data-acr-screen]').dataset.acrScreen") == "PlayerName":
        # The first visit asks for the player's name.
        page.locator('[data-acr-key="ok"]').tap()
        run.wait("() => !['MainMenu', 'PlayerName'].includes(document.querySelector('[data-acr-screen]')?.dataset.acrScreen)", "the campaigns after the name")
    time.sleep(1.0)
    run.shot("1e-campaigns")
    run.audit("campaigns")
    run.reach_all("campaigns")
    page.locator("[role=option]").nth(1).tap()
    time.sleep(0.8)
    run.shot("1f-campaign-chosen")
    run.back_arrow("MainMenu")
    run.back_arrow("MainMenu")
    for label in ROUTE:
        run.tap_label(label)
        run.wait("() => document.querySelector('[data-acr-touch=sheet]')", "the sheet after " + label)
    run.wait("() => document.querySelector('[data-acr-screen=RandomMap]')", "the random map")
    time.sleep(1.0)
    run.shot("2-random-map")
    run.audit("random map setup")
    random_map = run.reach_all("random map")
    run.reach(page.locator(CONTROLS).last)
    run.shot("2b-random-map-bottom")
    # Two levels back with the header arrow: the setup, then the single player submenu.
    run.back_arrow("MainMenu")
    run.shot("2c-back-single-menu")
    run.back_arrow("MainMenu")
    run.wait("() => !document.querySelector('[data-acr-back]')", "the first page of the main menu")
    time.sleep(0.5)
    run.shot("2d-back-main-menu")
    for label in ROUTE:
        run.tap_label(label)
        run.wait("() => document.querySelector('[data-acr-touch=sheet]')", "the sheet after " + label)
    run.wait("() => document.querySelector('[data-acr-screen=RandomMap]')", "the random map again")
    # The setup's start button: `start` (its key), else the OK that sends it on.
    start = page.locator('[data-acr-key="start"], [data-acr-key="ok"]').first
    start.scroll_into_view_if_needed()
    start.tap()
    run.wait("() => document.querySelector('[data-acr-touch=hud]')", "the game")
    time.sleep(2.0)
    run.shot("3-game")
    in_game = run.audit("game")
    # The resources fold like the minimap: the bar's buttons stay, the panel goes.
    fold = page.locator('[data-acr-id="bar/resources"]')
    run.report["resources_fold"] = False
    if fold.count():
        was = page.locator("[data-acr-touch=resources]").count()
        fold.tap()
        time.sleep(0.5)
        run.shot("3b-resources-folded")
        run.report["resources_fold"] = was == 1 and page.locator("[data-acr-touch=resources]").count() == 0
        fold.tap()
        time.sleep(0.5)
        # Pause and menu are never covered by the resources.
        run.report["bar_clear"] = page.evaluate(
            "() => ['bar/pause', 'bar/menu'].every((id) => { const el = document.querySelector(`[data-acr-id=\"${id}\"]`); const r = el.getBoundingClientRect(); const hit = document.elementFromPoint(r.left + r.width / 2, r.top + r.height / 2); return el.contains(hit); })"
        )

    # The speed button opens one slider; a tap near its right end snaps to the fast stops, a tap outside closes it.
    speed = page.locator('[data-acr-id="bar/speed"]')
    run.report["speed_popover"] = False
    if speed.count():
        speed.tap()
        time.sleep(0.5)
        run.shot("3c-speed")
        slider = page.locator("[data-acr-touch=speed] input[type=range]")
        box = slider.bounding_box()
        before = slider.input_value()
        page.touchscreen.tap(box["x"] + box["width"] * 0.9, box["y"] + box["height"] / 2)
        time.sleep(0.8)
        run.shot("3d-speed-fast")
        after = page.locator("[data-acr-touch=speed] input[type=range]").input_value()
        page.touchscreen.tap(8, size[1] / 2)
        time.sleep(0.4)
        run.report["speed_popover"] = before == "2" and int(after) >= 5 and page.locator("[data-acr-touch=speed]").count() == 0

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
    # The command grids the touch skin draws itself (`data-acr-glyph`) and the engine's enlarged portraits
    # (`scale/4/..`): the peasants' build menu, then a fort (the page buttons), put up finished beside them.
    state = run.drive({"cmd": "state", "units": True})["state"]
    mine = [u for u in state["units"] if u["owner"] == state["player"]]
    peasants = [u["id"] for u in mine if u["type"].startswith("SpnKri")]
    grids = {}
    for step, ids in (("6-peasants", peasants[:1]), ("7-fort", None)):
        if ids is None:
            at = next(u for u in mine if u["id"] in peasants)
            fort = run.drive({"cmd": "spawn", "type": "SpnKre", "owner": state["player"], "x": at["x"], "y": at["y"] + 500, "dir": 0})
            ids = [fort["unit"]["id"]]
        # The camera over the unit, a finger on it (a spawned fort's centre is on its first tile).
        target = next(u for u in run.drive({"cmd": "state", "units": True})["state"]["units"] if u["id"] == ids[0])
        run.drive({"cmd": "camera", "x": target["x"], "y": target["y"]})
        time.sleep(1.5)
        target = next(u for u in run.drive({"cmd": "state", "units": True, "visible_only": True})["state"]["units"] if u["id"] == ids[0])
        page.touchscreen.tap(*target["screen"])
        time.sleep(2.0)
        grids[step] = page.evaluate("() => ({ glyphs: document.querySelectorAll('[data-acr-glyph]').length, icons: document.querySelectorAll('.icon').length, portraits: document.querySelectorAll('.icon img').length })")
        run.shot(step)
        run.audit(step)
    run.report["grids"] = grids
    run.report["errors"] = run.errors[:20]
    sheets = run.report["sheets"]
    run.report["passed"] = bool(
        ok
        and first["skin"] == "touch"
        and not run.errors
        and not run.report["unreachable"]
        and run.report["last_control_changed"]
        and run.report["picker_full_width"]
        and run.report["resources_fold"]
        and run.report["speed_popover"]
        and run.report.get("bar_clear")
        # One column of at most 560 CSS pixels, centred, whose whole area is the host's.
        and all(s["columns"] == 1 and s["width"] <= 560.5 and abs(s["left"] - s["right"]) < 1 and s["hit"] for s in sheets)
        # The arrow is a 44 pixel target; a sheet that overflows was scrolled.
        and all(w >= 44 and h >= 44 for w, h in run.report["back_arrow"])
        and (options["overflow"] <= 0 or options["scrollTop"] > 0)
        and (random_map["overflow"] <= 0 or random_map["scrollTop"] > 0)
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
            print("   unreachable=%s last_changed=%s picker_full_width=%s" % (r["unreachable"], r["last_control_changed"], r["picker_full_width"]))
            for s in r["sheets"]:
                print("   %s: overflow=%s width=%.0f columns=%s" % (s["step"], round(s["overflow"]), s["width"], s["columns"]))
            for s in r["steps"]:
                if s["outside"] or s["small"]:
                    print("   %s: outside=%s small=%s" % (s["step"], json.dumps(s["outside"][:4]), json.dumps(s["small"][:4])))
        browser.close()
    (out / "report.json").write_text(json.dumps(reports, indent=2))
    sys.exit(0 if all(r["passed"] for r in reports.values()) else 1)


if __name__ == "__main__":
    main()
