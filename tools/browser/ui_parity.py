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

# How to bring a screen up from the main menu: labels to click, in order (an [x, y] pair clicks
# that canvas position, e.g. to open a combo box). The screens the UI
# claims later add their routes with their issues.
ROUTES = {
    "MainMenu": [],
    "PlayerName": ["Einzelspieler", "Zufallskarte"],
    "Options": ["Optionen"],
    "Campaigns": ["Einzelspieler", "Kampagne", "OK"],
    "SingleMission": ["Einzelspieler", "Einzelmission", "OK"],
    "Placeholder": ["Mehrspieler", "Internetspiel"],
    "LoadGame": ["Laden"],
    "RandomMap": ["Einzelspieler", "Zufallskarte", "OK"],
    "RandomMapCombo": ["Einzelspieler", "Zufallskarte", "OK", [840, 199]],
    "CampaignMissions": ["Einzelspieler", "Kampagne", "OK", "Mission spielen!"],
}

# Over the running game (`Screen::in_game`): start the blank grass field (nothing moves, and the game
# stands still under a dialog, so both captures show the same frame), then open the dialog. A dict is
# a driver request sent as is. Pixels are counted over the dialog's area (`--rect`, see `dialog_area`).
BLANK = {"cmd": "start", "blank": {"ground": "grass"}}
MENU = [BLANK, {"cmd": "key", "key": "F12"}]
MORE = MENU + ["Spieleinstellungen", "Weitere Optionen »"]
# The briefing of the first single mission is a MessageWindow; once OK, the "M" drop-down's last row
# (UM_HIST, 0x4c6cb0) opens the history. The mission loads in the page (seconds), so a route waits; the
# message box takes only `ui` of the driver, so Enter dismisses it.
BRIEFING = ["Einzelspieler", "Einzelmission", "OK", "Die Indianeraufstände.", "Mission spielen!"]
ROUTES.update({
    "MessageWindow": BRIEFING,
    "HistoryWindow": BRIEFING + [{"wait_screen": "MessageWindow"}, {"press": "Enter"}, {"wait_screen": "game"}, [275, 17], [276, 288]],
    "GameMenu": MENU,
    "GameOptions": MENU + ["Spieleinstellungen"],
    "GameOptionsMore": MORE,
    "GameOptionsRules": MORE + ["Seite 2/6: Grafik »", "Seite 3/6: Details »", "Seite 4/6: ACR-Regeln »"],
    "GameLoadGame": MENU + ["Laden"],
    "SaveGameDialog": MENU + ["Spiel speichern"],
})

# The parts of the in-game interface (#326), on the same blank field: the top bar (also wide, centred
# and the touch bar, at the window sizes and settings of `HUD_SETTINGS`). The mouse waits mid-screen otherwise: in a corner it would scroll the map.
# Pixels are counted over the part's `HUD_AREAS` box. They are `game` for the driver's tree, which has no widget of them.
ROUTES.update({
    "TopBar": [BLANK, {"hover": [512, 400]}],
    "TopBarWide": [BLANK, {"hover": [960, 540]}],
    "TopBarCentered": [BLANK, {"hover": [960, 540]}],
    "TopBarTouch": [BLANK, {"hover": [792, 360]}],
    "TopBarTouchOpen": [BLANK, [1547, 19], {"hover": [792, 360]}],
})
# Settings and window size of the routes that need them (`top_bar`, `center_top_bar` and `hud_scale` of
# settings.txt); the others run at --size.
HUD_SETTINGS = {
    "TopBarWide": ("", [1920, 1080]),
    "TopBarCentered": ("center_top_bar=on\n", [1920, 1080]),
    "TopBarTouch": ("top_bar=touch\n", [1584, 720]),
    "TopBarTouchOpen": ("top_bar=touch\n", [1584, 720]),
}
HUD_AREAS = {
    "TopBar": "243,0,781,44",
    "TopBarWide": "0,0,1920,44",
    "TopBarCentered": "0,0,1920,44",
    "TopBarTouch": "0,0,1584,70",
    "TopBarTouchOpen": "1100,0,484,330",
}

# Routes whose screen is another route's (`Screen::name`): the campaign's missions are a SingleMission.
SCREEN_OF = {
    "CampaignMissions": "SingleMission", "RandomMapCombo": "RandomMap", "GameOptionsMore": "GameOptions",
    "GameOptionsRules": "GameOptions", "GameLoadGame": "LoadGame",
    "TopBar": "game", "TopBarWide": "game",
    "TopBarCentered": "game", "TopBarTouch": "game", "TopBarTouchOpen": "game",
}

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


# The interface's parts are laid out by the same stage maths as the dialogs: every sprite and fill
# (`data-acr-part-rect`, the rect Rust published, in game canvas pixels) as the page places it. Their
# groups are at hud_scale 1 here. Text has no box of its own; its pixels are counted.
HUD_RECTS = """() => {
    const hud = document.querySelector('[data-acr-hud]');
    if (!hud) return {};
    const box = hud.getBoundingClientRect();
    const scale = box.width / hud.offsetWidth;
    const out = {};
    for (const group of document.querySelectorAll('[data-acr-hud-group]')) {
        group.querySelectorAll('[data-acr-part-rect]').forEach((e, i) => {
            let [x, y, w, h] = e.dataset.acrPartRect.split(',').map(Number);
            // A group at hud_scale is stretched about its anchor.
            const k = Number(group.dataset.acrHudScale);
            const [ax, ay] = group.dataset.acrHudAnchor.split(',').map(Number);
            [x, y, w, h] = [ax + (x - ax) * k, ay + (y - ay) * k, w * k, h * k];
            const r = e.getBoundingClientRect();
            out[group.dataset.acrHudGroup + '/' + i] = {
                dom: [r.x, r.y, r.width, r.height],
                want: [box.x + x * scale, box.y + y * scale, w * scale, h * scale],
            };
        });
    }
    return out;
}"""


class Page:
    """One browser page on the game, with the driver hook."""

    def __init__(self, browser, url, size, timeout_s, extra="", init=""):
        self.timeout_s = timeout_s
        self.errors = []
        self.page = browser.new_page(viewport={"width": size[0], "height": size[1]}, device_scale_factor=1)
        self.page.on("console", lambda m: m.type == "error" and self.errors.append(m.text))
        self.page.on("pageerror", lambda e: self.errors.append(str(e)))
        self.page.add_init_script(
            "localStorage.setItem('acr-settings', %s);" % json.dumps(base64.b64encode((SETTINGS + extra).encode()).decode())
        )
        if init:
            self.page.add_init_script(init)
        self.page.goto(url)
        self.page.wait_for_function(
            "() => document.querySelector('canvas') && !document.getElementById('picker')",
            timeout=timeout_s * 1000,
        )
        self.page.evaluate("() => window.__acr = import('./acr_game.js')")
        self.hover = None

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

    def wait_screen(self, screen):
        """Until the driver's tree is at `screen` and built (`game`: no dialog)."""
        end = time.time() + self.timeout_s
        while time.time() < end:
            tree = self.tree()
            if tree["screen"] == screen and tree["built"] and not tree["busy"]:
                return
            time.sleep(0.2)
        raise SystemExit("the screen %s did not come up (at %s)" % (screen, tree["screen"]))

    def show(self, screen, route):
        """Brings `screen` up and waits until it is built and the page is quiet."""
        for step in route:
            if isinstance(step, dict) and "wait_screen" in step:
                self.wait_screen(step["wait_screen"])
            elif isinstance(step, dict) and "hover" in step:
                # The mouse stays here for the screenshot (a hint needs it over its target).
                self.hover = step["hover"]
                self.page.mouse.move(*self.hover)
            elif isinstance(step, dict) and "press" in step:
                # A message box takes no driver request but `ui`: the key goes in like a player's.
                self.page.keyboard.press(step["press"])
            elif isinstance(step, dict):
                self.drive(step)
            else:
                self.drive({"cmd": "click", "pos": step} if isinstance(step, list) else {"cmd": "click", "label": step})
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
        if self.hover:
            self.page.mouse.move(*self.hover)
        else:
            self.page.mouse.move(self.page.viewport_size["width"] - 1, self.page.viewport_size["height"] - 1)
        time.sleep(1.0)
        return self.tree()


def shoot(browser, url, ui, screen, route, size, timeout_s, out, extra=""):
    page = Page(browser, "%s%sui=%s" % (url, "&" if "?" in url else "?", ui), size, timeout_s, extra)
    tree = page.show(screen, route)
    if ui == "svelte":
        page.page.wait_for_selector("[data-acr-id], [data-acr-hud-group]", state="attached", timeout=timeout_s * 1000)
        page.page.evaluate("async () => { await document.fonts.ready; await new Promise(requestAnimationFrame); }")
        time.sleep(0.5)
    path = out / ("%s.png" % ui)
    page.page.screenshot(path=str(path))
    rects = page.page.evaluate(DOM_RECTS) if ui == "svelte" else {}
    hud = page.page.evaluate(HUD_RECTS) if ui == "svelte" else {}
    errors = page.errors
    page.page.close()
    return tree, rects, hud, errors


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


def hud_layout(hud):
    """The largest pixel difference between the page's and Rust's rect of an interface part."""
    per_id = {k: round(max(abs(a - b) for a, b in zip(v["dom"], v["want"])), 2) for k, v in hud.items()}
    return max(per_id.values(), default=0.0), per_id, sorted(per_id)


def dialog_area(tree, size, margin=40):
    """The box of a dialog over the game: its widgets' rects plus the frame that reaches past them,
    as `x,y,w,h` (the whole page for screens of the menus)."""
    if not tree.get("in_game"):
        return None
    rects = [w["rect"] for w in tree["widgets"] if w.get("visible", True) and w["rect"][2] > 0]
    if not rects:
        return None
    x0 = max(0, min(r[0] for r in rects) - margin)
    y0 = max(0, min(r[1] for r in rects) - margin)
    x1 = min(size[0], max(r[0] + r[2] for r in rects) + margin)
    y1 = min(size[1], max(r[1] + r[3] for r in rects) + margin)
    return "%d,%d,%d,%d" % (x0, y0, x1 - x0, y1 - y0)


def pixel_share(a, b, diff, root, rect=None):
    run = subprocess.run(
        ["cargo", "xtask", "ui-diff", str(a), str(b), "--out", str(diff), "--min", "0"] + (["--rect", rect] if rect else []),
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
    default_size = [int(v) for v in a.size.split("x")]
    screens = [s for s in a.screens.split(",") if s]
    unknown = [s for s in screens if s not in ROUTES]
    if unknown:
        raise SystemExit("no route to %s; known: %s" % (", ".join(unknown), ", ".join(ROUTES)))

    ok = True
    size = default_size
    with sync_playwright() as p:
        browser = p.chromium.launch(headless=True, args=CHROMIUM_ARGS[a.gpu])
        for screen in screens:
            out = pathlib.Path(a.out) / screen
            out.mkdir(parents=True, exist_ok=True)
            extra, size = HUD_SETTINGS.get(screen, ("", default_size))
            tree, _, _, engine_errors = shoot(browser, a.url, "engine", SCREEN_OF.get(screen, screen), ROUTES[screen], size, a.timeout_s, out, extra)
            _, rects, hud, svelte_errors = shoot(browser, a.url, "svelte", SCREEN_OF.get(screen, screen), ROUTES[screen], size, a.timeout_s, out, extra)
            area = HUD_AREAS.get(screen) or dialog_area(tree, size)
            pixels = pixel_share(out / "engine.png", out / "svelte.png", out / "diff.png", root, area)
            if screen in HUD_AREAS:
                layout, per_id, compared = hud_layout(hud)
                extra = []
            else:
                layout, per_id, extra, compared = layout_difference(tree, rects)
            passed = pixels >= a.min_pixels and layout <= a.max_layout and bool(compared) and not svelte_errors
            ok &= passed
            report = {
                "screen": screen, "size": size, "area": area, "pixels": pixels, "layout": layout,
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
