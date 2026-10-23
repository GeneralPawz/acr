---
name: acr-genai
description: "Harvesting generated 3D assets for ACR: reference pictures with Codex, models from the Meshy API and from Rodin (Hyper3D) in the browser, credits and privacy rules, file naming, and the click-by-click Rodin flow. Use when producing gear, props or nature models with generative services."
---

# Generative 3D harvesting

**What generators are for:** rigid things such as equipment (hats, helmets, breastplates, cartridge boxes, muskets, swords, shields, quivers), props, saddles, guns, and nature (trees, rocks).
**What they are not for:** people's bodies or garments. Those come from our MakeHuman pipeline and sewn garments; their generated pictures serve only as references.

## Files

| What | Where |
|---|---|
| Reference pictures | `C:\Users\verwalter\WIP\ACR-analysis\genai\refs-codex\<name>.png` (units: `refs-codex\soldiers\<unit>_<part>.png`) |
| Models | `C:\Users\verwalter\WIP\ACR-analysis\genai\<service>\<name>.glb` plus `<name>.json` (seconds, credits, settings, task id) |
| Style sources (the game's art) | `genai\style\` |

Never put generated or downloaded models into the repo.

## 1. A reference picture (Codex, already paid)

```
acr-genai ref C:/Users/verwalter/WIP/ACR-analysis/genai/refs-codex/<name>.png "<what, period, materials, colours>" [--style <image>]
```

- One object per picture: white background, three-quarter view, whole object visible. The command adds that wording itself.
- For unit gear: `--style` with the unit's art picture, and describe exactly one item.
- It takes about 1 minute. Look at a small preview (`acr-shots trim --max 400`) before spending model credits on it.

## 2a. Meshy (API, paid account; check credits first)

```
acr-genai meshy-balance
acr-genai meshy <ref.png> C:/Users/verwalter/WIP/ACR-analysis/genai/meshy/<name>.glb --poly 25000
```

- **Cost:** about 30 credits per model; generation takes 2–5 minutes.
- **Triangle budget (`--poly`):** gear 3–8k, props 10–25k, trees 30–40k.
- **Ownership:** on the paid plan the output is the user's and private. Meshy deletes API outputs after 3 days, so download at once (the command does).

## 2b. Rodin / Hyper3D (browser only, the user is logged in; 0.5 credit per model, 45 credits on the account)

Use the Claude-in-Chrome tools. Find elements by text (`find`), not by fixed coordinates: the layout moves. The flow (German UI):
1. **Open the workspace:** navigate to `https://hyper3d.ai/workspace/rodin?lang=de`. If a cookie banner shows, choose "Einstellungen" → "Nur notwendige".
2. **Upload:** `find` "image-to-3D file input" (a file-type button in the "Image to 3D" card), then `file_upload` the reference PNG. Only files under `genai\` can be uploaded.
3. **If an "Agentic Mode" bar replaced the Generieren button,** close it with its ✕.
4. **Privacy:** the lock left of Generieren must be red (private). Click it once if it is open.
5. **Quality:** below the button it must read `Gen-2.5` and `High`. If a "Welche Rechenintensität" picker opens, choose "High", then "Auswählen". If a dropdown stays open, click "High" in it, then move the mouse away.
6. **Generate:** `find` "Generieren button" and click it. The button turns into "Vorbereitung", then "Generierung läuft". Geometry takes about 60–90 s; wait in 10 s steps.
7. **Geometry view (normal-map colours):**
   - dismiss the tutorial tips ("Weiter"/"Done");
   - click **Bestätigen**;
   - in the "Confirm via Triangular" dialog click **"Vierecke beibehalten"** (keep quads; triangles go up to 1M, far too heavy);
   - the refinement runs about 60 s.
8. **Material:** in the "Material" panel click **Erzeugen**. The PBR material reaches 95% in about 30 s, then waits up to about 1 more minute. When the textured preview shows, click **Bestätigen**.
9. **Export:**
   - format `.glb`; materials "Shaded" and "PBR" ticked; 2K;
   - click **Herunterladen**. A zip lands in `C:\Users\verwalter\Downloads\<task-uuid>.zip` with `base_basic_pbr.glb` and `base_basic_shaded.glb`;
   - unzip `base_basic_pbr.glb` to `genai\rodin\<name>.glb` and write `<name>.json` (time, credits, task URL).
10. **Next object:** navigate to step 1's URL again.

**Credits:** read the counter at the top right before and after each model, and stop if it would fall below what the task allows.

## Rules

- **Inputs:** only our own Codex pictures, our renders or period images we may use; never the original game's sprites to services other than Codex.
- **Keys:** the API keys in `~/.config/<service>/api_key` are never printed, logged, committed or sent anywhere but that service.
- **Generations:** one per object and service, unless the result is technically broken (then one retry, noted in the json).
- **Report:** a table of name, service, seconds, credits, triangles and path, plus one trimmed preview sheet (`acr-shots sheet`) of the new models when a render is asked for.
