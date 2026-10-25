"""Ranked report of ACR performance snapshots (docs/testing.md "Performance recorder").

    python -I tools/perf/perf_report.py [SNAP.json ...]   # default: the newest in the folder
    python -I tools/perf/perf_report.py --all             # one line per snapshot, oldest first

Snapshots are written by Ctrl + Space, R (modern controls), automatically after 3 s of
frames over 250 ms, or by the driver's {"cmd":"perf","snapshot":true}; they live in
%LOCALAPPDATA%\\ACR\\perf (or perf/ next to ACR_LOG, or ACR_PERF_DIR). Standard library only.
"""
import glob
import json
import os
import statistics
import sys


def snap_dir():
    if os.environ.get("ACR_PERF_DIR"):
        return os.environ["ACR_PERF_DIR"]
    base = os.environ.get("LOCALAPPDATA") or os.path.expanduser("~")
    return os.path.join(base, "ACR", "perf")


def mean(v):
    v = [x for x in v if x is not None]
    return statistics.fmean(v) if v else None


def fmt(v, unit="", nd=1):
    return "-" if v is None else f"{v:.{nd}f}{unit}"


def report(path):
    s = json.load(open(path, encoding="utf-8"))
    su = s["summary"]
    fr = s["frames"]
    ad = s.get("adapter") or {}
    out = []
    p = out.append
    p(f"== {os.path.basename(path)} ({s['reason']}, {s['time_utc']}, up {s['uptime_s']:.0f} s)")
    p(f"adapter: {ad.get('name', '?')} | {ad.get('backend', '?')} {ad.get('device_type', '')} | driver {ad.get('driver', '')} {ad.get('driver_info', '')}")
    p(f"gpu timing: {s['gpu_timing']}")
    settings = {k: v for k, _, v in (line.partition("=") for line in s["settings"])}
    keys = ["models", "shadows", "lod", "gore", "camera", "controls", "ground", "water", "corpse_minutes"]
    p("settings: " + ", ".join(f"{k}={settings[k]}" for k in keys if k in settings))
    fm = su["frame_ms"]
    p(f"last {su['window_s']} s: {su['fps']:.1f} fps, frame mean {fm['mean']:.1f} ms, p50 {fm['p50']:.1f}, p95 {fm['p95']:.1f}, p99 {fm['p99']:.1f}, max {fm['max']:.1f} ({su['frames']} frames)")
    d = s["diagnosis"]
    p(f"diagnosis: {d['bound'].upper()}-bound")
    for line in d["lines"][1:]:
        p(f"  - {line}")
    for lever in d["levers"]:
        p(f"  > try: {lever}")
    c = su["cpu_ms"]
    g = su.get("gpu_ms")
    p(f"cpu: advance {c['advance']:.2f} ms, render {c['render_cpu']:.2f}, acquire {c['acquire']:.2f}, present {c['present']:.2f}")
    if g:
        p(f"gpu: total {g['total']:.2f} ms = ground {g['ground']:.2f} + shadow map {g['shadow_map']:.2f} + 3d pass {g['3d_pass']:.2f} + interface {g['interface']:.2f} ({g['samples']} samples)")
    k = su["counts"]
    b = s["budgets"]
    p(f"counts: {k['triangles'] / 1e6:.2f} M tri (budget {b['triangles'] / 1e6:.1f}), {k['shadow_triangles'] / 1e6:.2f} M cast (budget {b['shadow_triangles'] / 1e6:.1f}), {k['draws']} draws (budget {b['draws']}), {k['shadow_draws']} shadow draws")
    p(f"        {k['drawn']} drawn ({k['instances']} instances + caps and inserts), lod {k['lod']}, {k['units']} units, {k['corpses']} corpses, {k['ragdolls_awake']} ragdolls, {k['particles']} particles, {k['decals']} decals, zoom {k['zoom']}, window {k['window']}")
    m = s["memory_mb"]
    p(f"memory: gpu allocated {m['gpu_allocated']:.0f} MB (reserved {m['gpu_reserved']:.0f}), model textures {m['model_textures']:.0f} MB")
    p(f"recorder: {su['recorder_us']:.1f} us per frame, {su.get('scope_calls', 0):.0f} profiler scope calls per frame")
    p("top scopes (mean / max ms per frame, last 10 s; nested scopes count in their parents):")
    for t in s["top_scopes"][:12]:
        p(f"  {t['name']:<26} {t['mean_ms']:>8.2f} {t['max_ms']:>8.2f}")
    p("worst frames:")
    for w in s["worst_frames"][:6]:
        top = ", ".join(f"{n} {v:.1f}" for n, v in w["scopes"][:4])
        p(f"  t {w['t']:>7.1f} s  {w['ms']:>7.1f} ms  gpu {fmt(w['gpu_ms'], ' ms')}  {w['triangles'] / 1e6:.2f} M tri {w['draws']} draws  {w['units']} units  | {top}")
    # The frame time over the kept window in ten slices: where it got slow.
    n = len(fr["ms"])
    if n >= 10:
        p("timeline (frame ms mean per slice, triangles M, units):")
        for i in range(10):
            a, z = i * n // 10, (i + 1) * n // 10
            p(f"  t {fr['t'][a]:>7.1f}..{fr['t'][z - 1]:<7.1f} {fmt(mean(fr['ms'][a:z]), ' ms'):>10}  {fmt(mean(fr['triangles'][a:z]) / 1e6, ' M', 2):>8}  {mean(fr['units'][a:z]):>6.0f}")
    return "\n".join(out)


def one_line(path):
    s = json.load(open(path, encoding="utf-8"))
    su = s["summary"]
    k = su["counts"]
    return f"{os.path.basename(path)}  {su['fps']:6.1f} fps  {su['frame_ms']['mean']:7.1f} ms  {s['diagnosis']['bound']:>5}  {k['triangles'] / 1e6:5.2f} M tri  {k['draws']:5} draws  {k['units']:5} units"


def main(argv):
    if argv and argv[0] == "--all":
        for f in sorted(glob.glob(os.path.join(snap_dir(), "snap-*.json"))):
            print(one_line(f))
        return
    files = argv or sorted(glob.glob(os.path.join(snap_dir(), "snap-*.json")))[-1:]
    if not files:
        sys.exit(f"no snapshots in {snap_dir()}")
    for f in files:
        print(report(f))
        print()


if __name__ == "__main__":
    main(sys.argv[1:])
