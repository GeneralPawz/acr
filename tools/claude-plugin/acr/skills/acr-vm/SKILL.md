---
name: acr-vm
description: "How to build, check, test, run Blender asset builds and take GPU screenshots of ACR on the render VM (claude-render, RX 9060 XT) from a local worktree. Covers the acr-vm-cargo, acr-vm-drive, acr-check and acr-shots commands. Use for any cargo, xtask or screenshot step in ACR."
---

# The render VM

`claude-render` (192.168.178.130):
- Ubuntu 24.04, 12 cores, 23 GB RAM, RX 9060 XT;
- Rust 1.99.1 with the wasm32 and Windows targets, sccache shared by all branches;
- Blender 5.2;
- the game data (`~/game/American Conquest`) and the showcase models;
- a headless X server on the GPU (`DISPLAY=:5`).

You edit files locally. Every command below syncs your worktree to the VM first (changed files only), runs there, and brings the output back. Run them from your worktree root. They are commands on the Bash PATH (the acr plugin's `bin/`).

Everything lives under `~/projects/acr` on the VM (checkout `ACR`, worktrees `ACR-wt/`, staged copies `wt/`, build dirs `target/`, `sources`, `game`, `ACR-models`, `ACR-analysis`); `~/wt`, `~/target`, `~/sources`, `~/game` and the other old paths are links into it.

**Working on the VM itself** (a Claude session on `claude-render`): the same commands work unchanged. They detect the VM (hostname, or `ACR_VM_LOCAL=1`) and run through the local shell instead of ssh; `acr-showcase` builds the Linux showcase (`~/projects/acr/acr-test/showcase`) and `blender-vm` runs `/opt/blender/blender` directly. Use worktrees under `~/projects/acr/ACR-wt/`.

## Checks, tests, builds: `acr-vm-cargo`

```
acr-vm-cargo -- acr-check     # the 5 landing checks, PASS/FAIL each
acr-vm-cargo -- cargo test -q -p acr-game editor  # anything cargo
acr-vm-cargo -- cargo xtask assets artpus \
    --get 'target/assets/units/artpus.glb=C:/Users/verwalter/WIP/ACR-analysis/x/artpus.glb'
```

- `--get REMOTE=LOCAL` fetches a file or folder afterwards. `--get` may stand before or after `--`. REMOTE is relative to the worktree on the VM (xtask writes target/assets there). A failed fetch prints an error and exits 3; LOCAL is an absolute Windows path.
- `--no-sync` skips the upload.
- Each worktree has its own target dir on the VM. A cold check takes about 1m45s, an incremental one about 15 s, a release build about 1 min.
- `cargo xtask assets <names>` runs Blender right there. Never use `--help`: it builds everything.
- At most two commands build at once; others queue.
- Quote a command with `&&` or pipes as one argument.

## The game headless on the GPU: `acr-vm-drive`

```
acr-vm-drive --steps C:/abs/steps.json --out C:/abs/out [--models C:/abs/glbs | --models wt:target/assets/units] [--size 1600x900] [--no-build]
             [--settings models=all,shadows=off] [--env K=V ...]
```

1. Builds your branch's release game on the VM.
2. Runs it with `--drive --drive-gpu` on the GPU.
3. Feeds the steps (a JSON list of driver requests: load the `acr-driver` skill).
4. Fetches `answers.jsonl`, `game.log` and the screenshots into `--out`. Give each screenshot step a relative `path`.

**Settings:** the run reads the showcase `settings.txt` (`models=units`). `--settings k=v,...` copies it into the run folder with those keys set (e.g. `models=all` for buildings too, `models=off` for sprites). `--env K=V` exports more variables; VM paths work as written from Git Bash (`--env ACR_SETTINGS_DIR=/home/claude/x`): the wrappers switch MSYS path conversion off, so no `//home` trick. After the run it prints the game's `settings: loaded <file>: models=...` line; `no settings file at ...` means the path was wrong and the defaults (models off) were used.

**Models:** the showcase models are synced up automatically; `--models` lays your own glbs over them for that run only. A screenshot takes about 30 ms, and a small run without a rebuild about 3 s. One run is one game process, so it is one field.

## Cheap images: `acr-shots`

An image costs about width × height / 750 tokens to look at. Trim before reading:

```
acr-shots trim C:/abs/out/a.png --max 700
acr-shots sheet C:/abs/out/sheet.png C:/abs/out/f*.png --cols 6 --cell 300 --labels t0,t10,t20
```

- **trim:** crops to the content (everything that differs from the corner colour) and scales down. On a white or greenscreen field this keeps just the units.
- **sheet:** crops all frames to their common box, so a sequence stays aligned, then tiles and labels them.

Look at the trimmed image or the sheet, not the full frames.

## Troubleshooting

- **`no DX12/Vulkan/Metal adapter`:** the X server is down. Run `ssh -i ~/.ssh/claude-render -o IdentitiesOnly=yes claude@192.168.178.130 'sudo systemctl restart xorg-gpu'`. Never `pkill -f Xorg`: over ssh it kills your own shell.
- **The VM is unreachable:** stop and report it. Do not fall back to local builds.
