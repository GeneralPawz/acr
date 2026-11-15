#!/usr/bin/env python3
"""The unit inspector in the Svelte touch skin (docs/controls.md "Inspector", issue #499).

Playwright with a phone held sideways (844x390) and upright (390x844), against
`cargo xtask serve --release --game-dir ...` with the UI bundle built. Per viewport it starts the
blank field on the inspector, and checks that the drawer opened by itself, that it shows the
nation's units as icon cells and the controls above them, that it leaves the model its part of
the picture (the left of a sideways window, the top of an upright one), that a tap on a unit
cell places that unit, that the clip stepper names a clip of the model, that a drag on the
picture turns the camera and that two fingers move the picture with the unit kept inside it. Screenshots go to `--shots` as `inspector-<viewport>-<step>.png`.

Run it with
`~/browser/venv/bin/python tools/browser/touch_inspector.py --url 'http://127.0.0.1:8090/?game=/game/'`.
"""
import argparse
import json
import pathlib
import sys
import time

from playwright.sync_api import sync_playwright

from touch_skin import CHROMIUM_ARGS, Run

VIEWPORTS = {"sideways": (844, 390), "upright": (390, 844)}
DRAWER = "aside.drawer"
# Where the drawer and the picture's free part lie.
LAYOUT = """() => {
  const d = document.querySelector('aside.drawer');
  if (!d) return null;
  const r = d.getBoundingClientRect();
  return {
    left: r.left, top: r.top, width: r.width, height: r.height,
    cells: d.querySelectorAll('[data-acr-id^="editor/inspect/pick/"]').length,
    first: d.querySelector('.scroll').firstElementChild.className,
    inspector: d.classList.contains('inspector'),
  };
}"""


def drag(run, x0, x1, y, steps=24, pause=0.05):
    """A slow one-finger drag along a row. On the narrow upright phone it starts so slowly that the finger counts
    as resting first (the long press, the right button): the camera turns all the same."""
    send = lambda kind, x: run.cdp.send(
        "Input.dispatchTouchEvent", {"type": kind, "touchPoints": [{"x": x, "y": y, "id": 9}] if kind != "touchEnd" else []}
    )
    send("touchStart", x0)
    for i in range(1, steps + 1):
        send("touchMove", x0 + (x1 - x0) * i / steps)
        time.sleep(pause)
    send("touchEnd", x1)
    time.sleep(0.4)


def state(run):
    return run.drive({"cmd": "state"})["state"]


def play(browser, url, name, size, shots, timeout_s):
    run = Run(browser, url, name, size, shots, timeout_s, prefix="inspector")
    page, (w, h) = run.page, size
    run.drive({"cmd": "start", "blank": {"inspector": True, "ground": "dark"}})
    r = run.report
    try:
        page.wait_for_selector(DRAWER, timeout=20000)
        r["opened"] = True
    except Exception:
        r["opened"] = False
        run.shot("failed")
        r["passed"] = False
        return r
    # The model loads and is drawn before its clips are listed.
    end = time.time() + timeout_s
    while time.time() < end and not state(run).get("inspector", {}).get("clips"):
        time.sleep(0.5)
    time.sleep(1.5)
    run.shot("1-first")
    layout = page.evaluate(LAYOUT)
    r["layout"] = layout
    upright = h > w
    r["placed"] = layout["inspector"] and (
        (layout["top"] >= 0.5 * h and layout["width"] >= w - 1) if upright else (layout["left"] >= 0.45 * w)
    )
    r["controls_first"] = "controls" in layout["first"]
    r["cells"] = layout["cells"]
    # The sixth unit of England's list is the line musketeer.
    cell = page.locator(DRAWER + ' [data-acr-id="editor/inspect/pick/5"]')
    cell.scroll_into_view_if_needed()
    cell.tap()
    end = time.time() + timeout_s
    while time.time() < end:
        i = state(run).get("inspector", {})
        if (i.get("unit") or {}).get("type") == "EngM18(EN)" and len(i.get("clips", [])) > 10:
            break
        time.sleep(0.5)
    time.sleep(1.5)
    r["picked"] = (state(run)["inspector"]["unit"] or {}).get("type")
    run.shot("2-musketeer")
    # The clip stepper: twice on, a clip of the model plays.
    step = page.locator(DRAWER + ' [data-acr-id="editor/inspect/clipstep/+"]')
    step.scroll_into_view_if_needed()
    step.tap()
    step.tap()
    time.sleep(0.8)
    i = state(run)["inspector"]
    r["clip"] = i["clip"]
    r["clip_ok"] = i["clip"] in i["clips"]
    # A drag on the free picture turns the camera, also while the game is paused.
    run.drive({"cmd": "key", "key": "pause"})
    time.sleep(0.3)
    r["paused"] = state(run).get("paused")
    before = state(run).get("orbit")
    x, y = (w * 0.25, h * 0.6) if not upright else (w * 0.3, h * 0.36)
    r["under"] = page.evaluate(
        "([x, y]) => { const e = document.elementFromPoint(x, y); return e ? e.tagName + '.' + e.className + ' ' + (e.closest('[data-acr-viewport]') !== null) : null; }",
        [x, y],
    )
    drag(run, x, x + 0.3 * w, y)
    time.sleep(0.6)
    after = state(run).get("orbit")
    r["orbit"] = [before, after]
    r["turned"] = before is not None and after is not None and before != after
    run.shot("3-clip-turned")
    # Two fingers move the picture: the unit goes with them, stays inside the free part, and the
    # camera still turns round it afterwards.
    def two(kind, dx):
        pts = [] if kind == "touchEnd" else [{"x": x + dx, "y": y, "id": 1}, {"x": x + dx, "y": y + 60, "id": 2}]
        run.cdp.send("Input.dispatchTouchEvent", {"type": kind, "touchPoints": pts})

    two("touchStart", 0)
    for n in range(1, 13):
        two("touchMove", -0.6 * w * n / 12)
        time.sleep(0.03)
    two("touchEnd", 0)
    time.sleep(0.6)
    off = state(run)["inspector"]["offset"]
    r["offset"] = off
    drag(run, x, x + 0.2 * w, y)
    time.sleep(0.5)
    r["offset_after_turn"] = state(run)["inspector"]["offset"]
    # The fingers went left along a row: so did the unit, although the camera is turned.
    r["panned"] = off[0] < -20 and abs(off[1]) < 12 and r["offset_after_turn"] == off
    run.shot("4-panned")
    r["errors"] = run.errors
    r["passed"] = bool(
        r["opened"] and r["placed"] and r["controls_first"] and r["cells"] > 5 and r["picked"] == "EngM18(EN)"
        and r["clip_ok"] and r["turned"] and r["panned"] and not run.errors
    )
    return r


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("--url", default="http://127.0.0.1:8090/?game=/game/")
    ap.add_argument("--viewports", nargs="*", default=list(VIEWPORTS), choices=list(VIEWPORTS))
    ap.add_argument("--gpu", choices=sorted(CHROMIUM_ARGS), default="swiftshader")
    ap.add_argument("--timeout-s", type=float, default=240.0)
    ap.add_argument("--out", default="runs/touch-inspector")
    ap.add_argument("--shots", default=None, help="folder for the screenshots (default: --out)")
    a = ap.parse_args()
    out = pathlib.Path(a.out)
    out.mkdir(parents=True, exist_ok=True)
    shots = pathlib.Path(a.shots) if a.shots else out
    shots.mkdir(parents=True, exist_ok=True)
    reports = {}
    with sync_playwright() as p:
        browser = p.chromium.launch(headless=True, args=CHROMIUM_ARGS[a.gpu])
        for name in a.viewports:
            r = reports[name] = play(browser, a.url, name, VIEWPORTS[name], shots, a.timeout_s)
            keys = ("opened", "placed", "controls_first", "cells", "picked", "clip", "paused", "turned", "panned", "offset")
            print("%-9s %s  %s errors=%d" % (name, "PASS" if r["passed"] else "FAIL", {k: r.get(k) for k in keys}, len(r.get("errors", []))))
        browser.close()
    (out / "report.json").write_text(json.dumps(reports, indent=2))
    sys.exit(0 if all(r["passed"] for r in reports.values()) else 1)


if __name__ == "__main__":
    main()
