//! Decoded vertex attributes for ACU mesh buffers. See ACU-RE-0011.
//!
//! Layout provenance: AnvilToolkit 8.2.0.7535 classes
//! `AnvilToolkit.FileTypes.AnvilNext.Models.Vertex.ReadFromFile`,
//! `AnvilToolkit.Common.PackedS16Vector4`, `PackedU8Vector4`, `PackedUV` and
//! `AnvilToolkit.FileTypes.AnvilNext.Models.ClusteredMeshData` (decompiled
//! reference: `local/reference-atk/`). Numeric validation against the stored
//! clustered center/half-extent and mesh local extents is recorded in
//! ACU-RE-0011. Positions stay in native axes and units; no conversion,
//! quantization rescale or root transform is applied here.

use crate::{Limits, Result, invalid, reader::Reader};

/// Unity vertex format index 0:
/// `Pos3s_Col1s_Norm3ub_Col1ub_Tan4ub_Binorm4ub_Tex2s_Joint4`.
pub const FORMAT_TAN_JOINT4: u8 = 0;
/// Unity vertex format index 1:
/// `Pos3s_Col1s_Norm3ub_Col1ub_Tan4ub_Binorm4ub_Tex2s_Joint8`.
pub const FORMAT_TAN_JOINT8: u8 = 1;
/// Unity vertex format index 4:
/// `Pos3s_Col1s_Norm3ub_Col1ub_Tan4ub_Tex2s`.
pub const FORMAT_TAN: u8 = 4;
/// Unity vertex format index 5:
/// `Pos3s_Col1s_Norm3ub_Col1ub_Tan4ub_Col4ub_Tex2s`.
pub const FORMAT_TAN_COLOR: u8 = 5;
/// Unity vertex format index 6:
/// `Pos3s_Col1s_Norm3ub_Col1ub_Tex2s`.
pub const FORMAT_PLAIN: u8 = 6;

/// Attribute layout recovered from the reference decoder's `ReadFromFile`
/// switch. `joints` is the number of packed joint slots (0, 4 or 8); the
/// buffer stores `joints` index bytes followed by `joints` weight bytes.
///
/// Lookup is keyed by `(format, stride)`: the stride disambiguates variants
/// whose attribute list differs from the reference table (ACU_BaseSkeleton's
/// sibling meshes use format 5 with stride 28, not 24; see ACU-RE-0011).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Layout {
    pub format: u8,
    pub name: &'static str,
    pub stride: u8,
    pub tangent: bool,
    pub binormal: bool,
    pub joints: u8,
    /// Extra uninterpreted `Col4ub` slot.
    pub color: bool,
}

pub const LAYOUTS: &[Layout] = &[
    Layout {
        format: FORMAT_TAN_JOINT4,
        name: "Pos3s_Col1s_Norm3ub_Col1ub_Tan4ub_Binorm4ub_Tex2s_Joint4",
        stride: 32,
        tangent: true,
        binormal: true,
        joints: 4,
        color: false,
    },
    Layout {
        format: FORMAT_TAN_JOINT8,
        name: "Pos3s_Col1s_Norm3ub_Col1ub_Tan4ub_Binorm4ub_Tex2s_Joint8",
        stride: 40,
        tangent: true,
        binormal: true,
        joints: 8,
        color: false,
    },
    Layout {
        format: FORMAT_TAN,
        name: "Pos3s_Col1s_Norm3ub_Col1ub_Tan4ub_Tex2s",
        stride: 20,
        tangent: true,
        binormal: false,
        joints: 0,
        color: false,
    },
    Layout {
        format: FORMAT_TAN_COLOR,
        name: "Pos3s_Col1s_Norm3ub_Col1ub_Tan4ub_Col4ub_Tex2s",
        stride: 24,
        tangent: true,
        binormal: false,
        joints: 0,
        color: true,
    },
    Layout {
        format: FORMAT_TAN_COLOR,
        name: "Pos3s_Col1s_Norm3ub_Col1ub_Tan4ub_Binorm4ub_Col4ub_Tex2s",
        stride: 28,
        tangent: true,
        binormal: true,
        joints: 0,
        color: true,
    },
    Layout {
        format: FORMAT_PLAIN,
        name: "Pos3s_Col1s_Norm3ub_Col1ub_Tex2s",
        stride: 16,
        tangent: false,
        binormal: false,
        joints: 0,
        color: false,
    },
];

pub fn layout(format: u8, stride: u8) -> Option<&'static Layout> {
    LAYOUTS
        .iter()
        .find(|l| l.format == format && l.stride == stride)
}

/// Layout candidates for a format, for diagnostics when the stride mismatches.
pub fn stride_candidates(format: u8) -> Vec<u8> {
    LAYOUTS
        .iter()
        .filter(|l| l.format == format)
        .map(|l| l.stride)
        .collect()
}

/// One decoded vertex. Vector semantics are the reference decoder's; raw
/// packed fields are preserved where their meaning is not established.
#[derive(Debug, Clone, PartialEq)]
pub struct Vertex {
    /// Native-axis position: `i16 / (32767 * sign(w)) * quantization_factor`.
    pub position: [f32; 3],
    /// Packed position `w` (i16). Its sign selects the position scale.
    pub position_w: i16,
    /// `(b - 127) / 127` per byte, clamped above at 1.0.
    pub normal: [f32; 3],
    /// Byte `w` of the packed normal vector, preserved.
    pub normal_w: u8,
    pub tangent: Option<[f32; 3]>,
    pub tangent_w: Option<u8>,
    pub binormal: Option<[f32; 3]>,
    pub binormal_w: Option<u8>,
    /// `i16 / 2048` per coordinate.
    pub uv0: Option<[f32; 2]>,
    /// Format-5 `Col4ub` bytes, preserved without interpretation.
    pub color: Option<[u8; 4]>,
    /// Packed joint indices (`joints` bytes) and weights (`joints` bytes).
    pub joints: Option<Vec<u8>>,
    pub joint_weights: Option<Vec<u8>>,
}

fn signed_unit(value: i16) -> f32 {
    if value < 0 { -32767.0 } else { 32767.0 }
}

fn packed_normal(r: &mut Reader<'_>) -> Result<([f32; 3], u8)> {
    let b = r.take(4)?;
    let component = |i: usize| ((b[i] as f32) / 127.0 - 1.0).min(1.0);
    Ok(([component(0), component(1), component(2)], b[3]))
}

/// Decode a packed vertex buffer with a recovered Unity layout.
///
/// `scale` is the owning `CompiledMesh` quantization factor; native position is
/// `i16 / (32767 * sign(w)) * scale` (validated in ACU-RE-0011 against stored
/// cluster bounds). Fails clearly when the format/stride pair is unknown.
pub fn decode(
    bytes: &[u8],
    format: u8,
    stride: u8,
    scale: f32,
    limits: Limits,
) -> Result<Vec<Vertex>> {
    let layout = layout(format, stride).ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!(
                "unsupported vertex format {format} stride {stride} (known strides: {:?})",
                stride_candidates(format)
            ),
        )
    })?;
    if !scale.is_finite() || scale <= 0.0 {
        return invalid("mesh quantization factor is not positive finite");
    }
    if !bytes.len().is_multiple_of(stride as usize) {
        return invalid("vertex buffer length is not a multiple of stride");
    }
    let count = bytes.len() / stride as usize;
    if count > limits.max_resources {
        return invalid("vertex count exceeds configured limit");
    }
    let mut r = Reader::new(bytes);
    let mut vertices = Vec::with_capacity(count);
    for _ in 0..count {
        let x = r.i16()?;
        let y = r.i16()?;
        let z = r.i16()?;
        let w = r.i16()?;
        let scale_sign = signed_unit(w);
        let (normal, normal_w) = packed_normal(&mut r)?;
        let (tangent, tangent_w, binormal, binormal_w) = if layout.tangent {
            let (t, tw) = packed_normal(&mut r)?;
            let (b, bw) = if layout.binormal {
                let (b, bw) = packed_normal(&mut r)?;
                (Some(b), Some(bw))
            } else {
                (None, None)
            };
            (Some(t), Some(tw), b, bw)
        } else {
            (None, None, None, None)
        };
        let color = if layout.color {
            Some(r.take(4)?.try_into()?)
        } else {
            None
        };
        let uv0 = Some([r.i16()? as f32 / 2048.0, r.i16()? as f32 / 2048.0]);
        let (joints, joint_weights) = if layout.joints > 0 {
            let n = layout.joints as usize;
            (Some(r.take(n)?.to_vec()), Some(r.take(n)?.to_vec()))
        } else {
            (None, None)
        };
        vertices.push(Vertex {
            position: [
                x as f32 / scale_sign * scale,
                y as f32 / scale_sign * scale,
                z as f32 / scale_sign * scale,
            ],
            position_w: w,
            normal,
            normal_w,
            tangent,
            tangent_w,
            binormal,
            binormal_w,
            uv0,
            color,
            joints,
            joint_weights,
        });
    }
    Ok(vertices)
}

/// Decoded geometry ready for presentation: native-axis positions, first UV
/// channel, and triangle-list indices from the recovered primitive spans.
#[derive(Debug)]
pub struct Geometry {
    pub vertices: Vec<Vertex>,
    pub triangles: Vec<[u32; 3]>,
    /// `"mesh_data"` or `"clustered"`; names the buffer that carried geometry.
    pub vertex_source: &'static str,
    pub index_source: &'static str,
    /// `"standard"` or `"shadow"`: which primitive list described topology.
    pub topology_source: &'static str,
}

impl Geometry {
    pub fn bounds(&self) -> ([f32; 3], [f32; 3]) {
        let mut min = [f32::INFINITY; 3];
        let mut max = [f32::NEG_INFINITY; 3];
        for v in &self.vertices {
            for i in 0..3 {
                min[i] = min[i].min(v.position[i]);
                max[i] = max[i].max(v.position[i]);
            }
        }
        (min, max)
    }

    /// Longest axis extent, usable for framing a camera.
    pub fn extent(&self) -> f32 {
        let (min, max) = self.bounds();
        (max[0] - min[0]).max(max[1] - min[1]).max(max[2] - min[2])
    }
}
