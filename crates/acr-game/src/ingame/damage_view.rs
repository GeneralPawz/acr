//! What the renderer draws of a body (#262 S5, `docs/damage-model.md` §3): the state of
//! `acr_sim::body` turned into an `acr_render::damage::Damage`, and the clip of a crawler.
//! Pure functions, render only.

use acr_render::damage::{Damage, Fracture, Keep, Limb, LimbDamage};
use acr_sim::body::{Body, FractureKind, LimbId, Torn};

/// Smallest bend drawn for a break or dislocation (degrees): the sim's bend can be small.
const MIN_BEND: f32 = 25.0;
/// A dislocated joint bends this much more than a broken bone, and sideways.
const DISLOCATED: f32 = 1.5;

/// The damage a body shows: cut limbs, bent fractures, an open belly, and from a torn
/// death (`DeathRecord::torn`) the trunk cut or the missing head. `None`: nothing whole is
/// left (pulverized), the body is not drawn at all, only debris and mist.
pub fn damage(body: &Body, torn: Option<Torn>) -> Option<Damage> {
    let mut d = Damage { belly_open: body.belly_open, ..Damage::default() };
    for (i, limb) in body.limbs.iter().enumerate() {
        let l = Limb::ALL[i];
        *d.limb_mut(l) =
            LimbDamage { cut: limb.lost, fracture: limb.fracture.filter(|f| f.level < limb.lost).map(|f| bend(l, f)) };
    }
    match torn {
        Some(Torn::Pulverized) => return None,
        Some(Torn::Headless) => d.head_gone = true,
        // The sim counts from the shoulders, the renderer from the hips.
        Some(Torn::InTwo { level }) => d.trunk_cut = Some(255 - level),
        None => {}
    }
    Some(d)
}

/// The upper half of a body torn in two (thrown by the blow), or `None` when it is not.
pub fn upper(body: &Damage) -> Option<Damage> {
    body.trunk_cut.map(|_| Damage { keep: Keep::Upper, ..*body })
}

/// A broken limb bent at the break: closed and open fractures fold forwards (about the rest
/// pose's x axis), a dislocated joint further and sideways, outwards for each side.
fn bend(limb: Limb, f: acr_sim::body::Fracture) -> Fracture {
    let sign = if f.bend < 0 { -1.0 } else { 1.0 };
    let degrees = sign * (f32::from(f.bend) * 2.0).abs().max(MIN_BEND);
    let left = matches!(limb, Limb::ArmL | Limb::LegL);
    let (axis, degrees) = match f.kind {
        FractureKind::Closed | FractureKind::Open => ([1.0, 0.0, 0.0], degrees),
        FractureKind::Dislocated => {
            // Outwards: about the forward axis, the left limb one way, the right the other.
            let side = if left { -1.0 } else { 1.0 };
            ([0.0, 0.0, 1.0], side * (degrees.abs() * DISLOCATED).min(100.0))
        }
    };
    Fracture { level: f.level, axis, angle: degrees.to_radians() }
}

/// The clip of a crawler (`docs/assets-humans.md`): `CRAWL2` with both arms pulling and the
/// legs dragged, `CRAWL1` with one leg still pushing, `CRAWLARM` with one working arm,
/// `WRITHE` when he cannot move at all.
pub fn crawl_clip(body: &Body) -> &'static str {
    let legs = [LimbId::LegL, LimbId::LegR].into_iter().filter(|&l| body.leg(l) > 0).count();
    match body.working_arms() {
        0 => "WRITHE",
        1 => "CRAWLARM",
        _ if legs == 1 => "CRAWL1",
        _ => "CRAWL2",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use acr_sim::body::{self, Limb as SimLimb};

    fn broken(kind: FractureKind, bend: i8) -> SimLimb {
        SimLimb { fracture: Some(body::Fracture { level: 160, kind, bend }), ..SimLimb::default() }
    }

    #[test]
    fn whole_body_draws_whole() {
        assert_eq!(damage(&Body::default(), None), Some(Damage::default()));
    }

    #[test]
    fn lost_limbs_are_cut_and_belly_flagged() {
        let mut b = Body { belly_open: true, ..Body::default() };
        b.limbs[LimbId::LegR as usize].lost = 70;
        let d = damage(&b, None).unwrap();
        assert_eq!(d.limb(Limb::LegR).cut, 70);
        assert_eq!(d.limb(Limb::ArmL).cut, 255);
        assert!(d.belly_open);
    }

    #[test]
    fn fractures_bend_dislocations_more_and_sideways() {
        let mut b = Body::default();
        b.limbs[0] = broken(FractureKind::Closed, 10);
        b.limbs[1] = broken(FractureKind::Dislocated, 30);
        b.limbs[2] = broken(FractureKind::Dislocated, 30);
        let d = damage(&b, None).unwrap();
        let closed = d.limb(Limb::ArmL).fracture.unwrap();
        assert_eq!(closed.axis, [1.0, 0.0, 0.0]);
        assert!((closed.angle.to_degrees() - MIN_BEND).abs() < 0.01);
        let (r, l) = (d.limb(Limb::ArmR).fracture.unwrap(), d.limb(Limb::LegL).fracture.unwrap());
        assert_eq!(r.axis, [0.0, 0.0, 1.0]);
        assert!((r.angle.to_degrees() - 90.0).abs() < 0.01);
        assert!((l.angle + r.angle).abs() < 1e-6, "left and right bend outwards");
    }

    #[test]
    fn a_fracture_on_the_lost_part_is_not_drawn() {
        let mut b = Body::default();
        b.limbs[0] = SimLimb { lost: 120, ..broken(FractureKind::Open, 20) };
        assert_eq!(damage(&b, None).unwrap().limb(Limb::ArmL).fracture, None);
    }

    #[test]
    fn torn_bodies() {
        let b = Body::default();
        assert_eq!(damage(&b, Some(Torn::Pulverized)), None);
        assert!(damage(&b, Some(Torn::Headless)).unwrap().head_gone);
        let d = damage(&b, Some(Torn::InTwo { level: 200 })).unwrap();
        assert_eq!(d.trunk_cut, Some(55));
        assert_eq!(upper(&d).unwrap().keep, Keep::Upper);
        assert_eq!(upper(&Damage::default()), None);
    }

    #[test]
    fn crawl_clips_follow_the_working_limbs() {
        let mut b = Body::default();
        b.limbs[LimbId::LegL as usize].lost = 60;
        b.limbs[LimbId::LegR as usize].lost = 60;
        assert_eq!(crawl_clip(&b), "CRAWL2");
        b.limbs[LimbId::ArmL as usize].lost = 60;
        assert_eq!(crawl_clip(&b), "CRAWLARM");
        b.limbs[LimbId::ArmR as usize].lost = 60;
        assert_eq!(crawl_clip(&b), "WRITHE");
        let mut one_leg = Body::default();
        one_leg.limbs[LimbId::LegL as usize].lost = 60;
        assert_eq!(crawl_clip(&one_leg), "CRAWL1");
    }
}
