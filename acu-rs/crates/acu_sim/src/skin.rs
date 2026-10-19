//! Full eight-influence linear blend skinning, independent of the renderer.
use glam::{Mat4, Quat, Vec3};

#[derive(Clone, Copy, Debug)]
pub struct Influence {
    pub joints: [usize; 8],
    pub weights: [u8; 8],
}

pub struct Rig {
    pub bind: Vec<Mat4>,
    inverse_bind: Vec<Mat4>,
    parents: Vec<Option<usize>>,
    order: Vec<usize>,
}

impl Rig {
    pub fn new(bind: Vec<Mat4>, parents: Vec<Option<usize>>) -> Result<Self, String> {
        if bind.is_empty() || bind.len() != parents.len() || bind.len() > 4096 {
            return Err("invalid rig dimensions".into());
        }
        let mut order = Vec::new();
        let mut state = vec![0u8; bind.len()];
        fn visit(
            i: usize,
            p: &[Option<usize>],
            s: &mut [u8],
            o: &mut Vec<usize>,
        ) -> Result<(), String> {
            if i >= s.len() {
                return Err("parent outside rig".into());
            }
            if s[i] == 1 {
                return Err("cyclic rig".into());
            }
            if s[i] == 2 {
                return Ok(());
            }
            s[i] = 1;
            if let Some(parent) = p[i] {
                visit(parent, p, s, o)?;
            }
            s[i] = 2;
            o.push(i);
            Ok(())
        }
        for i in 0..bind.len() {
            visit(i, &parents, &mut state, &mut order)?;
        }
        if bind
            .iter()
            .any(|m| !m.is_finite() || m.determinant().abs() < 1e-8)
        {
            return Err("invalid bind matrix".into());
        }
        let inverse_bind = bind.iter().map(|m| m.inverse()).collect();
        Ok(Self {
            bind,
            inverse_bind,
            parents,
            order,
        })
    }

    /// Authored rotation in mesh axes at each bind pivot. Parents carry every
    /// descendant, including joints without mesh weights. Native solver absent.
    pub fn matrices(&self, rotations: &[Quat]) -> Vec<Mat4> {
        assert_eq!(rotations.len(), self.bind.len());
        let mut posed = vec![Mat4::IDENTITY; self.bind.len()];
        for &i in &self.order {
            let p = self.bind[i].transform_point3(Vec3::ZERO);
            let rotation = Mat4::from_translation(p)
                * Mat4::from_quat(rotations[i])
                * Mat4::from_translation(-p);
            let parent = self.parents[i].map(|p| posed[p]).unwrap_or(Mat4::IDENTITY);
            // Explicit bind/inverse-bind multiplication: identity at rest.
            posed[i] = parent * rotation * self.bind[i] * self.inverse_bind[i];
        }
        posed
    }
}

impl Influence {
    pub fn validate(&self, joint_count: usize) -> Result<(), String> {
        if self.weights.iter().map(|&w| u32::from(w)).sum::<u32>() != 255 {
            return Err("weights must sum to 255".into());
        }
        if self
            .joints
            .iter()
            .zip(self.weights)
            .any(|(&j, w)| w != 0 && j >= joint_count)
        {
            return Err("weighted joint outside rig".into());
        }
        Ok(())
    }
    pub fn deform(&self, p: Vec3, n: Vec3, matrices: &[Mat4]) -> (Vec3, Vec3) {
        let mut position = Vec3::ZERO;
        let mut normal = Vec3::ZERO;
        for (&j, w) in self.joints.iter().zip(self.weights) {
            if w == 0 {
                continue;
            }
            let weight = f32::from(w) / 255.0;
            position += matrices[j].transform_point3(p) * weight;
            normal += matrices[j].transform_vector3(n) * weight;
        }
        (position, normal.normalize_or_zero())
    }
}

pub struct Surface {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub tangents: Vec<[f32; 4]>,
}

/// Optimized vertex loop, independent of the viewer and preserving eight slots.
pub fn surface(
    vertices: &[(Vec3, Vec3)],
    tangents: &[(Vec3, f32)],
    weights: &[Influence],
    matrices: &[Mat4],
) -> Surface {
    assert_eq!(vertices.len(), weights.len());
    assert_eq!(vertices.len(), tangents.len());
    let mut result = Surface {
        positions: Vec::with_capacity(vertices.len()),
        normals: Vec::with_capacity(vertices.len()),
        tangents: Vec::with_capacity(vertices.len()),
    };
    for ((&(p, n), w), &(t, sign)) in vertices.iter().zip(weights).zip(tangents) {
        let (p, n) = w.deform(p, n, matrices);
        let (_, t) = w.deform(Vec3::ZERO, t, matrices);
        let t = (t - n * t.dot(n)).normalize_or_zero();
        result.positions.push(p.to_array());
        result.normals.push(n.to_array());
        result.tangents.push([t.x, t.y, t.z, sign]);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn eight_weights_bind_identity_and_pivot_rotation() {
        let rig = Rig::new(
            vec![Mat4::IDENTITY, Mat4::from_translation(Vec3::X)],
            vec![None, Some(0)],
        )
        .unwrap();
        let weights = Influence {
            joints: [0, 1, 0, 1, 0, 1, 0, 1],
            weights: [32, 32, 32, 32, 32, 32, 32, 31],
        };
        weights.validate(2).unwrap();
        let p = Vec3::new(2.0, 0.0, 0.0);
        let (rest, _) = weights.deform(p, Vec3::Y, &rig.matrices(&[Quat::IDENTITY; 2]));
        assert!((rest - p).length() < 1e-6);
        let m = rig.matrices(&[
            Quat::IDENTITY,
            Quat::from_rotation_z(std::f32::consts::FRAC_PI_2),
        ]);
        let (posed, _) = weights.deform(p, Vec3::Y, &m);
        // 128/255 stays at (2,0); 127/255 rotates to (1,1).
        assert!((posed - Vec3::new(383.0 / 255.0, 127.0 / 255.0, 0.0)).length() < 1e-6);
        let s = surface(&[(p, Vec3::Y)], &[(Vec3::X, -1.0)], &[weights], &m);
        assert!((Vec3::from_array(s.positions[0]) - posed).length() < 1e-6);
        assert_eq!(s.tangents[0][3], -1.0);
        assert!(
            Vec3::from_array(s.normals[0])
                .dot(Vec3::new(
                    s.tangents[0][0],
                    s.tangents[0][1],
                    s.tangents[0][2]
                ))
                .abs()
                < 1e-6
        );
        let mut bad = weights;
        bad.joints[7] = 2;
        assert!(bad.validate(2).is_err());
        let mut bad = weights;
        bad.weights[7] = 0;
        assert!(bad.validate(2).is_err());
    }
    #[test]
    fn hierarchy_order_and_invalid_matrices() {
        assert!(Rig::new(vec![Mat4::IDENTITY; 2], vec![Some(1), Some(0)]).is_err());
        assert!(Rig::new(vec![Mat4::ZERO], vec![None]).is_err());
        assert!(Rig::new(vec![Mat4::IDENTITY], vec![Some(1)]).is_err());
        let r = Rig::new(
            vec![Mat4::from_translation(Vec3::X), Mat4::IDENTITY],
            vec![Some(1), None],
        )
        .unwrap();
        let m = r.matrices(&[
            Quat::IDENTITY,
            Quat::from_rotation_z(std::f32::consts::FRAC_PI_2),
        ]);
        assert!((m[0].transform_point3(Vec3::X) - Vec3::Y).length() < 1e-6);
    }
}
