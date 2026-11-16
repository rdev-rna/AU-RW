//! Baked GI containers (ACU-RE-0073): `GILoader` (`9ed472d4`) and `GIDatablock` (`f7d22b3c`).
//!
//! Field order follows the native loaders (vtable slot 2) of the runtime image
//! `ACU.runtime-analysis.pe` (motion-link capture, base `0x140000000`):
//! - `GILoader` `0x1421CE1A0`; `GITreeNode` (`4aa4037e`) `0x1421CE710`;
//!   tree-node link (`a8a5bb0a`) `0x1421CE5F0`; `DataLayerFilter` `0x1426F16A0`,
//!   element (`e31593e1`) `0x1426F1640`;
//! - `GIDatablock` `0x1421CDF00`; `GIResult` (`32ed41b1`) `0x1421CE480`;
//!   `GIData` (`daf9220e`) `0x1421CDD20`.
//!
//! Stream helpers: embedded pointer `0x14270B920` (u8 tag; 0 = u64 id, u32 type,
//! body; 2 = u64 reference; 3 = null), embedded value `0x14270BA90` (u64 id, u32 type,
//! body), handle `0x14270FF90` (u8, u64), inline handle `0x14270E560` (u8 tag, u8,
//! u64, then u32 type and body when the tag is 0), arrays `0x14000B2A0` /
//! `0x1405F08E0` (u32 count).
//!
//! The four GICP blobs of a `GIData` are kept as bytes: their CPU decoder is not
//! recovered, so no irradiance value is produced here. Decoding a container says
//! nothing about which duplicate the game streams or how cells are selected.

use serde::Serialize;

use crate::reader::Reader;
use crate::{Result, invalid};

pub const CLASS_GI_LOADER: u32 = 0x9ed4_72d4;
pub const CLASS_GI_DATABLOCK: u32 = 0xf7d2_2b3c;
const CLASS_TREE_NODE: u32 = 0x4aa4_037e;
const CLASS_TREE_LINK: u32 = 0xa8a5_bb0a;
const CLASS_LAYER_FILTER: u32 = 0xdb1d_406e;
const CLASS_LAYER_FILTER_ITEM: u32 = 0xe315_93e1;
const CLASS_GI_RESULT: u32 = 0x32ed_41b1;
const CLASS_GI_DATA: u32 = 0xdaf9_220e;

/// Defensive bounds, not native constants.
const MAX_DEPTH: usize = 32;
const MAX_NODES: usize = 1_000_000;
const MAX_COUNT: usize = 100_000;
/// Every observed blob starts with these four bytes.
pub const GICP_MAGIC: &[u8; 4] = b"GICP";

pub fn hex16(id: u64) -> String {
    format!("{id:016x}")
}

fn ser_hex<S: serde::Serializer>(id: &u64, s: S) -> std::result::Result<S::Ok, S::Error> {
    s.serialize_str(&hex16(*id))
}

pub(crate) fn f32s<const N: usize>(r: &mut Reader) -> Result<[f32; N]> {
    let mut out = [0.0; N];
    for v in &mut out {
        *v = f32::from_bits(r.u32()?);
    }
    Ok(out)
}

/// Object header of a resource payload: `u8 1, u64 id, u32 type`.
pub(crate) fn root_header(r: &mut Reader, class: u32) -> Result<u64> {
    let tag = r.u8()?;
    if tag != 1 {
        return invalid(format!("root object tag {tag}, expected 1"));
    }
    let id = r.u64()?;
    expect_type(r, class)?;
    Ok(id)
}

pub(crate) fn expect_type(r: &mut Reader, class: u32) -> Result<()> {
    let at = r.position;
    let t = r.u32()?;
    if t != class {
        return invalid(format!("type {t:08x} at {at}, expected {class:08x}"));
    }
    Ok(())
}

fn count(r: &mut Reader) -> Result<usize> {
    let at = r.position;
    let n = r.u32()? as usize;
    if n > MAX_COUNT {
        return invalid(format!("count {n} at {at} exceeds {MAX_COUNT}"));
    }
    Ok(n)
}

/// `0x14270FF90`: u8 kind, u64 id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Handle {
    pub kind: u8,
    #[serde(serialize_with = "ser_hex")]
    pub id: u64,
}

fn handle(r: &mut Reader) -> Result<Handle> {
    Ok(Handle {
        kind: r.u8()?,
        id: r.u64()?,
    })
}

/// One `GITreeNode`. `min`/`size` are the stored vectors (`+20`, `+36`); reading them
/// as an axis-aligned cell corner and edge is an inference from the values.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TreeNode {
    pub min: [f32; 3],
    pub level: u32,
    pub size: [f32; 3],
    pub flag: u8,
    pub handle: Handle,
    pub children: Vec<TreeNode>,
    /// Child slots stored as null pointers (tag 3); the native array keeps them empty.
    pub null_children: usize,
    pub link_value: u64,
    /// `GIResult` id inside the linked datablock (0 when the node has none).
    pub result: Handle,
    /// `GIDatablock` resource id in the `_gi` archive (0 when the node has none).
    pub datablock: Handle,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct GiLoader {
    #[serde(serialize_with = "ser_hex")]
    pub object_id: u64,
    /// `+48`: the time-of-day `Season` this loader belongs to (e.g. `PAR_TOD_Paris_Noon`).
    #[serde(serialize_with = "ser_hex")]
    pub season: u64,
    pub vector80: [f32; 3],
    pub vector92: [f32; 3],
    pub value104: u32,
    pub root: Option<TreeNode>,
    pub vector16: [f32; 4],
    pub vector32: [f32; 4],
    pub layer_filter: Vec<(Handle, u32)>,
    pub node_count: usize,
}

impl GiLoader {
    /// Nodes that link a datablock, depth first.
    pub fn leaves(&self) -> Vec<&TreeNode> {
        fn walk<'a>(n: &'a TreeNode, out: &mut Vec<&'a TreeNode>) {
            if n.datablock.id != 0 {
                out.push(n);
            }
            for c in &n.children {
                walk(c, out);
            }
        }
        let mut out = Vec::new();
        if let Some(root) = &self.root {
            walk(root, &mut out);
        }
        out
    }
}

fn tree_node(r: &mut Reader, depth: usize, nodes: &mut usize) -> Result<Option<TreeNode>> {
    if depth > MAX_DEPTH {
        return invalid(format!("GI tree deeper than {MAX_DEPTH}"));
    }
    let at = r.position;
    match r.u8()? {
        3 => return Ok(None),
        0 => {}
        tag => return invalid(format!("GI tree pointer tag {tag} at {at} unsupported")),
    }
    *nodes += 1;
    if *nodes > MAX_NODES {
        return invalid(format!("GI tree exceeds {MAX_NODES} nodes"));
    }
    r.u64()?;
    expect_type(r, CLASS_TREE_NODE)?;
    let min = f32s::<3>(r)?;
    let level = r.u32()?;
    let size = f32s::<3>(r)?;
    let flag = r.u8()?;
    let node_handle = handle(r)?;
    let n = count(r)?;
    let mut children = Vec::with_capacity(n.min(64));
    let mut null_children = 0;
    for _ in 0..n {
        match tree_node(r, depth + 1, nodes)? {
            Some(c) => children.push(c),
            None => null_children += 1,
        }
    }
    r.u64()?;
    expect_type(r, CLASS_TREE_LINK)?;
    let link_value = r.u64()?;
    let result = handle(r)?;
    let datablock = handle(r)?;
    Ok(Some(TreeNode {
        min,
        level,
        size,
        flag,
        handle: node_handle,
        children,
        null_children,
        link_value,
        result,
        datablock,
    }))
}

/// Decode a `GILoader` payload; it must end exactly at the payload end.
pub fn decode_loader(payload: &[u8]) -> Result<GiLoader> {
    let mut r = Reader::new(payload);
    let object_id = root_header(&mut r, CLASS_GI_LOADER)?;
    let season = r.u64()?;
    let vector80 = f32s::<3>(&mut r)?;
    let vector92 = f32s::<3>(&mut r)?;
    let value104 = r.u32()?;
    let mut node_count = 0;
    let root = tree_node(&mut r, 0, &mut node_count)?;
    let vector16 = f32s::<4>(&mut r)?;
    let vector32 = f32s::<4>(&mut r)?;
    r.u64()?;
    expect_type(&mut r, CLASS_LAYER_FILTER)?;
    let n = count(&mut r)?;
    let mut layer_filter = Vec::with_capacity(n);
    for _ in 0..n {
        r.u64()?;
        expect_type(&mut r, CLASS_LAYER_FILTER_ITEM)?;
        layer_filter.push((handle(&mut r)?, r.u32()?));
    }
    finish(&r, "GILoader")?;
    Ok(GiLoader {
        object_id,
        season,
        vector80,
        vector92,
        value104,
        root,
        vector16,
        vector32,
        layer_filter,
        node_count,
    })
}

fn finish(r: &Reader, what: &str) -> Result<()> {
    if r.remaining() != 0 {
        return invalid(format!(
            "{what} stopped at {} with {} trailing bytes",
            r.position,
            r.remaining()
        ));
    }
    Ok(())
}

/// One `GIData`: four GICP blobs plus the loader's trailing fields.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct GiData {
    #[serde(skip)]
    pub blobs: [Vec<u8>; 4],
    pub blob_lengths: [usize; 4],
    /// GICP header dimensions (`+8..+20`) of each blob.
    pub blob_dims: [[u32; 3]; 4],
    /// `+0` and `+16`, one component per blob. Their role (decode scale / bias for the
    /// `GIDecode*` compute shaders' `Scale[4]` / `Bias[4]`) is an unverified inference.
    pub vector0: [f32; 4],
    pub vector16: [f32; 4],
    pub byte104: u8,
    pub byte105: u8,
    pub value96: u32,
    pub byte106: u8,
    pub value100: u32,
}

fn gicp_dims(blob: &[u8]) -> Result<[u32; 3]> {
    // Observed: some Paris Dawn GIData carry a zero-length blob (2026-11-16, a cell near
    // Arno's anchor). Kept as framing with dims 0; what the engine decodes for it is unknown,
    // so `gicp::decode_cell` rejects such a cell rather than guessing.
    if blob.is_empty() {
        return Ok([0; 3]);
    }
    if blob.len() < 20 || &blob[..4] != GICP_MAGIC {
        let head: String = blob.iter().take(8).map(|b| format!("{b:02x}")).collect();
        return invalid(format!(
            "GIData blob without GICP magic ({} bytes, starts {head})",
            blob.len()
        ));
    }
    let d = |o: usize| u32::from_le_bytes(blob[o..o + 4].try_into().expect("4 bytes"));
    Ok([d(8), d(12), d(16)])
}

fn gi_data(r: &mut Reader) -> Result<GiData> {
    let mut blobs: [Vec<u8>; 4] = Default::default();
    let mut blob_lengths = [0; 4];
    let mut blob_dims = [[0; 3]; 4];
    for i in 0..4 {
        let at = r.position;
        let n = r.u32()? as usize;
        if n > r.remaining() {
            return invalid(format!("GICP blob of {n} bytes at {at} overruns payload"));
        }
        blobs[i] = r.take(n)?.to_vec();
        blob_lengths[i] = n;
        blob_dims[i] = gicp_dims(&blobs[i])?;
    }
    Ok(GiData {
        blobs,
        blob_lengths,
        blob_dims,
        vector0: f32s::<4>(r)?,
        vector16: f32s::<4>(r)?,
        byte104: r.u8()?,
        byte105: r.u8()?,
        value96: r.u32()?,
        byte106: r.u8()?,
        value100: r.u32()?,
    })
}

/// One `GIResult`: a probe cell with its GI payloads.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct GiResult {
    #[serde(serialize_with = "ser_hex")]
    pub object_id: u64,
    pub values: [u32; 3],
    /// Reflected property names `Center` (`+16`) and `Extents` (`+32`). Observed: `Center` equals
    /// the min corner of the GI tree leaf that links this result, and `Extents` its edge length.
    pub center: [f32; 4],
    pub extents: [f32; 4],
    pub vector48: [f32; 4],
    /// `GIConvexIndoorVolume` handle (`+64`).
    pub indoor_volume: Handle,
    pub byte104: u8,
    pub data: Vec<GiData>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct GiDatablock {
    #[serde(serialize_with = "ser_hex")]
    pub object_id: u64,
    #[serde(serialize_with = "ser_hex")]
    pub season: u64,
    pub results: Vec<GiResult>,
}

fn gi_result(r: &mut Reader) -> Result<GiResult> {
    let values = [r.u32()?, r.u32()?, r.u32()?];
    let center = f32s::<4>(r)?;
    let extents = f32s::<4>(r)?;
    let vector48 = f32s::<4>(r)?;
    let indoor_volume = handle(r)?;
    let at = r.position;
    match r.u8()? {
        3 => {}
        tag => {
            return invalid(format!(
                "GIRenderIndoorVolume pointer tag {tag} at {at} unsupported (only null observed)"
            ));
        }
    }
    let byte104 = r.u8()?;
    let n = count(r)?;
    let mut data = Vec::with_capacity(n.min(16));
    for _ in 0..n {
        r.u64()?;
        expect_type(r, CLASS_GI_DATA)?;
        data.push(gi_data(r)?);
    }
    Ok(GiResult {
        object_id: 0,
        values,
        center,
        extents,
        vector48,
        indoor_volume,
        byte104,
        data,
    })
}

/// Decode a `GIDatablock` payload; it must end exactly at the payload end.
pub fn decode_datablock(payload: &[u8]) -> Result<GiDatablock> {
    let mut r = Reader::new(payload);
    let object_id = root_header(&mut r, CLASS_GI_DATABLOCK)?;
    let season = r.u64()?;
    let n = count(&mut r)?;
    let mut results = Vec::with_capacity(n.min(1024));
    for _ in 0..n {
        let at = r.position;
        let tag = r.u8()?;
        if tag != 0 {
            return invalid(format!(
                "GIResult handle tag {tag} at {at}: only inline observed"
            ));
        }
        r.u8()?;
        let id = r.u64()?;
        expect_type(&mut r, CLASS_GI_RESULT)?;
        let mut res = gi_result(&mut r)?;
        res.object_id = id;
        results.push(res);
    }
    finish(&r, "GIDatablock")?;
    Ok(GiDatablock {
        object_id,
        season,
        results,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn push_f32s(b: &mut Vec<u8>, v: &[f32]) {
        for x in v {
            b.extend_from_slice(&x.to_le_bytes());
        }
    }

    /// Synthetic loader: root node with one leaf child, independent of install data.
    fn synthetic_loader() -> Vec<u8> {
        let mut b = vec![1];
        b.extend_from_slice(&0x003a_66d8_e3b6_u64.to_le_bytes());
        b.extend_from_slice(&CLASS_GI_LOADER.to_le_bytes());
        b.extend_from_slice(&0x7f5f_1bc0bu64.to_le_bytes());
        push_f32s(&mut b, &[32.0; 3]);
        push_f32s(&mut b, &[16.0; 3]);
        b.extend_from_slice(&1u32.to_le_bytes());
        let node = |b: &mut Vec<u8>, children: u32, block: u64| {
            b.push(0);
            b.extend_from_slice(&0u64.to_le_bytes());
            b.extend_from_slice(&CLASS_TREE_NODE.to_le_bytes());
            push_f32s(b, &[-8.0, -8.0, 0.0]);
            b.extend_from_slice(&3u32.to_le_bytes());
            push_f32s(b, &[16.0; 3]);
            b.push(1);
            b.push(0);
            b.extend_from_slice(&0u64.to_le_bytes());
            b.extend_from_slice(&children.to_le_bytes());
            (block, b.len())
        };
        let link = |b: &mut Vec<u8>, block: u64| {
            b.extend_from_slice(&0u64.to_le_bytes());
            b.extend_from_slice(&CLASS_TREE_LINK.to_le_bytes());
            b.extend_from_slice(&0u64.to_le_bytes());
            b.push(0);
            b.extend_from_slice(&(block + 1).to_le_bytes());
            b.push(0);
            b.extend_from_slice(&block.to_le_bytes());
        };
        node(&mut b, 1, 0);
        node(&mut b, 0, 0x17_5871_6bd6);
        link(&mut b, 0x17_5871_6bd6);
        b.extend_from_slice(&0u64.to_le_bytes());
        b.extend_from_slice(&CLASS_TREE_LINK.to_le_bytes());
        b.extend_from_slice(&0u64.to_le_bytes());
        b.push(0);
        b.extend_from_slice(&0u64.to_le_bytes());
        b.push(0);
        b.extend_from_slice(&0u64.to_le_bytes());
        push_f32s(&mut b, &[-960.0, -864.0, -112.0, 0.0]);
        push_f32s(&mut b, &[1568.0, 1376.0, 128.0, 0.0]);
        b.extend_from_slice(&0u64.to_le_bytes());
        b.extend_from_slice(&CLASS_LAYER_FILTER.to_le_bytes());
        b.extend_from_slice(&0u32.to_le_bytes());
        b
    }

    #[test]
    fn synthetic_loader_decodes_to_end() -> Result<()> {
        let l = decode_loader(&synthetic_loader())?;
        assert_eq!(l.season, 0x7f5f_1bc0b);
        assert_eq!(l.node_count, 2);
        let leaves = l.leaves();
        assert_eq!(leaves.len(), 1);
        assert_eq!(leaves[0].datablock.id, 0x17_5871_6bd6);
        assert_eq!(l.vector32, [1568.0, 1376.0, 128.0, 0.0]);
        Ok(())
    }

    #[test]
    fn loader_rejects_trailing_and_truncated_bytes() {
        let mut b = synthetic_loader();
        b.push(0);
        assert!(
            decode_loader(&b)
                .unwrap_err()
                .to_string()
                .contains("trailing")
        );
        let b = synthetic_loader();
        for cut in [0, 1, 13, 40, b.len() - 1] {
            assert!(decode_loader(&b[..cut]).is_err(), "cut {cut}");
        }
    }

    #[test]
    fn loader_rejects_wrong_class_and_tags() {
        let mut b = synthetic_loader();
        b[9] ^= 0xff;
        assert!(
            decode_loader(&b)
                .unwrap_err()
                .to_string()
                .contains("expected 9ed472d4")
        );
        let mut b = synthetic_loader();
        b[0] = 0;
        assert!(decode_loader(&b).is_err());
        let mut b = synthetic_loader();
        b[49] = 2; // root pointer tag: references are not supported
        assert!(decode_loader(&b).unwrap_err().to_string().contains("tag 2"));
    }

    fn synthetic_blob(dim: u32) -> Vec<u8> {
        let mut b = GICP_MAGIC.to_vec();
        b.extend_from_slice(&[1, 0, 0, 0]);
        for _ in 0..3 {
            b.extend_from_slice(&dim.to_le_bytes());
        }
        b.extend_from_slice(&[0xab; 7]);
        b
    }

    fn synthetic_datablock(blob_magic: bool) -> Vec<u8> {
        let mut b = vec![1];
        b.extend_from_slice(&0x17_5871_6bd6u64.to_le_bytes());
        b.extend_from_slice(&CLASS_GI_DATABLOCK.to_le_bytes());
        b.extend_from_slice(&0x7f5f_1bc0bu64.to_le_bytes());
        b.extend_from_slice(&1u32.to_le_bytes());
        b.extend_from_slice(&[0, 1]);
        b.extend_from_slice(&0x18_62d4_bc09u64.to_le_bytes());
        b.extend_from_slice(&CLASS_GI_RESULT.to_le_bytes());
        for v in [1u32, 6825, 0] {
            b.extend_from_slice(&v.to_le_bytes());
        }
        push_f32s(&mut b, &[-720.0, -336.0, -48.0, 0.0]);
        push_f32s(&mut b, &[16.0, 16.0, 16.0, 0.0]);
        push_f32s(&mut b, &[32.0; 4]);
        b.push(0);
        b.extend_from_slice(&0u64.to_le_bytes());
        b.push(3);
        b.push(1);
        b.extend_from_slice(&1u32.to_le_bytes());
        b.extend_from_slice(&0u64.to_le_bytes());
        b.extend_from_slice(&CLASS_GI_DATA.to_le_bytes());
        for i in 0..4 {
            let mut blob = synthetic_blob(if i == 3 { 32 } else { 16 });
            if !blob_magic {
                blob[0] = b'X';
            }
            b.extend_from_slice(&(blob.len() as u32).to_le_bytes());
            b.extend_from_slice(&blob);
        }
        push_f32s(&mut b, &[0.7, 0.76, 0.74, 0.04]);
        push_f32s(&mut b, &[-0.13, -0.69, -0.08, 0.0]);
        b.extend_from_slice(&[0, 1]);
        b.extend_from_slice(&7u32.to_le_bytes());
        b.push(1);
        b.extend_from_slice(&32u32.to_le_bytes());
        b
    }

    #[test]
    fn synthetic_datablock_decodes_to_end() -> Result<()> {
        let d = decode_datablock(&synthetic_datablock(true))?;
        assert_eq!(d.results.len(), 1);
        let r = &d.results[0];
        assert_eq!(r.object_id, 0x18_62d4_bc09);
        assert_eq!(r.center, [-720.0, -336.0, -48.0, 0.0]);
        assert_eq!(r.data[0].blob_dims[3], [32, 32, 32]);
        assert_eq!(r.data[0].value96, 7);
        assert_eq!(r.data[0].value100, 32);
        Ok(())
    }

    #[test]
    fn datablock_rejects_bad_magic_overrun_and_trailing() {
        assert!(
            decode_datablock(&synthetic_datablock(false))
                .unwrap_err()
                .to_string()
                .contains("GICP")
        );
        let mut b = synthetic_datablock(true);
        b.push(9);
        assert!(
            decode_datablock(&b)
                .unwrap_err()
                .to_string()
                .contains("trailing")
        );
        let b = synthetic_datablock(true);
        assert!(decode_datablock(&b[..b.len() - 3]).is_err());
        // A blob length larger than the payload must fail before allocation.
        let mut b = synthetic_datablock(true);
        let at = b.windows(4).position(|w| w == GICP_MAGIC).expect("blob") - 4;
        b[at..at + 4].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(
            decode_datablock(&b)
                .unwrap_err()
                .to_string()
                .contains("overruns")
        );
    }
}
