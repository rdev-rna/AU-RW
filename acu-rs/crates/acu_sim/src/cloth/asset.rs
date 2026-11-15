//! Solver assembly from a decoded `Cloth` LOD blob (`acu_assets::cloth`, ACU-RE-0081).
//!
//! Shared by the viewer (`acu_bevy::cloth`) and the live replay (`acu_tap_bridge::cloth`) so
//! both run the same solver. Native branches this port does not cover are refused here, not
//! approximated.

use super::{Cloth, DistanceSet, Integration, ParticleValues, State, collision};
use acu_assets::cloth::{Blob, ByteMap};
use glam::{Vec3, Vec4};

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

pub fn values(m: &ByteMap) -> ParticleValues {
    ParticleValues::from_bytes(&m.bytes, m.scale, m.bias, m.threshold)
}

/// The solver at the blob's rest pose (positions = previous = targets), without colliders.
pub fn solver(blob: &Blob) -> Result<Cloth, String> {
    let n = blob.particle_count().map_err(err)?;
    let cap = (n + 15) & !15;
    let flags = blob.flags().map_err(err)?;
    // Native branches this port does not cover (offsets from body +96).
    if flags[22] != 0 {
        return Err("cloth: LOD damping branch (body +118) is not ported".into());
    }
    if flags[29] != 0 {
        return Err("cloth: damping-map integrator (body +125) is not ported".into());
    }
    if flags[7] != 1 {
        return Err("cloth: 4-wide tether mode (body +103 != 1) is not ported".into());
    }
    let settings = blob.settings().map_err(err)?;
    let maps = blob.particle_maps().map_err(err)?;
    let gravity_scale = if flags[28] != 0 {
        let m = maps[0]
            .as_ref()
            .ok_or("cloth: body +124 set without map 0")?;
        Some(values(m).value[..n].to_vec())
    } else {
        None
    };
    let distance = blob.distance_sets().map_err(err)?.map(|s| {
        let v = ParticleValues::from_bytes(&s.rest_q, s.scale, s.offset, s.threshold);
        DistanceSet {
            pairs: s.pairs,
            rest: v.value,
            active: v.enabled,
            simd_count: s.simd_count,
            k_min: s.k_min,
            k_max: s.k_max,
            stiffness: s.stiffness,
        }
    });
    let pc = blob.particle_constraints().map_err(err)?;
    let particles: Vec<Vec4> = blob
        .particles()
        .map_err(err)?
        .iter()
        .map(|p| Vec4::from_array(*p))
        .collect();
    let cloth = Cloth {
        state: State {
            pos: particles.clone(),
            prev: particles.clone(),
            target: particles,
        },
        normals: vec![Vec4::X; cap],
        integration: Integration {
            gravity: Vec3::from_array(settings.gravity).extend(0.0),
            damping: settings.damping,
            max_speed: settings.max_speed,
            lod_damping: None,
        },
        gravity_scale,
        distance,
        backstop: values(&pc.backstop),
        max_distance: values(&pc.max_distance),
        tether: values(&pc.tether),
        anchors: pc.anchors,
        triangles: blob.triangles().map_err(err)?,
        schedule: blob.schedule().map_err(err)?,
        blend: settings.blend,
        drag: (Vec4::ZERO, settings.drag.a, settings.drag.b),
        shapes: Vec::new(),
        collision: None,
        skipped_collision: 0,
    };
    cloth.validate()?;
    Ok(cloth)
}

/// Opcode 7 inputs from the blob: `Some` when the settings select rigid colliders on the
/// swept path (S+7, S+192); an error for the non-swept path, which is not ported.
pub fn collision_params(blob: &Blob) -> Result<Option<collision::Params>, String> {
    let n = blob.particle_count().map_err(err)?;
    let settings = blob.settings().map_err(err)?;
    let cs = settings.collision;
    if cs.rigid && !cs.swept {
        return Err("cloth: the non-swept collision path is not ported".into());
    }
    if !cs.rigid {
        return Ok(None);
    }
    let flags = blob.flags().map_err(err)?;
    let maps = blob.particle_maps().map_err(err)?;
    let map = |k: usize| maps[k].as_ref().map(|m| values(m).value[..n].to_vec());
    Ok(Some(collision::Params {
        thickness: settings.thickness,
        friction: cs.friction,
        thickness_factor: if flags[30] != 0 { map(2) } else { None },
        friction_table: if flags[31] != 0 { map(3) } else { None },
    }))
}
