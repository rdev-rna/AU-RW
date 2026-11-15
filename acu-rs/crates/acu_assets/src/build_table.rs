//! Bounded `BuildTable` (`22ecbe63`) decoder: ACU-RE-0016 layout plus the
//! PropertyPath / `MaterialPropertyPathNodeSolver` / value-typed properties
//! needed by `CN_P_Avatars_ColorSets` (ACU-RE-0071).
//!
//! Field order was cross-checked against AnvilToolkit's Unity branch (a
//! third-party layout lead, see the findings) and every supported table must
//! decode to its exact payload end. Unsupported variants fail at the cursor;
//! the decoder never scans or resynchronises. Decoding a row says nothing
//! about runtime selection.

use serde::Serialize;

use crate::reader::Reader;
use crate::{Result, invalid};

pub const CLASS_BUILD_TABLE: u32 = 0x22ec_be63;
const CLASS_COLUMN: u32 = 0x3683_9608;
const CLASS_PROPERTY_PATH: u32 = 0x3c00_b2e0;
const CLASS_PROPERTY_PATH_NODE: u32 = 0x1a87_50ba;
const CLASS_MATERIAL_SOLVER: u32 = 0xbc5b_87af;
const CLASS_MASK16: u32 = 0x92b9_5f74;
const CLASS_ROW: u32 = 0x348b_28d6;
const CLASS_TAG: u32 = 0xb332_698e;
const CLASS_TAGS: u32 = 0x11bd_5345;
const CLASS_SELECTOR: u32 = 0xf5bd_7b8a;

const KIND_REFERENCE: u32 = 0x001c_0000;
const KIND_HANDLE: u32 = 0x0012_0000;
const MAX_COUNT: usize = 10_000;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum Value {
    Reference { tag: u8, scope: u8, file_id: String },
    Handle { tag: u8, file_id: String },
    Bool(bool),
    Signed8(i8),
    Unsigned8(u8),
    Signed16(i16),
    Unsigned16(u16),
    Signed32(i32),
    Unsigned32(u32),
    Signed64(i64),
    Unsigned64(u64),
    Float(f32),
    Vector2([f32; 2]),
    Vector3([f32; 3]),
    Vector4([f32; 4]),
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Property {
    /// Payload offset of the property header.
    pub offset: usize,
    pub data_type: String,
    pub kind: String,
    pub extra: u32,
    pub value: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MaterialSolver {
    pub file_id: String,
    pub filtering_by_blend_mode: bool,
    pub target_blend_mode: i32,
    pub filtering_by_alpha_test: bool,
    pub target_alpha_test: bool,
    pub match_using_material_overrider: bool,
    pub mode: i32,
    pub use_flag_mask: bool,
    pub match_mask: [u8; 16],
    pub target_material: String,
    pub target_template: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PathNode {
    pub index: i16,
    pub condition: u32,
    pub condition2: u32,
    pub match_all_conditions: bool,
    pub array_multi_targetting: bool,
    pub optional: bool,
    /// Property name hash this node steps into ("00000000" for none).
    pub target_property: String,
    pub condition_property: String,
    pub condition_data: u64,
    pub condition2_property: String,
    pub condition2_data: u64,
    /// The serialized byte that precedes an optional solver; 0 means present.
    pub solver_flag: u8,
    pub solver: Option<MaterialSolver>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Column {
    pub file_id: String,
    pub pass_id: u32,
    pub path: Vec<PathNode>,
    pub target_unique: bool,
    pub set_whole_array: bool,
    pub index: i32,
    pub properties: Vec<(u32, Property)>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Row {
    pub file_id: String,
    pub weight: f32,
    pub tag: u32,
    pub possible_tags: Vec<u32>,
    pub dependencies: Vec<String>,
    /// (column index, property), in serialized order.
    pub components: Vec<(i32, Property)>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BuildTable {
    pub file_id: String,
    pub columns: Vec<Column>,
    pub rows: Vec<Row>,
    pub shuffle: bool,
    pub subtables: Vec<String>,
}

impl BuildTable {
    pub fn column(&self, index: i32) -> Option<&Column> {
        self.columns.iter().find(|c| c.index == index)
    }
}

fn hex64(v: u64) -> String {
    format!("{v:016x}")
}

struct Cursor<'a> {
    r: Reader<'a>,
    len: usize,
}

impl<'a> Cursor<'a> {
    fn pos(&self) -> usize {
        self.len - self.r.remaining()
    }
    fn fail<T>(&self, message: &str) -> Result<T> {
        invalid(format!(
            "build table: {message} at payload offset {}",
            self.pos()
        ))
    }
    fn boolean(&mut self) -> Result<bool> {
        match self.r.u8()? {
            0 => Ok(false),
            1 => Ok(true),
            _ => self.fail("invalid boolean"),
        }
    }
    fn f32(&mut self) -> Result<f32> {
        let v = f32::from_bits(self.r.u32()?);
        if !v.is_finite() {
            return self.fail("nonfinite scalar");
        }
        Ok(v)
    }
    fn count(&mut self, minimum: usize) -> Result<usize> {
        let n = self.r.i32()?;
        if n < 0 || n as usize > MAX_COUNT || n as usize > self.r.remaining() / minimum.max(1) {
            return self.fail("invalid count");
        }
        Ok(n as usize)
    }
    fn header(&mut self, class: u32) -> Result<String> {
        let id = self.r.u64()?;
        let found = self.r.u32()?;
        if found != class {
            return self.fail(&format!(
                "unexpected class {found:08x}, expected {class:08x}"
            ));
        }
        Ok(hex64(id))
    }
    fn handle(&mut self) -> Result<(u8, String)> {
        Ok((self.r.u8()?, hex64(self.r.u64()?)))
    }
    fn pointer(&mut self) -> Result<Option<String>> {
        match self.r.u8()? {
            3 => Ok(None),
            1 => Ok(Some(hex64(self.r.u64()?))),
            _ => self.fail("unsupported object pointer"),
        }
    }
    fn tag(&mut self) -> Result<u32> {
        self.header(CLASS_TAG)?;
        self.r.u32()
    }
    fn tags(&mut self) -> Result<Vec<u32>> {
        self.header(CLASS_TAGS)?;
        let n = self.count(16)?;
        (0..n).map(|_| self.tag()).collect()
    }
    fn property(&mut self) -> Result<Property> {
        let offset = self.pos();
        let data_type = self.r.u32()?;
        let kind = self.r.u32()?;
        let extra = self.r.u32()?;
        let value = match kind {
            KIND_REFERENCE => {
                let (tag, scope, id) = (self.r.u8()?, self.r.u8()?, self.r.u64()?);
                if !(tag == 1 || (tag == 3 && id == 0)) {
                    return self.fail("unsupported file reference");
                }
                Value::Reference {
                    tag,
                    scope,
                    file_id: hex64(id),
                }
            }
            KIND_HANDLE => {
                let (tag, file_id) = self.handle()?;
                Value::Handle { tag, file_id }
            }
            0x0001_0000 => Value::Bool(self.boolean()?),
            0x0002_0000 => Value::Signed8(self.r.u8()? as i8),
            0x0003_0000 => Value::Unsigned8(self.r.u8()?),
            0x0004_0000 => Value::Signed16(self.r.i16()?),
            0x0005_0000 => Value::Unsigned16(self.r.u16()?),
            0x0006_0000 => Value::Signed32(self.r.i32()?),
            0x0007_0000 => Value::Unsigned32(self.r.u32()?),
            0x0008_0000 => Value::Signed64(self.r.i64()?),
            0x0009_0000 => Value::Unsigned64(self.r.u64()?),
            0x000a_0000 => Value::Float(self.f32()?),
            0x000b_0000 => Value::Vector2([self.f32()?, self.f32()?]),
            0x000c_0000 => Value::Vector3([self.f32()?, self.f32()?, self.f32()?]),
            0x000d_0000 => Value::Vector4([self.f32()?, self.f32()?, self.f32()?, self.f32()?]),
            _ => return self.fail(&format!("unsupported dynamic property kind {kind:08x}")),
        };
        Ok(Property {
            offset,
            data_type: format!("{data_type:08x}"),
            kind: format!("{kind:08x}"),
            extra,
            value,
        })
    }
    fn solver(&mut self) -> Result<MaterialSolver> {
        let id = self.r.u64()?;
        let class = self.r.u32()?;
        if class != CLASS_MATERIAL_SOLVER {
            return self.fail(&format!("unsupported path-node solver {class:08x}"));
        }
        let filtering_by_blend_mode = self.boolean()?;
        let target_blend_mode = self.r.i32()?;
        let filtering_by_alpha_test = self.boolean()?;
        let target_alpha_test = self.boolean()?;
        // Unity/Syndicate branch: one extra boolean before Mode.
        let match_using_material_overrider = self.boolean()?;
        let mode = self.r.i32()?;
        let use_flag_mask = self.boolean()?;
        self.header(CLASS_MASK16)?;
        let mut match_mask = [0u8; 16];
        match_mask.copy_from_slice(self.r.take(16)?);
        let target_material = self.handle()?.1;
        let target_template = self.handle()?.1;
        Ok(MaterialSolver {
            file_id: hex64(id),
            filtering_by_blend_mode,
            target_blend_mode,
            filtering_by_alpha_test,
            target_alpha_test,
            match_using_material_overrider,
            mode,
            use_flag_mask,
            match_mask,
            target_material,
            target_template,
        })
    }
    fn node(&mut self) -> Result<PathNode> {
        self.header(CLASS_PROPERTY_PATH_NODE)?;
        let index = self.r.i16()?;
        let condition = self.r.u32()?;
        let condition2 = self.r.u32()?;
        let match_all_conditions = self.boolean()?;
        let array_multi_targetting = self.boolean()?;
        let optional = self.boolean()?;
        let target_property = format!("{:08x}", self.r.u32()?);
        let condition_property = format!("{:08x}", self.r.u32()?);
        let condition_data = self.r.u64()?;
        let condition2_property = format!("{:08x}", self.r.u32()?);
        let condition2_data = self.r.u64()?;
        let solver_flag = self.r.u8()?;
        let solver = if solver_flag == 0 {
            Some(self.solver()?)
        } else {
            None
        };
        Ok(PathNode {
            index,
            condition,
            condition2,
            match_all_conditions,
            array_multi_targetting,
            optional,
            target_property,
            condition_property,
            condition_data,
            condition2_property,
            condition2_data,
            solver_flag,
            solver,
        })
    }
    fn column(&mut self) -> Result<Column> {
        let file_id = self.header(CLASS_COLUMN)?;
        let pass_id = self.r.u32()?;
        self.header(CLASS_PROPERTY_PATH)?;
        let n = self.count(54)?;
        let path = (0..n).map(|_| self.node()).collect::<Result<Vec<_>>>()?;
        let target_unique = self.boolean()?;
        let set_whole_array = self.boolean()?;
        let index = self.r.i32()?;
        if self.count(1)? != 0 {
            return self.fail("unsupported entity position selections");
        }
        self.pointer()?;
        let n = self.count(16)?;
        let mut properties = Vec::with_capacity(n);
        for _ in 0..n {
            let key = self.r.u32()?;
            properties.push((key, self.property()?));
        }
        Ok(Column {
            file_id,
            pass_id,
            path,
            target_unique,
            set_whole_array,
            index,
            properties,
        })
    }
    fn row(&mut self) -> Result<Row> {
        let file_id = self.header(CLASS_ROW)?;
        self.pointer()?;
        let weight = self.f32()?;
        let tag = self.tag()?;
        let possible_tags = self.tags()?;
        let n = self.count(8)?;
        let dependencies = (0..n)
            .map(|_| Ok(hex64(self.r.u64()?)))
            .collect::<Result<Vec<_>>>()?;
        let n = self.count(16)?;
        let mut components: Vec<(i32, Property)> = Vec::with_capacity(n);
        for _ in 0..n {
            let key = self.r.i32()?;
            if components.iter().any(|(k, _)| *k == key) {
                return self.fail("duplicate row component");
            }
            components.push((key, self.property()?));
        }
        Ok(Row {
            file_id,
            weight,
            tag,
            possible_tags,
            dependencies,
            components,
        })
    }
    fn selector(&mut self) -> Result<()> {
        self.header(CLASS_SELECTOR)?;
        self.tags()?;
        self.r.u32()?; // column mask
        self.r.u32()?; // random seed
        let n = self.count(1)?;
        for _ in 0..n {
            self.pointer()?;
        }
        self.handle()?; // entity builder
        let n = self.count(9)?;
        for _ in 0..n {
            self.handle()?;
        }
        Ok(())
    }
}

pub fn parse(payload: &[u8], expected_id: &str) -> Result<BuildTable> {
    let mut c = Cursor {
        r: Reader::new(payload),
        len: payload.len(),
    };
    if c.r.u8()? != 1 {
        return c.fail("unsupported resource marker");
    }
    let file_id = c.header(CLASS_BUILD_TABLE)?;
    if file_id != expected_id {
        return invalid(format!(
            "build table id {file_id} != expected {expected_id}"
        ));
    }
    let n = c.count(12)?;
    let columns = (0..n).map(|_| c.column()).collect::<Result<Vec<_>>>()?;
    for (i, col) in columns.iter().enumerate() {
        if columns[..i].iter().any(|o| o.index == col.index) {
            return c.fail("duplicate column index");
        }
    }
    let n = c.count(12)?;
    let rows = (0..n).map(|_| c.row()).collect::<Result<Vec<_>>>()?;
    for row in &rows {
        if row
            .components
            .iter()
            .any(|(k, _)| !columns.iter().any(|col| col.index == *k))
        {
            return c.fail("row component references undefined column");
        }
    }
    let shuffle = c.boolean()?;
    c.selector()?;
    for _ in 0..4 {
        c.boolean()?;
    }
    let n = c.count(9)?;
    let subtables = (0..n)
        .map(|_| Ok(c.handle()?.1))
        .collect::<Result<Vec<_>>>()?;
    if c.r.remaining() != 0 {
        return c.fail("trailing bytes");
    }
    Ok(BuildTable {
        file_id,
        columns,
        rows,
        shuffle,
        subtables,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Minimal synthetic writer for the supported layout (independent of the parser).
    struct W(Vec<u8>);
    impl W {
        fn u8(&mut self, v: u8) -> &mut Self {
            self.0.push(v);
            self
        }
        fn u32(&mut self, v: u32) -> &mut Self {
            self.0.extend(v.to_le_bytes());
            self
        }
        fn i32(&mut self, v: i32) -> &mut Self {
            self.0.extend(v.to_le_bytes());
            self
        }
        fn u64(&mut self, v: u64) -> &mut Self {
            self.0.extend(v.to_le_bytes());
            self
        }
        fn f32(&mut self, v: f32) -> &mut Self {
            self.0.extend(v.to_le_bytes());
            self
        }
        fn hdr(&mut self, id: u64, class: u32) -> &mut Self {
            self.u64(id).u32(class)
        }
        fn node(&mut self, target: u32, solver: Option<u8>) -> &mut Self {
            self.hdr(0, CLASS_PROPERTY_PATH_NODE);
            self.0.extend([0xff, 0xff]); // index -1
            self.u32(0)
                .u32(0)
                .u8(1)
                .u8(0)
                .u8(0)
                .u32(target)
                .u32(0)
                .u64(0)
                .u32(0)
                .u64(0);
            match solver {
                None => self.u8(3),
                Some(bit) => {
                    self.u8(0)
                        .hdr(7, CLASS_MATERIAL_SOLVER)
                        .u8(0)
                        .i32(0)
                        .u8(0)
                        .u8(0)
                        .u8(0)
                        .i32(1)
                        .u8(0);
                    self.hdr(0, CLASS_MASK16);
                    let mut m = [0u8; 16];
                    m[9] = bit;
                    self.0.extend(m);
                    self.u8(0).u64(0).u8(0).u64(0)
                }
            }
        }
    }

    fn table(vector: [f32; 4], solver_class_override: Option<u32>) -> Vec<u8> {
        let mut w = W(Vec::new());
        w.u8(1).hdr(0x1e7c52e192, CLASS_BUILD_TABLE).i32(1);
        // one column: path [solver(mask bit 9), target b000af3e], one vector4 property
        w.hdr(0x10, CLASS_COLUMN)
            .u32(4)
            .hdr(0, CLASS_PROPERTY_PATH)
            .i32(2);
        w.node(0, Some(1));
        if let Some(class) = solver_class_override {
            let needle = CLASS_MATERIAL_SOLVER.to_le_bytes();
            let at = w.0.windows(4).position(|x| x == needle).unwrap();
            w.0[at..at + 4].copy_from_slice(&class.to_le_bytes());
        }
        w.node(0xb000af3e, None);
        w.u8(1).u8(0).i32(1).i32(0).u8(1).u64(0x1e7c52e192).i32(1);
        w.u32(0x2ced_f817)
            .u32(0)
            .u32(0x000d_0000)
            .u32(0)
            .f32(1.0)
            .f32(1.0)
            .f32(1.0)
            .f32(0.0);
        // one row writing column 1
        w.i32(1)
            .hdr(0x20, CLASS_ROW)
            .u8(1)
            .u64(0x1e7c52e192)
            .f32(1.0);
        w.hdr(0, CLASS_TAG)
            .u32(0x84f0c283)
            .hdr(0x21, CLASS_TAGS)
            .i32(0)
            .i32(0)
            .i32(1);
        w.i32(1).u32(0).u32(0x000d_0000).u32(0);
        for v in vector {
            w.f32(v);
        }
        // trailer
        w.u8(0)
            .hdr(0x30, CLASS_SELECTOR)
            .hdr(0x31, CLASS_TAGS)
            .i32(0)
            .u32(0)
            .u32(0)
            .i32(0);
        w.u8(0).u64(0).i32(0).u8(0).u8(0).u8(0).u8(0).i32(0);
        w.0
    }

    #[test]
    fn decodes_material_path_column_and_vector_row() {
        let t = parse(&table([0.62, 1.2, 1.0, 0.0], None), "0000001e7c52e192").unwrap();
        let col = t.column(1).unwrap();
        assert_eq!(col.path.len(), 2);
        let solver = col.path[0].solver.as_ref().unwrap();
        assert_eq!(solver.mode, 1);
        assert_eq!(solver.match_mask[9], 1);
        assert_eq!(col.path[1].target_property, "b000af3e");
        assert!(col.path[1].solver.is_none());
        assert_eq!(t.rows[0].tag, 0x84f0c283);
        assert_eq!(
            t.rows[0].components[0].1.value,
            Value::Vector4([0.62, 1.2, 1.0, 0.0])
        );
    }

    #[test]
    fn rejects_truncation_trailing_and_unknown_solver() {
        let good = table([1.0; 4], None);
        for cut in [1, 10, good.len() / 2, good.len() - 1] {
            assert!(
                parse(&good[..cut], "0000001e7c52e192").is_err(),
                "cut {cut}"
            );
        }
        let mut long = good.clone();
        long.push(0);
        assert!(
            parse(&long, "0000001e7c52e192")
                .unwrap_err()
                .to_string()
                .contains("trailing")
        );
        let bad = table([1.0; 4], Some(0x1234_5678));
        assert!(
            parse(&bad, "0000001e7c52e192")
                .unwrap_err()
                .to_string()
                .contains("solver")
        );
        assert!(parse(&good, "0000001e7c52e193").is_err());
    }

    #[test]
    fn rejects_nonfinite_values() {
        let t = table([f32::NAN, 0.0, 0.0, 0.0], None);
        assert!(
            parse(&t, "0000001e7c52e192")
                .unwrap_err()
                .to_string()
                .contains("nonfinite")
        );
    }
}
