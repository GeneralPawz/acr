"""Spanish reiter (`SpnRei`, `SPNREI.MD`, `SpnRei(SP)` of SPAIN.NDS) on the WildMesh horse
(#287): an armoured horseman, steel cuirass over a buff coat, a combed morion with a red
plume, a pistol (the shot, ATTACK1) and a sabre (the cut, ATTACK), the military saddle with
holsters on a red saddlecloth; a bay (`acr_assets.cavalry`). Colours from the sprites.
The armoured reiter is slow: 9.1 px (0.30 m) a tick, 4.8 m per 16-frame cycle, two trot
strides (4.3 m) at 0.8 of the source's pace.

SpnReiG1/SpnReiG2 (`SPNREIG1.MD`, `SPNREIG2.MD`) are unused leftovers of this unit: no
nation lists them (only `SpnRei(SP)` is in SPAIN.NDS), so the game never makes one (the
catalog has no such type); they swap only the body sprites (`SpnReiG1/G2`), have no LIFE,
DAMAGE 1 and the icon `KIR`. This model is SpnRei's.

Reworked (m2-cav17): a buff coat with buff sleeves under the cuirass and steel arm defences,
falling band, bucket boots, a lobster-pot helmet (`zischagge`) with red and white plumes, a
pair of wheellock pistols (one in the hand, one in the left holster) and a drawn sword for
the cuts.
"""

from acr_assets import cavalry

TYPE = "SpnRei"
CHECKS = ["--anim STAND", "--anim DEATHLIE1"]

MD = {
    "REST": "@REST 9 0 0 85",
    "STAND": "#STAND 9 1 0 0",
    "MOTION_L": "@MOTION_L 9 1 0 6",
    "MOTION_R": "@MOTION_R 9 1 7 15",
    "MOTION_LB": "@MOTION_LB 9 1 15 7",
    "MOTION_RB": "@MOTION_RB 9 1 6 0",
    "PATTACK": "#PATTACK 9 1 3 0",
    "UATTACK": "#UATTACK 9 1 3 0",
    "ATTACK": "@ATTACK 9 3 0 61",
    "PATTACK1": "#PATTACK1 9 1 4 0",
    "UATTACK1": "#UATTACK1 9 1 4 0",
    "ATTACK1": "@ATTACK1 9 4 0 113",
    "DEATH": "@DEATH 9 2 0 37",
    "DEATHLIE1": "#DEATHLIE1 9 1 2 37",
    "PSTAND": "#PSTAND 9 1 3 0",
    "PSTAND1": "#PSTAND1 9 1 4 0",
}

PALETTE = dict(cavalry.PALETTE, **cavalry.PALETTE17, **{"coat": (0.72, 0.62, 0.45), "waistcoat": (0.72, 0.62, 0.45), "metal": (0.62, 0.63, 0.66),
                                   "saddlecloth": (0.70, 0.10, 0.06), "breeches": (0.72, 0.60, 0.42), "crest": (0.92, 0.90, 0.86), "sash": (0.75, 0.08, 0.06)})

SPEC = {"md": MD, "palette": PALETTE, "paint": True, "fixed": cavalry.FIXED17, "dress": "buff17", "sleeves": "buff", "armour": True, "arms": True, "tack": "military", "hat": "zischagge", "plume": True, "held": None,
        "gun": "pistol", "holster": True, "drawn": True, "blade": "sword", "motions": {"": ("G1_Trot", 2)}}


def build(ctx):
    cavalry.build(ctx, SPEC)
