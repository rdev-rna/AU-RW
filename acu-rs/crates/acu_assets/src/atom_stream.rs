//! Exact AtomGraph stream reader (resource type `c0c861cd`), ACU-RE-0066.
//!
//! Every read follows the native per-class Load function of ACU.exe (SHA-256 748abdc6…7e40);
//! the class -> loader map comes from the engine's create path (constructor at class info +80,
//! vtable slot 2). This is a port of `tools/atomgraph_stream.py`, which consumes NewDemo
//! (42,174,002 bytes, 945,730 objects) exactly. Field names are the member offsets the loader
//! writes (`f80` = `*(a1 + 80)`); meanings are assigned by callers with their own evidence.
//! Unknown classes, oversized counts, truncation and nesting past a bound fail with an error.

use crate::{Result, invalid};

/// CRC-32 (IEEE), used for class hashes.
pub fn crc32(data: &[u8]) -> u32 {
    let mut crc = !0u32;
    for &b in data {
        crc ^= u32::from(b);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xEDB8_8320 & 0u32.wrapping_sub(crc & 1));
        }
    }
    !crc
}

/// A decoded value.
#[derive(Clone, Debug, PartialEq)]
pub enum Val {
    U(u64),
    F(f32),
    Raw(Vec<u8>),
    Str(String),
    Ints(Vec<u32>),
    Obj(Box<Obj>),
    Arr(Vec<Val>),
    /// Pointer tag 2: a reference to an object id.
    Ref(u64),
    /// Pointer tag 3.
    Null,
    /// Pointer tag other than 0/2/3 (reads nothing).
    Tag(u8),
}

/// One object: class name, payload offset of its header, stream id and fields in stream order.
#[derive(Clone, Debug, PartialEq)]
pub struct Obj {
    pub class: &'static str,
    pub at: usize,
    pub id: u64,
    pub fields: Vec<(&'static str, Val)>,
}

impl Obj {
    pub fn get(&self, name: &str) -> Option<&Val> {
        self.fields.iter().find(|(n, _)| *n == name).map(|(_, v)| v)
    }
    pub fn u(&self, name: &str) -> Option<u64> {
        match self.get(name)? {
            Val::U(v) => Some(*v),
            _ => None,
        }
    }
    pub fn f(&self, name: &str) -> Option<f32> {
        match self.get(name)? {
            Val::F(v) => Some(*v),
            _ => None,
        }
    }
    pub fn obj(&self, name: &str) -> Option<&Obj> {
        match self.get(name)? {
            Val::Obj(o) => Some(o),
            _ => None,
        }
    }
    /// Objects of an array field (references and nulls are skipped).
    pub fn objs(&self, name: &str) -> Vec<&Obj> {
        match self.get(name) {
            Some(Val::Arr(items)) => items
                .iter()
                .filter_map(|v| match v {
                    Val::Obj(o) => Some(o.as_ref()),
                    _ => None,
                })
                .collect(),
            _ => Vec::new(),
        }
    }
    pub fn ints(&self, name: &str) -> &[u32] {
        match self.get(name) {
            Some(Val::Ints(v)) => v,
            _ => &[],
        }
    }
    pub fn str(&self, name: &str) -> Option<&str> {
        match self.get(name)? {
            Val::Str(s) => Some(s),
            _ => None,
        }
    }
    /// Depth-first visit of this object and every object below it.
    pub fn visit<'a>(&'a self, f: &mut impl FnMut(&'a Obj)) {
        f(self);
        for (_, v) in &self.fields {
            visit_val(v, f);
        }
    }
}

fn visit_val<'a>(v: &'a Val, f: &mut impl FnMut(&'a Obj)) {
    match v {
        Val::Obj(o) => o.visit(f),
        Val::Arr(items) => items.iter().for_each(|i| visit_val(i, f)),
        _ => {}
    }
}

#[derive(Clone, Copy, Debug)]
enum Op {
    Base(&'static str),
    U8(&'static str),
    U16(&'static str),
    U32(&'static str),
    U64(&'static str),
    F32(&'static str),
    Raw(usize, &'static str),
    Emb(&'static str, &'static str),
    Embs(&'static str, &'static str),
    Ptr(&'static str),
    Ptrs(&'static str),
    SPtr(&'static str, &'static str),
    VObj(&'static str),
    Handle(&'static str),
    Str(&'static str),
    RawArr(usize, &'static str),
}
use Op::*;

const GRAPH_NODE: &[Op] = &[
    Embs("AtomInputPort", "f8"),
    Embs("AtomOutputPort", "f24"),
    Emb("AtomNodeID", "f56"),
    U16("f72"),
];

/// (class, native loader VA, ops). The VA is provenance only.
const LOADERS: &[(&str, u64, &[Op])] = &[
    (
        "AtomGraph",
        0x1400C5320,
        &[
            Ptr("f16"),
            Ptr("f24"),
            Ptr("f32"),
            Embs("AtomRTCPDescriptor", "f40"),
            VObj("f56"),
            SPtr("AtomCustomTransitionSystem", "f96"),
            U32("f104"),
            U32("f108"),
            U32("f112"),
            Emb("ExternalIdMap", "f116"),
        ],
    ),
    (
        "AtomStateNode",
        0x14010C100,
        &[
            Ptrs("f16"),
            U32("f100"),
            Emb("AtomNodeID", "f104"),
            Embs("AtomNodeID", "f28"),
            U32("f120"),
            RawArr(2, "f40"),
            RawArr(2, "f52"),
            Ptrs("f64"),
            RawArr(4, "f88"),
        ],
    ),
    ("AtomNullStateNode", 0x14010C100, &[Base("AtomStateNode")]),
    (
        "AtomGraphStateNode",
        0x14010B9B0,
        &[
            Base("AtomStateNode"),
            Ptrs("f128"),
            U32("f152"),
            U16("f160"),
            U16("f162"),
            U16("f164"),
            U16("f166"),
            U16("f168"),
            U32("f156"),
            U8("f170"),
            RawArr(4, "f140"),
            U8("f171"),
        ],
    ),
    (
        "AtomStateMachineNode",
        0x14010BF10,
        &[
            Base("AtomStateNode"),
            Ptrs("f128"),
            Embs("AtomInitialState", "f140"),
            Ptrs("f152"),
        ],
    ),
    (
        "AtomLayeringStateNode",
        0x1400C5D70,
        &[
            Base("AtomStateNode"),
            Emb("AtomNodeID", "f164"),
            Emb("AtomLayeringInfo", "f184"),
            U32("f320"),
            Embs("AtomLayeringInfo", "f128"),
            Ptrs("f140"),
            U32("f324"),
            U32("f328"),
            RawArr(4, "f152"),
            U8("f332"),
        ],
    ),
    (
        "AtomStateDescriptor",
        0x14010BE50,
        &[Embs("AtomStateProperty", "f8"), U32("f20"), U32("f24")],
    ),
    (
        "AtomStateProperty",
        0x14010C400,
        &[
            U32("f0"),
            U32("f4"),
            Emb("AtomDataContainerWrapper", "f16"),
            U16("f8"),
            U16("f10"),
            U8("f12.0"),
            U8("f12.1"),
        ],
    ),
    (
        "AtomInitialState",
        0x14010BC60,
        &[
            SPtr("AtomConditionExpression", "f0"),
            U16("f8"),
            Emb("AtomNodeID", "f10"),
        ],
    ),
    (
        "AtomStateTransitionTarget",
        0x14010C4E0,
        &[
            SPtr("AtomConditionExpression", "f8"),
            U16("f36"),
            U8("f38.b11"),
            U32("f38.b0_2"),
            U32("f38.b5_6"),
            U32("f38.b7_8"),
            U32("f38.b9_10"),
            U32("f38.b3_4"),
            U32("f32.first"),
            F32("f32"),
            U8("f38.b15"),
            U8("f38.b13"),
            U8("f38.b12"),
            Emb("AtomNodeID", "f16"),
        ],
    ),
    (
        "AtomCustomTransitionSystem",
        0x14010B930,
        &[Embs("AtomTransitionCellDescription", "f0")],
    ),
    (
        "AtomTransitionCellDescription",
        0x14010C870,
        &[
            Emb("AtomNodeID", "f20"),
            Emb("AtomNodeID", "f36"),
            Emb("AtomNodeID", "f52"),
            Emb("AtomNodeID", "f68"),
            U32("f8.b0_3"),
            U32("f8.b4_7"),
            U32("f8.b8_9"),
            U32("f8.b10_11"),
            U32("f8.b12_13"),
            U32("f8.b14_17"),
            U32("f8.b18_21"),
            U32("f8.b22_23"),
            U32("f8.b24_25"),
            U32("f8.b26_27"),
            U32("f12"),
            U32("f16"),
            Ptr("f0"),
        ],
    ),
    (
        "AtomLayeringInfo",
        0x1400C5A90,
        &[
            Emb("AtomNodeID", "f118"),
            RawArr(4, "f0"),
            U32("f80"),
            U8("f104"),
            U32("f84"),
            RawArr(4, "f12"),
            RawArr(4, "f24"),
            RawArr(4, "f36"),
            Embs("BoneWeight", "f106"),
            U32("f88"),
            U32("f48"),
            Ptr("f56"),
            SPtr("AtomSelectBoneMaskDecisionNode", "f64"),
            Handle("f72"),
            U32("f92"),
            U8("f105"),
            U32("f96"),
            U32("f100"),
        ],
    ),
    (
        "AtomSelectBoneMaskDecisionNode",
        0x1400C67F0,
        &[
            U32("f0"),
            U32("f16"),
            Embs("BoneWeight", "f32"),
            U32("f20"),
            U32("f24"),
            Embs("AtomSelectBoneMaskDecisionNode", "f44"),
            SPtr("AtomConditionExpression", "f8"),
            U32("f28"),
            Emb("AtomNodeID", "f56"),
        ],
    ),
    ("BoneWeight", 0x140B999C0, &[U32("f0"), U8("f4")]),
    (
        "AtomRTCPDescriptor",
        0x14010BD40,
        &[
            Str("f0"),
            Str("f8"),
            U32("f16"),
            U32("f20"),
            Emb("AtomDataContainerWrapper", "f32"),
        ],
    ),
    (
        "ExternalIdMap",
        0x1400C6F80,
        &[RawArr(4, "f0"), Embs("ExternalIdHierarchyData", "f12")],
    ),
    (
        "ExternalIdHierarchyData",
        0x1400C6EE0,
        &[Embs("ExternalIdNodeData", "f0")],
    ),
    ("ExternalIdNodeData", 0x141E179B0, &[U32("f0"), U32("f4")]),
    (
        "AtomConditionExpression",
        0x1400C5060,
        &[U64("f0"), Embs("AtomCondition", "f20"), RawArr(4, "f8")],
    ),
    (
        "AtomCondition",
        0x1400C4F20,
        &[
            U32("f0"),
            U32("f4"),
            U32("f8.b0_2"),
            U32("f8.b3_6"),
            U32("f8.b7_8"),
            U32("f8.b9_10"),
            U8("f8.b11"),
            U16("f10"),
            U16("f12"),
            Emb("AtomDataContainerWrapper", "f16"),
        ],
    ),
    (
        "AtomDataContainerWrapper",
        0x1400C5210,
        &[
            U32("type"),
            U32("i"),
            U32("j"),
            Raw(12, "v12"),
            Raw(16, "v16"),
        ],
    ),
    (
        "AtomInputPort",
        0x1400C5870,
        &[
            U32("kind"),
            U64("q"),
            U32("j"),
            Raw(12, "v12"),
            Raw(16, "v16"),
        ],
    ),
    ("AtomOutputPort", 0x14204A650, &[U32("f8")]),
    ("AtomNodeID", 0x1400C5FF0, &[Raw(16, "id")]),
    ("AtomGraphNode", 0x1400C5530, GRAPH_NODE),
    ("AtomLogicalConjunctionNode", 0x1400C5530, GRAPH_NODE),
    (
        "AtomAnimationDataNode",
        0x1400C4A30,
        &[
            Base("AtomGraphNode"),
            Handle("f80"),
            U16("f100"),
            U16("f102"),
            U16("f104"),
            U16("f106"),
            U32("f88"),
            U32("f92"),
            U32("f96"),
        ],
    ),
    (
        "AtomPlayCustomActionNode",
        0x1400C4A30,
        &[Base("AtomAnimationDataNode")],
    ),
    (
        "AtomAnimationRootNode",
        0x1400C4B00,
        &[
            Base("AtomGraphNode"),
            U32("f80"),
            U32("f84"),
            U32("f88"),
            U32("f92"),
            U8("f112"),
            U8("f113"),
            U8("f114"),
            U32("f96"),
            U32("f100"),
            U32("f104"),
            U32("f108"),
        ],
    ),
    (
        "AtomAnimationSequenceNode",
        0x1400C4D70,
        &[
            Base("AtomGraphNode"),
            U32("f80"),
            U32("f84"),
            U16("f88"),
            U16("f90"),
        ],
    ),
    (
        "AtomAngularBlendNode",
        0x1400C4D70,
        &[Base("AtomAnimationSequenceNode")],
    ),
    (
        "AtomAxialBlendNode",
        0x1400C4CF0,
        &[Base("AtomAnimationSequenceNode"), RawArr(4, "f96")],
    ),
    (
        "AtomTimeScaleNode",
        0x1400C6E90,
        &[Base("AtomGraphNode"), U16("f80")],
    ),
    (
        "AtomAddSubtractNode",
        0x1400C48E0,
        &[Base("AtomGraphNode"), U8("f84"), U32("f80")],
    ),
    (
        "AtomMultiplyDivideNode",
        0x1400C48E0,
        &[Base("AtomGraphNode"), U8("f84"), U32("f80")],
    ),
    (
        "AtomCacheValueNode",
        0x1400C6390,
        &[Base("AtomGraphNode"), U32("f80"), U32("f84")],
    ),
    (
        "AtomIfNode",
        0x1400C6390,
        &[Base("AtomGraphNode"), U32("f80"), U32("f84")],
    ),
    (
        "AtomRagdollBoneControllerNode",
        0x1400C6390,
        &[Base("AtomGraphNode"), U32("f80"), U32("f84")],
    ),
    (
        "AtomSwitchNode",
        0x1400C6390,
        &[Base("AtomGraphNode"), U32("f80"), U32("f84")],
    ),
    (
        "AtomWriteParamNode",
        0x1400C6390,
        &[Base("AtomGraphNode"), U32("f80"), U32("f84")],
    ),
    (
        "AtomCompareValuesNode",
        0x1400C4ED0,
        &[Base("AtomGraphNode"), U32("f80")],
    ),
    (
        "AtomRemapNode",
        0x1400C4ED0,
        &[Base("AtomGraphNode"), U32("f80")],
    ),
    (
        "AtomDampRTCPNode",
        0x1400C5190,
        &[
            Base("AtomGraphNode"),
            U32("f80"),
            U32("f84"),
            U32("f88"),
            U32("f92"),
            U32("f96"),
        ],
    ),
    (
        "AtomGraphConditionNode",
        0x1400C54D0,
        &[
            Base("AtomGraphNode"),
            SPtr("AtomConditionExpression", "f80"),
            U32("f88"),
        ],
    ),
    (
        "AtomIKSolveOptionsNode",
        0x1400C5810,
        &[Base("AtomGraphNode"), U8("f80"), U16("f88")],
    ),
    (
        "AtomBipedFootIKNode",
        0x1400C6340,
        &[Base("AtomIKSolveOptionsNode"), U32("f96")],
    ),
    (
        "AtomModifyIKNode",
        0x1400C6340,
        &[Base("AtomIKSolveOptionsNode"), U32("f96")],
    ),
    (
        "AtomIKPathNode",
        0x1400C57B0,
        &[Base("AtomBipedFootIKNode"), U16("f108"), U32("f104")],
    ),
    (
        "AtomSetIKPathParamNode",
        0x1400C6BE0,
        &[
            Base("AtomGraphNode"),
            U8("f80"),
            U16("f116"),
            U32("f88"),
            U32("f92"),
            U32("f96"),
            U32("f100"),
            U32("f104"),
            U32("f108"),
            U32("f112"),
        ],
    ),
    (
        "AtomIkPathExtension",
        0x14010BBB0,
        &[
            U16("f8"),
            U32("f16"),
            U32("f20"),
            U32("f24"),
            U32("f28"),
            U32("f32"),
            U32("f36"),
            U32("f40"),
            U8("f44"),
        ],
    ),
    (
        "AtomLayerBlendNode",
        0x1400C5990,
        &[
            Base("AtomGraphNode"),
            Embs("BoneWeight", "f80"),
            U32("f92"),
            U32("f96"),
            U32("f100"),
            U32("f104"),
            U32("f108"),
            U32("f112"),
        ],
    ),
    (
        "AtomRandomSelectionNode",
        0x1400C63F0,
        &[
            Base("AtomGraphNode"),
            U8("f174.b0"),
            U8("f174.b1"),
            U32("f174.b2_3"),
            U32("f140"),
            U32("f144"),
            RawArr(4, "f80"),
            RawArr(4, "f92"),
            RawArr(4, "f104"),
            RawArr(4, "f116"),
            RawArr(4, "f128"),
            U32("f148"),
            U32("f152"),
            U32("f156"),
            U32("f160"),
            U32("f164"),
            U32("f168"),
            U16("f172"),
        ],
    ),
    ("AtomRawFile", 0x141E6D4E0, &[]),
    (
        "AtomReadParamNode",
        0x1400C66D0,
        &[
            Base("AtomGraphNode"),
            U32("f80"),
            U32("f84"),
            U32("f88"),
            U32("f92"),
        ],
    ),
    (
        "AtomSelectBestMatchingTagsNode",
        0x1400C6740,
        &[
            Base("AtomGraphNode"),
            U32("f80"),
            U32("f84"),
            U32("f88"),
            U32("f92"),
            U32("f96"),
            U32("f100"),
            U16("f104"),
            U16("f106"),
        ],
    ),
    (
        "AtomSelectFromIntervalNode",
        0x1400C69A0,
        &[Base("AtomGraphNode"), U32("f84"), U32("f80")],
    ),
    (
        "AtomSelectionNode",
        0x1400C6B00,
        &[Base("AtomGraphNode"), U32("f80"), U32("f88"), U32("f84")],
    ),
    (
        "AtomSetBipedIKParamsNode",
        0x1400C6B60,
        &[
            Base("AtomGraphNode"),
            U8("f94"),
            U16("f92"),
            U32("f80"),
            U32("f84"),
            U32("f88"),
        ],
    ),
    (
        "AtomSetTimeNode",
        0x14010BDD0,
        &[
            Base("AtomGraphNode"),
            U32("f80"),
            U16("f84"),
            U16("f86"),
            U8("f88"),
        ],
    ),
    (
        "AtomSimpleBlendNode",
        0x1400C6D40,
        &[Base("AtomGraphNode"), U16("f80"), U16("f82")],
    ),
    (
        "AtomSkeletonPoseNode",
        0x1400C6DA0,
        &[Base("AtomGraphNode"), U16("f84"), U16("f86"), U32("f80")],
    ),
    (
        "AtomStickyAxialBlendNode",
        0x1400C6E10,
        &[
            Base("AtomGraphNode"),
            U32("f80"),
            U32("f84"),
            U32("f88"),
            U16("f92"),
            U16("f94"),
        ],
    ),
];

fn loader(class: &str) -> Option<&'static [Op]> {
    LOADERS
        .iter()
        .find(|(n, _, _)| *n == class)
        .map(|(_, _, ops)| *ops)
}

fn class_of(hash: u32) -> Option<&'static str> {
    LOADERS
        .iter()
        .map(|(n, _, _)| *n)
        .find(|n| crc32(n.as_bytes()) == hash)
}

/// Defensive bounds (not native constants).
const MAX_COUNT: u32 = 1 << 20;
const MAX_DEPTH: usize = 64;

pub struct Stream<'a> {
    b: &'a [u8],
    pub pos: usize,
    pub objects: usize,
}

impl<'a> Stream<'a> {
    pub fn new(b: &'a [u8]) -> Self {
        Self {
            b,
            pos: 0,
            objects: 0,
        }
    }

    fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        let end = self.pos.checked_add(n).filter(|&e| e <= self.b.len());
        match end {
            Some(e) => {
                let s = &self.b[self.pos..e];
                self.pos = e;
                Ok(s)
            }
            None => invalid(format!(
                "AtomGraph read of {n} bytes at {} past end {}",
                self.pos,
                self.b.len()
            )),
        }
    }
    fn u8(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }
    fn u16(&mut self) -> Result<u16> {
        Ok(u16::from_le_bytes(self.take(2)?.try_into().unwrap()))
    }
    fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn u64(&mut self) -> Result<u64> {
        Ok(u64::from_le_bytes(self.take(8)?.try_into().unwrap()))
    }
    fn count(&mut self) -> Result<u32> {
        let at = self.pos;
        let n = self.u32()?;
        if n > MAX_COUNT {
            return invalid(format!(
                "AtomGraph array count {n} at {at} exceeds {MAX_COUNT}"
            ));
        }
        Ok(n)
    }

    fn header(&mut self) -> Result<(usize, u64, u32)> {
        let at = self.pos;
        Ok((at, self.u64()?, self.u32()?))
    }

    fn dynamic(&mut self, hash: u32) -> Result<&'static str> {
        class_of(hash).map_or_else(
            || {
                invalid(format!(
                    "AtomGraph: no loader for class {hash:08x} at {}",
                    self.pos - 4
                ))
            },
            Ok,
        )
    }

    fn body(&mut self, class: &'static str, at: usize, id: u64, depth: usize) -> Result<Obj> {
        if depth > MAX_DEPTH {
            return invalid(format!(
                "AtomGraph nesting deeper than {MAX_DEPTH} at {}",
                self.pos
            ));
        }
        self.objects += 1;
        let mut obj = Obj {
            class,
            at,
            id,
            fields: Vec::new(),
        };
        self.run(class, &mut obj, depth)?;
        Ok(obj)
    }

    fn embedded(&mut self, class: &'static str, depth: usize) -> Result<Val> {
        let (at, id, _) = self.header()?;
        Ok(Val::Obj(Box::new(self.body(class, at, id, depth)?)))
    }

    fn pointer(&mut self, fixed: Option<&'static str>, depth: usize) -> Result<Val> {
        match self.u8()? {
            0 => {
                let (at, id, hash) = self.header()?;
                let class = match fixed {
                    Some(c) => c,
                    None => self.dynamic(hash)?,
                };
                Ok(Val::Obj(Box::new(self.body(class, at, id, depth)?)))
            }
            2 => Ok(Val::Ref(self.u64()?)),
            3 => Ok(Val::Null),
            t => Ok(Val::Tag(t)),
        }
    }

    fn run(&mut self, class: &'static str, obj: &mut Obj, depth: usize) -> Result<()> {
        let Some(ops) = loader(class) else {
            return invalid(format!("AtomGraph: no loader for {class}"));
        };
        for op in ops {
            let (name, v) = match *op {
                Base(c) => {
                    self.run(c, obj, depth)?;
                    continue;
                }
                U8(n) => (n, Val::U(self.u8()?.into())),
                U16(n) => (n, Val::U(self.u16()?.into())),
                U32(n) => (n, Val::U(self.u32()?.into())),
                U64(n) => (n, Val::U(self.u64()?)),
                F32(n) => (n, Val::F(f32::from_bits(self.u32()?))),
                Raw(k, n) => (n, Val::Raw(self.take(k)?.to_vec())),
                Emb(c, n) => (n, self.embedded(c, depth + 1)?),
                Embs(c, n) => {
                    let k = self.count()?;
                    let mut items = Vec::with_capacity(k.min(4096) as usize);
                    for _ in 0..k {
                        items.push(self.embedded(c, depth + 1)?);
                    }
                    (n, Val::Arr(items))
                }
                Ptr(n) => (n, self.pointer(None, depth + 1)?),
                Ptrs(n) => {
                    let k = self.count()?;
                    let mut items = Vec::with_capacity(k.min(4096) as usize);
                    for _ in 0..k {
                        items.push(self.pointer(None, depth + 1)?);
                    }
                    (n, Val::Arr(items))
                }
                SPtr(c, n) => (n, self.pointer(Some(c), depth + 1)?),
                VObj(n) => {
                    let (at, id, hash) = self.header()?;
                    let c = self.dynamic(hash)?;
                    (n, Val::Obj(Box::new(self.body(c, at, id, depth + 1)?)))
                }
                Handle(n) => {
                    let tag = self.u8()?;
                    self.take(1)?;
                    if tag == 0 {
                        let (at, id, hash) = self.header()?;
                        let c = self.dynamic(hash)?;
                        (n, Val::Obj(Box::new(self.body(c, at, id, depth + 1)?)))
                    } else {
                        (n, Val::Ref(self.u64()?))
                    }
                }
                Str(n) => {
                    let k = self.count()? as usize;
                    let s = if k == 0 {
                        String::new()
                    } else {
                        let raw = self.take(k + 1)?;
                        raw[..k].iter().map(|&c| char::from(c)).collect()
                    };
                    (n, Val::Str(s))
                }
                RawArr(size, n) => {
                    let k = self.count()? as usize;
                    let raw = self.take(k * size)?;
                    let v = raw
                        .chunks_exact(size)
                        .map(|c| {
                            if size == 2 {
                                u32::from(u16::from_le_bytes([c[0], c[1]]))
                            } else {
                                u32::from_le_bytes(c.try_into().unwrap())
                            }
                        })
                        .collect();
                    (n, Val::Ints(v))
                }
            };
            obj.fields.push((name, v));
        }
        Ok(())
    }
}

/// Parse a whole AtomGraph payload (u8 1, u64 id, u32 class, then the root object) and require
/// that it is consumed exactly.
pub fn parse(b: &[u8]) -> Result<Obj> {
    let mut s = Stream::new(b);
    if s.u8()? != 1 {
        return invalid("AtomGraph resource tag is not 1");
    }
    let (at, id, hash) = s.header()?;
    let class = s.dynamic(hash)?;
    let root = s.body(class, at, id, 0)?;
    if s.pos != b.len() {
        return invalid(format!("AtomGraph: {} trailing bytes", b.len() - s.pos));
    }
    Ok(root)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hdr(b: &mut Vec<u8>, class: &str, id: u64) {
        b.extend(id.to_le_bytes());
        b.extend(crc32(class.as_bytes()).to_le_bytes());
    }
    fn u32s(b: &mut Vec<u8>, v: &[u32]) {
        v.iter().for_each(|x| b.extend(x.to_le_bytes()));
    }
    fn node_id(b: &mut Vec<u8>, n: u8) {
        hdr(b, "AtomNodeID", 0);
        b.extend([n; 16]);
    }

    /// Same synthetic graph as tools/test_atomgraph_stream.py.
    fn graph() -> Vec<u8> {
        let mut b = vec![1];
        hdr(&mut b, "AtomGraph", 0x13D5BC4D8);
        // f16: machine with one transition
        b.push(0);
        hdr(&mut b, "AtomStateMachineNode", 0x77);
        u32s(&mut b, &[1]);
        b.push(0);
        hdr(&mut b, "AtomStateTransitionTarget", 0x55);
        b.push(0);
        hdr(&mut b, "AtomConditionExpression", 0);
        b.extend(0u64.to_le_bytes());
        u32s(&mut b, &[0, 0]);
        b.extend(3u16.to_le_bytes());
        b.push(1);
        u32s(&mut b, &[1, 2, 3, 0, 1, 7]);
        b.extend(0.25f32.to_le_bytes());
        b.extend([1, 0, 1]);
        node_id(&mut b, 2);
        u32s(&mut b, &[0]);
        node_id(&mut b, 1);
        u32s(&mut b, &[0, 0, 0, 0, 0, 0, 0, 0, 0]);
        b.push(3); // f24 null
        b.push(2);
        b.extend(0x55u64.to_le_bytes()); // f32 ref
        u32s(&mut b, &[0]);
        hdr(&mut b, "AtomStateDescriptor", 0);
        u32s(&mut b, &[0, 4, 5]);
        b.push(3);
        u32s(&mut b, &[1, 2, 3]);
        hdr(&mut b, "ExternalIdMap", 0);
        u32s(&mut b, &[1, 9, 0]);
        b
    }

    #[test]
    fn synthetic_graph_consumes_exactly() {
        let root = parse(&graph()).unwrap();
        assert_eq!(root.id, 0x13D5BC4D8);
        let m = root.obj("f16").unwrap();
        assert_eq!(m.class, "AtomStateMachineNode");
        let t = m.objs("f16")[0];
        assert_eq!(
            (t.u("f36"), t.f("f32"), t.u("f32.first")),
            (Some(3), Some(0.25), Some(7))
        );
        assert_eq!(root.get("f24"), Some(&Val::Null));
        assert_eq!(root.get("f32"), Some(&Val::Ref(0x55)));
        assert_eq!(root.obj("f116").unwrap().ints("f0"), &[9]);
    }

    #[test]
    fn invalid_inputs_fail() {
        let b = graph();
        assert!(parse(&b[..b.len() - 3]).is_err());
        let mut extra = b.clone();
        extra.push(0);
        assert!(parse(&extra).unwrap_err().to_string().contains("trailing"));
        let mut unknown = vec![1];
        unknown.extend(0u64.to_le_bytes());
        unknown.extend(0x1234_5678u32.to_le_bytes());
        assert!(
            parse(&unknown)
                .unwrap_err()
                .to_string()
                .contains("no loader")
        );
        let mut big = vec![1];
        hdr(&mut big, "ExternalIdMap", 0);
        u32s(&mut big, &[1 << 30]);
        assert!(parse(&big).unwrap_err().to_string().contains("exceeds"));
    }

    /// Full NewDemo payload (local only): ACU_NEWDEMO=path cargo test -- --ignored newdemo
    #[test]
    #[ignore]
    fn newdemo_consumes_exactly() {
        let Ok(path) = std::env::var("ACU_NEWDEMO") else {
            eprintln!("ACU_NEWDEMO not set: unavailable");
            return;
        };
        let b = std::fs::read(path).unwrap();
        let mut s = Stream::new(&b);
        assert_eq!(s.u8().unwrap(), 1);
        let (at, id, hash) = s.header().unwrap();
        let class = s.dynamic(hash).unwrap();
        s.body(class, at, id, 0).unwrap();
        assert_eq!(s.pos, b.len());
        assert_eq!(s.objects, 945_730);
    }
}
