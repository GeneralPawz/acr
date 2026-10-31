"""The musket reload drill (#255; pure Python, no Blender):
`python -m unittest discover -s assets/blender/tests`."""

import os
import sys
import unittest

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "lib"))

from acr_assets import poses, rig  # noqa: E402

# EngM18's lines (frame counts as in ENGM18.MD; the frame lists cut short).
MD = {"STAND": "#STAND 9 1 5 0", "ATTACK": "@ATTACK 9 6 0 70", "ATTACK1": "@ATTACK1 9 3 0 86",
      "PSTAND3": "#PSTAND3 9 1 3 130", "PATTACK3": "#PATTACK3 9 56 5 0", "UATTACK3": "#UATTACK3 9 56 3 130",
      "ATTACK3": "#ATTACK3 9 88 3 87", "TRANSX3": "#TRANSX3 9 20 3 149", "TRANS3X": "#TRANS3X 9 20 3 130"}
AIM, STAND = {"aim": 1}, {"stand": 1}


class ReloadDrill(unittest.TestCase):
    def test_fast_musketeer_reloads_in_attack1(self):
        md = {k: v for k, v in MD.items() if not k.endswith("3") and "TRANS" not in k}
        plan = poses.reload_plan(md)
        self.assertEqual(list(plan), ["ATTACK1"])
        clips = poses.musketeer_clips(md, aim=AIM)
        self.assertIs(clips["ATTACK1"][-1][1], AIM)
        self.assertNotIn("ATTACK3", clips)

    def test_clips_cover_their_frames(self):
        clips = poses.musketeer_clips(MD, aim=AIM, stand=STAND)
        for clip in ("ATTACK1", "ATTACK3", "PSTAND3", "PATTACK3", "UATTACK3", "TRANSX3", "TRANS3X"):
            n = rig.md_clip_frames(MD[clip])[0]
            frames = [f for f, _ in clips[clip]]
            self.assertEqual(frames[0], 0, clip)
            self.assertEqual(frames[-1], n - 1, clip)
            self.assertEqual(frames, sorted(set(frames)), clip)

    def test_poses_chain_as_the_sim_plays_them(self):
        """ATTACK1 -> ATTACK3 (looped) -> TRANS3X -> ATTACK1; STAND -> PATTACK3 -> ...
        -> UATTACK3 -> STAND; aim -> TRANSX3 -> stance 4 (combat.rs, motion::try_to_stand)."""
        reload = {k: {"step": k} for k in poses.RELOAD_STEPS}
        c = poses.musketeer_clips(MD, aim=AIM, stand=STAND, reload=reload)
        hold = reload[poses.RAM_HOLD]
        self.assertIs(c["ATTACK1"][-1][1], hold)
        self.assertIs(c["ATTACK3"][0][1], hold)
        self.assertIs(c["ATTACK3"][-1][1], hold)
        self.assertIs(c["PSTAND3"][0][1], hold)
        self.assertIs(c["TRANS3X"][0][1], hold)
        self.assertIs(c["TRANS3X"][-1][1], AIM)
        self.assertIs(c["ATTACK1"][0][1], AIM)
        self.assertIs(c["TRANSX3"][0][1], AIM)
        self.assertIs(c["TRANSX3"][-1][1], hold)
        self.assertIs(c["PATTACK3"][0][1], STAND)
        self.assertIs(c["PATTACK3"][-1][1], hold)
        self.assertIs(c["UATTACK3"][-1][1], STAND)
        # FIRE_HIP ends in the stance too.
        c = poses.musketeer_clips(MD, aim=AIM, stand=STAND, reload=reload, hip=dict(AIM))
        self.assertIs(c["FIRE_HIP"][-1][1], hold)

    def test_drill_order(self):
        steps = [s for _f, s in poses.reload_plan(MD)["ATTACK1"][0]]
        order = ["half_cock", "bite", "pan", "cast", "pour", "grasp_rod", "rod_out", "ram_up", "ram_down"]
        self.assertEqual([s for s in dict.fromkeys(steps) if s in order], order)
        self.assertEqual(steps[0], "prime")
        frames = [f for f, _ in poses.reload_plan(MD)["ATTACK1"][0]]
        self.assertGreater(frames[0], poses.SHOT_FRAME + 3)
        # ATTACK3 rams three times a play; TRANS3X returns the rod before it presents.
        a3 = [s for _f, s in poses.reload_plan(MD)["ATTACK3"][0]]
        self.assertEqual(a3.count("ram_down"), 3)
        t = [s for _f, s in poses.reload_plan(MD)["TRANS3X"][0]]
        self.assertLess(t.index("return_rod"), t.index("poise"))

    def test_short_and_long_attack1_keep_the_whole_drill(self):
        """DelS15's ATTACK1 has 49 frames, SpnS15's 109: every step and the drawn rammer
        fit before the last frame, on distinct frames (`poses._fit`)."""
        full = poses.reload_plan(MD)["ATTACK1"]
        for n in (49, 81, 87, 109):
            keys, rod = poses.reload_plan(dict(MD, ATTACK1=f"@ATTACK1 9 3 0 {n - 1}"))["ATTACK1"]
            frames = [f for f, _ in keys]
            self.assertEqual(frames, sorted(set(frames)), n)
            self.assertEqual(frames[-1], n - 1, n)
            self.assertEqual(len(keys), len(full[0]), n)
            self.assertEqual([m for _f, m in rod], [m for _f, m in full[1]], n)
            self.assertLess(rod[-1][0], n - 1, n)
        self.assertEqual(full[0][:-1], list(poses.SLOW_ATTACK1))

    def test_rod_drawn_only_between_its_pipes(self):
        rods = poses.rod_phases(MD)
        self.assertEqual(set(rods), {"ATTACK1", "FIRE_AIMED", "FIRE_HIP", "ATTACK3", "PSTAND3", "PATTACK3",
                                     "UATTACK3", "TRANSX3", "TRANS3X"})
        self.assertEqual(rods["ATTACK1"][0][1], "stowed")
        self.assertEqual(rods["ATTACK1"][-1][1], "bore")
        self.assertEqual({m for _f, m in rods["ATTACK3"]}, {"bore"})
        self.assertEqual(rods["TRANS3X"][-1][1], "stowed")
        self.assertEqual(rods["UATTACK3"][-1][1], "stowed")
        for clip in ("ATTACK1", "TRANS3X", "TRANSX3", "PATTACK3", "UATTACK3"):
            frames = [f for f, _ in rods[clip]]
            self.assertEqual(frames, sorted(frames), clip)

    def test_reload_time_matches_the_sim(self):
        """EngM18: ATTACK_PAUSE 1 450, personal delay 100: five plays of ATTACK3 (88 frames)
        take it to 10 <= 88, then the shot (combat.rs, the slow-recharge branch)."""
        delay, plays = 450, 0
        frames = rig.md_clip_frames(MD["ATTACK3"])[0]
        while delay > frames:
            delay -= frames
            plays += 1
        self.assertEqual(plays, 5)


class TrapperReload(unittest.TestCase):
    """The trapper's reload (#347): the short rifle's own stance clips and steps."""

    TRAPPER = {"ATTACK1": "@ATTACK1 9 3 0 86", "PSTAND3": "#PSTAND3 9 1 3 87", "ATTACK3": "#ATTACK3 9 84 3 87",
               "TRANS3X": "#TRANS3X 9 21 3 129", "TRANSX3": "#TRANSX3 9 21 3 149"}

    def test_attack3_rams_twice_on_the_low_stroke(self):
        plan = poses.reload_plan(self.TRAPPER, "trapper")
        steps = [s for _f, s in plan["ATTACK3"][0]]
        self.assertEqual(steps, ["ram_down", "ram_up", "ram_down", "ram_up", "ram_down"])
        # ATTACK1 and the stance clips rest on the low stroke; the musketeer's plan is untouched.
        self.assertEqual(plan["ATTACK1"][0][-1][1], "ram_down")
        self.assertEqual(plan["PSTAND3"][0][0][1], "ram_down")
        self.assertEqual(plan["TRANS3X"][0][0][1], "ram_down")
        musketeer = poses.reload_plan(self.TRAPPER)
        self.assertEqual(musketeer["ATTACK1"][0][-1][1], poses.RAM_HOLD)
        self.assertEqual([s for _f, s in musketeer["ATTACK3"][0]].count("ram_up"), 4)

    def test_clips_use_the_style(self):
        reload = {k: {"step": k} for k in poses.RELOAD_STEPS}
        c = poses.musketeer_clips(self.TRAPPER, aim=AIM, stand=STAND, reload=reload, style="trapper")
        self.assertIs(c["ATTACK3"][0][1], reload["ram_down"])
        self.assertIs(c["ATTACK1"][-1][1], reload["ram_down"])

    def test_steps_follow_the_short_gun(self):
        self.assertEqual(poses.reload_steps(poses.MUSKET_LEN), poses.RELOAD_STEPS)
        short, full = poses.reload_steps(0.95), poses.RELOAD_STEPS
        for name, st in short.items():
            if isinstance(full[name]["left"], float) and full[name]["left"] >= 0.9:
                self.assertLessEqual(st["left"], 0.95 - 0.25, name)  # the left hand stays on the barrel
            if isinstance(full[name]["right"], float) and full[name]["right"] >= 0.9:
                self.assertAlmostEqual(st["right"], full[name]["right"] - 0.67, 3, name)
            if full[name].get("butt"):
                self.assertGreater(st["grip"][1], full[name]["grip"][1], name)  # the butt nearer the man
            self.assertEqual(st["base"]["root"], (0, 0, 0), name)


class MountedReload(unittest.TestCase):
    """The dragoons reload in ATTACK1 itself (ENGD18.MD: "/SLOWRECHARGE", 156 frames)."""

    MD = {"ATTACK1": "@ATTACK1 9 4 0 155"}

    def test_drill_on_attack1(self):
        keys, _rod = poses.mounted_reload_plan(self.MD)
        frames = [f for f, _ in keys]
        self.assertEqual(frames[0], 0)
        self.assertEqual(frames[-1], 155)
        self.assertEqual(frames, sorted(set(frames)))
        self.assertEqual(dict(keys)[poses.MOUNTED_SHOT], "@fire")
        self.assertEqual(keys[-1][1], "@aim")
        names = [s for _f, s in keys]
        for step in ("half_cock", "cartridge", "bite", "pan", "shut", "pour", "rod_out", "return_rod"):
            self.assertIn(step, names)

    def test_rams_once(self):
        keys, rod = poses.mounted_reload_plan(self.MD)
        names = [s for _f, s in keys]
        self.assertEqual(names.count("ram_down"), 1)
        modes = [m for _f, m in rod]
        self.assertEqual((modes[0], modes[-1]), ("stowed", "stowed"))
        kf = {s: f for f, s in keys}
        self.assertTrue(rod[0][0] >= kf["pour"] and rod[-1][0] <= kf["poise"])

    def test_no_attack1(self):
        self.assertIsNone(poses.mounted_reload_plan({"STAND": "#STAND 9 1 0 0"}))


if __name__ == "__main__":
    unittest.main()
