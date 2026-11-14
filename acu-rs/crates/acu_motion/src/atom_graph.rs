//! Native AtomGraph (NewDemo) locomotion interpreter. Bevy-free.
//!
//! Evidence: ACU-RE-0066 (stream, state machines, transition order, condition kinds),
//! ACU-RE-0067 (live parameters, axial blend, sync blend), ACU-RE-0068 (clip event intervals),
//! ACU-RE-0069 (node data pass, ports, condition windows, measured crossfade).
//!
//! Implementation choices that are **not** native evidence are marked `CHOICE:` and reported
//! through [`Diagnostics`] so that a run can never present them as parity.
use acu_assets::atom_stream::{Obj, Val};
use std::collections::HashMap;

/// Native "no blend" sentinel weight compare and float tolerance (`0x1439D71C8`, 1e-5).
const EPS: f32 = 0.000_01;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Value {
    Bool(bool),
    Int(i32),
    Scalar(f32),
    Vector3([f32; 3]),
    Quat([f32; 4]),
    None,
}

impl Value {
    pub fn scalar(self) -> f32 {
        match self {
            Value::Scalar(v) => v,
            Value::Int(v) => v as f32,
            Value::Bool(b) => f32::from(u8::from(b)),
            _ => 0.0,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Param {
    pub name: String,
    pub ty: String,
    /// Value-block offset (`f20`), ACU-RE-0067 obs. 3.
    pub offset: u32,
    pub default: Value,
}

/// One `AtomCondition` (loader `0x1400C4F20`).
#[derive(Clone, Debug)]
pub struct Cond {
    pub kind: u32,
    pub op: u32,
    pub join: u32,
    pub second_param: bool,
    /// Event mode (`f8` bits 9-10); 2 = absent or ending (`sub_1400C7500`).
    pub mode: u32,
    pub id: u32,
    pub value: Wrapped,
    pub children: Vec<Cond>,
}

/// `AtomDataContainerWrapper` (loader `0x1400C5210`).
#[derive(Clone, Copy, Debug, Default)]
pub struct Wrapped {
    pub ty: u32,
    pub i: u32,
    pub j: u32,
}

impl Wrapped {
    fn value(self) -> Value {
        match self.ty {
            0 => Value::Bool(self.j != 0),
            1 => Value::Scalar(f32::from_bits(self.i)),
            2 => Value::Int(self.j as i32),
            _ => Value::None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Transition {
    pub target: u16,
    /// `f32`: blend seconds (ACU-RE-0066 inference; ACU-RE-0069 measured 0.6 s fade for 0.6).
    pub blend: f32,
    pub expr: Option<Vec<Cond>>,
}

#[derive(Clone, Debug)]
pub struct Port {
    pub kind: u32,
    pub q: u64,
    pub j: u32,
}

#[derive(Clone, Debug)]
pub struct Node {
    pub class: &'static str,
    pub at: usize,
    pub inputs: Vec<Port>,
    pub clip: Option<u64>,
    /// Damp node: parameter hash (`f84`) and `f96` rate.
    pub damp: Option<(u32, f32)>,
    /// Time-scale node `f80`, axial-blend raw fields, etc. are read from `fields` when needed.
    pub fields: Vec<(&'static str, u64)>,
    pub ints: Vec<(&'static str, Vec<u32>)>,
    /// `AtomGraphConditionNode` expression (`f80`).
    pub expr: Option<Vec<Cond>>,
}

impl Node {
    fn field(&self, name: &str) -> u64 {
        self.fields
            .iter()
            .find(|(k, _)| *k == name)
            .map(|(_, v)| *v)
            .unwrap_or(0)
    }
    fn array(&self, name: &str) -> &[u32] {
        self.ints
            .iter()
            .find(|(k, _)| *k == name)
            .map(|(_, v)| v.as_slice())
            .unwrap_or(&[])
    }
}

#[derive(Clone, Debug)]
pub enum Body {
    Machine {
        states: Vec<usize>,
        entries: Vec<(Option<Vec<Cond>>, u16)>,
        machine: Vec<Transition>,
    },
    Graph {
        nodes: Vec<Node>,
        /// Output-port index -> node index (every non-root node has one output).
        outputs: Vec<usize>,
        root: Option<usize>,
    },
    /// `AtomLayeringStateNode`: base layer machine (`f184.f56`); masked layers (`f128`) are
    /// listed but not evaluated (CHOICE, reported).
    Layering {
        base: usize,
        layers: Vec<usize>,
    },
    Other,
}

#[derive(Clone, Debug)]
pub struct State {
    pub class: &'static str,
    pub at: usize,
    pub transitions: Vec<Transition>,
    /// `f88`: ids raised every frame while the state is active (`sub_14010E0F0`).
    pub events: Vec<u32>,
    pub body: Body,
}

pub struct Graph {
    pub params: Vec<Param>,
    pub states: Vec<State>,
    by_at: HashMap<usize, usize>,
    hash_to_param: HashMap<u32, usize>,
    /// Parent machine of each state.
    parent: Vec<Option<usize>>,
}

fn invalid<T>(m: impl Into<String>) -> Result<T, String> {
    Err(m.into())
}

fn conds(expr: Option<&Obj>) -> Option<Vec<Cond>> {
    let e = expr?;
    let flat: Vec<&Obj> = e.objs("f20");
    if flat.is_empty() {
        return None;
    }
    fn take(flat: &[&Obj], pos: &mut usize, n: usize) -> Vec<Cond> {
        let mut out = Vec::new();
        while out.len() < n && *pos < flat.len() {
            let c = flat[*pos];
            *pos += 1;
            let kind = c.u("f8.b0_2").unwrap_or(0) as u32;
            let group = c.u("f12").unwrap_or(0) as usize;
            let w = c.obj("f16");
            let wrapped = Wrapped {
                ty: w.and_then(|w| w.u("type")).unwrap_or(10) as u32,
                i: w.and_then(|w| w.u("i")).unwrap_or(0) as u32,
                j: w.and_then(|w| w.u("j")).unwrap_or(0) as u32,
            };
            let children = if kind == 2 {
                take(flat, pos, group)
            } else {
                Vec::new()
            };
            out.push(Cond {
                kind,
                op: c.u("f8.b3_6").unwrap_or(0) as u32,
                join: c.u("f8.b7_8").unwrap_or(0) as u32,
                second_param: c.u("f8.b11").unwrap_or(0) != 0,
                mode: c.u("f8.b9_10").unwrap_or(0) as u32,
                id: c.u("f0").unwrap_or(0) as u32,
                value: wrapped,
                children,
            });
        }
        out
    }
    // `sub_1400C2790`: the first condition is the root group; its f12 counts top-level items.
    let mut pos = 0;
    let root = take(&flat, &mut pos, 1);
    root.into_iter()
        .next()
        .map(|r| if r.kind == 2 { r.children } else { vec![r] })
}

fn transitions(o: &Obj, field: &str) -> Vec<Transition> {
    let mut out = Vec::new();
    if let Some(Val::Arr(items)) = o.get(field) {
        for it in items {
            if let Val::Obj(t) = it {
                out.push(Transition {
                    target: t.u("f36").unwrap_or(0) as u16,
                    blend: t
                        .f("f32")
                        .unwrap_or_else(|| f32::from_bits(t.u("f32").unwrap_or(0) as u32)),
                    expr: conds(t.obj("f8")),
                });
            }
        }
    }
    out
}

impl Graph {
    /// Build from a parsed NewDemo root (`acu_assets::atom_stream::parse`).
    pub fn from_root(root: &Obj) -> Result<Self, String> {
        let mut params = Vec::new();
        for d in root.objs("f40") {
            let ty = d.str("f8").unwrap_or("").to_string();
            let w = d.obj("f32");
            let wrapped = Wrapped {
                ty: w.and_then(|w| w.u("type")).unwrap_or(10) as u32,
                i: w.and_then(|w| w.u("i")).unwrap_or(0) as u32,
                j: w.and_then(|w| w.u("j")).unwrap_or(0) as u32,
            };
            params.push(Param {
                name: d.str("f0").unwrap_or("").to_string(),
                ty,
                offset: d.u("f20").unwrap_or(u64::from(u32::MAX)) as u32,
                default: wrapped.value(),
            });
        }
        if params.is_empty() {
            return invalid("AtomGraph has no parameter descriptors");
        }
        let hash_to_param = root
            .objs("f40")
            .iter()
            .enumerate()
            .map(|(i, d)| (d.u("f16").unwrap_or(0) as u32, i))
            .collect();
        let mut g = Graph {
            params,
            states: Vec::new(),
            by_at: HashMap::new(),
            hash_to_param,
            parent: Vec::new(),
        };
        let mut stack = vec![root];
        let mut states: Vec<&Obj> = Vec::new();
        while let Some(o) = stack.pop() {
            o.visit(&mut |x| {
                if matches!(
                    x.class,
                    "AtomStateMachineNode"
                        | "AtomStateNode"
                        | "AtomGraphStateNode"
                        | "AtomNullStateNode"
                        | "AtomLayeringStateNode"
                ) {
                    states.push(x);
                }
            });
        }
        for (i, s) in states.iter().enumerate() {
            g.by_at.insert(s.at, i);
        }
        for s in &states {
            let body = match s.class {
                "AtomStateMachineNode" => {
                    let mut kids = Vec::new();
                    for c in s.objs("f128") {
                        kids.push(*g.by_at.get(&c.at).ok_or("state child not indexed")?);
                    }
                    let entries = s
                        .objs("f140")
                        .iter()
                        .map(|e| (conds(e.obj("f0")), e.u("f8").unwrap_or(0) as u16))
                        .collect();
                    Body::Machine {
                        states: kids,
                        entries,
                        machine: transitions(s, "f152"),
                    }
                }
                "AtomGraphStateNode" => graph_body(s)?,
                "AtomLayeringStateNode" => {
                    let base = s
                        .obj("f184")
                        .and_then(|l| l.obj("f56"))
                        .and_then(|m| g.by_at.get(&m.at).copied())
                        .ok_or("layering state without base machine")?;
                    let layers = s
                        .objs("f128")
                        .iter()
                        .filter_map(|l| l.obj("f56").and_then(|m| g.by_at.get(&m.at).copied()))
                        .collect();
                    Body::Layering { base, layers }
                }
                _ => Body::Other,
            };
            g.states.push(State {
                class: s.class,
                at: s.at,
                transitions: transitions(s, "f16"),
                events: s.ints("f88").to_vec(),
                body,
            });
        }
        g.parent = vec![None; g.states.len()];
        for i in 0..g.states.len() {
            match &g.states[i].body {
                Body::Machine { states, .. } => {
                    for &c in states.clone().iter() {
                        g.parent[c] = Some(i);
                    }
                }
                Body::Layering { base, layers } => {
                    g.parent[*base] = Some(i);
                    for &l in layers.clone().iter() {
                        g.parent[l] = Some(i);
                    }
                }
                _ => {}
            }
        }
        Ok(g)
    }
    /// Whether `state` is `ancestor` or below it.
    pub fn within(&self, state: usize, ancestor: usize) -> bool {
        let mut s = Some(state);
        while let Some(x) = s {
            if x == ancestor {
                return true;
            }
            s = self.parent.get(x).copied().flatten();
        }
        false
    }
    pub fn state_at(&self, at: usize) -> Option<usize> {
        self.by_at.get(&at).copied()
    }
    pub fn param(&self, name: &str) -> Option<usize> {
        self.params.iter().position(|p| p.name == name)
    }
}

fn graph_body(s: &Obj) -> Result<Body, String> {
    let mut nodes = Vec::new();
    let mut outputs = Vec::new();
    let mut root = None;
    for (i, n) in s.objs("f128").into_iter().enumerate() {
        let inputs = n
            .objs("f8")
            .iter()
            .map(|p| Port {
                kind: p.u("kind").unwrap_or(0) as u32,
                q: p.u("q").unwrap_or(0),
                j: p.u("j").unwrap_or(0) as u32,
            })
            .collect();
        for _ in n.objs("f24") {
            outputs.push(i);
        }
        if n.class == "AtomAnimationRootNode" {
            root = Some(i);
        }
        let clip = match n.get("f80") {
            Some(Val::Ref(id)) if n.class == "AtomAnimationDataNode" => Some(*id),
            _ => None,
        };
        let damp = (n.class == "AtomDampRTCPNode").then(|| {
            (
                n.u("f84").unwrap_or(0) as u32,
                f32::from_bits(n.u("f96").unwrap_or(0) as u32),
            )
        });
        let fields = n
            .fields
            .iter()
            .filter_map(|(k, v)| match v {
                Val::U(x) => Some((*k, *x)),
                _ => None,
            })
            .collect();
        nodes.push(Node {
            class: n.class,
            at: n.at,
            inputs,
            clip,
            damp,
            fields,
            ints: n
                .fields
                .iter()
                .filter_map(|(k, v)| match v {
                    Val::Ints(x) => Some((*k, x.clone())),
                    _ => None,
                })
                .collect(),
            expr: if n.class == "AtomGraphConditionNode" {
                conds(n.obj("f80"))
            } else {
                None
            },
        });
    }
    Ok(Body::Graph {
        nodes,
        outputs,
        root,
    })
}

/// A clip's duration and runtime event intervals (ACU-RE-0068 step rule).
#[derive(Clone, Debug)]
pub struct ClipInfo {
    pub duration: f32,
    pub events: Vec<(u32, f32, f32)>,
}

impl ClipInfo {
    /// Build from decoded integer event tracks (codec 18/19): each key's id holds until the
    /// next key, the last until the clip end, plus the runtime end marker `0x80000002`.
    pub fn from_clip(clip: &acu_assets::animation::Clip) -> Self {
        let d = clip.duration;
        let mut events = Vec::new();
        for t in clip
            .tracks
            .iter()
            .filter(|t| t.codec == 18 || t.codec == 19)
        {
            let ids: Vec<u32> = t
                .encoded_values
                .as_chunks::<4>()
                .0
                .iter()
                .map(|c| u32::from_le_bytes(*c))
                .collect();
            for (k, &id) in ids.iter().enumerate() {
                let s = t.times.get(k).copied().unwrap_or(0.0);
                let e = t.times.get(k + 1).copied().unwrap_or(d);
                events.push((id, s, e));
            }
        }
        events.push((0x8000_0002, d, d));
        ClipInfo {
            duration: d,
            events,
        }
    }
}

/// Things the interpreter did that are not established native behaviour.
#[derive(Clone, Debug, Default)]
pub struct Diagnostics {
    pub unsupported: HashMap<&'static str, u32>,
    pub notes: HashMap<&'static str, u32>,
    /// Transitions taken: (machine payload offset, from, to, own-or-machine rule index).
    pub transitions: Vec<(usize, u16, u16, usize)>,
}

impl Diagnostics {
    fn note(&mut self, n: &'static str) {
        *self.notes.entry(n).or_default() += 1;
    }
}

/// A sampled leaf: (graph state index, node index) -> clip, weight, clip time.
#[derive(Clone, Debug, PartialEq)]
pub struct Leaf {
    pub state: usize,
    pub node: usize,
    pub clip: u64,
    pub weight: f32,
    pub time: f32,
    pub rate: f32,
}

#[derive(Clone, Debug, Default)]
struct GraphRt {
    /// Normalized phase of the root (CHOICE: native syncs by markers, ACU-RE-0069).
    phase: f32,
    started: bool,
    damp: HashMap<usize, f32>,
    /// CacheValue latches and RandomSelection picks (CHOICE: reset on graph entry).
    latch: HashMap<usize, f32>,
    pick: HashMap<usize, usize>,
}

#[derive(Clone, Debug)]
struct MachineRt {
    current: u16,
    time: f32,
    child: Box<StateRt>,
    fade: Option<Fade>,
}

#[derive(Clone, Debug)]
struct Fade {
    from: u16,
    child: Box<StateRt>,
    elapsed: f32,
    duration: f32,
}

#[derive(Clone, Debug)]
enum StateRt {
    Machine(MachineRt),
    Layer(Box<StateRt>),
    Graph(GraphRt),
    Empty,
}

/// Evaluation context of one frame.
struct Ctx<'a> {
    g: &'a Graph,
    values: &'a [Value],
    clips: &'a HashMap<u64, ClipInfo>,
    /// Events gathered from the previous step, in graph time, and that step's window.
    /// (leaf graph state, id, start, end) in graph time.
    events: &'a [(usize, u32, f32, f32)],
    /// State whose own event list kind-1 conditions read (`a2[1] + 2064`); None = instance list.
    scope: Option<usize>,
    window: (f32, f32),
    diag: &'a mut Diagnostics,
    /// Global LCG state (`qword_14525FAE0 + 4`; CHOICE: seed).
    rng: &'a mut u32,
}

fn compare_f(a: f32, b: f32, op: u32) -> bool {
    // `sub_14009E7C0`
    match op {
        0 => (a - b).abs() <= EPS,
        1 => a > b,
        2 => a >= b,
        3 => a < b,
        4 => a <= b,
        5 => (a - b).abs() > EPS,
        _ => false,
    }
}
fn compare_i(a: i64, b: i64, op: u32) -> bool {
    match op {
        0 => a == b,
        1 => a > b,
        2 => a >= b,
        3 => a < b,
        4 => a <= b,
        5 => a != b,
        _ => false,
    }
}

impl Ctx<'_> {
    /// `sub_1400C23D0`: Some(window) when the condition holds.
    fn cond(&mut self, c: &Cond, win: (f32, f32), st: (f32, f32)) -> Option<(f32, f32)> {
        match c.kind {
            0 => {
                if c.second_param {
                    self.diag
                        .unsupported
                        .entry("condition second-parameter compare")
                        .and_modify(|n| *n += 1)
                        .or_insert(1);
                    return None;
                }
                let v = *self.values.get(c.id as usize)?;
                let r = c.value.value();
                let ok = match (v, r) {
                    (Value::Bool(a), Value::Bool(b)) => compare_i(a.into(), b.into(), c.op),
                    (Value::Int(a), Value::Int(b)) => compare_i(a.into(), b.into(), c.op),
                    (Value::Scalar(a), Value::Scalar(b)) => compare_f(a, b, c.op),
                    (Value::Int(a), Value::Scalar(b)) => compare_f(a as f32, b, c.op),
                    (Value::Scalar(a), Value::Int(b)) => compare_f(a, b as f32, c.op),
                    (Value::Bool(a), Value::Int(b)) => compare_i(a.into(), b.into(), c.op),
                    _ => false,
                };
                ok.then_some(win)
            }
            2 => self.chain(&c.children, win, st),
            3 => {
                // sub_1400C23D0 kind 3: state time at frame end passes the threshold; the
                // window is the part of the frame after the crossing.
                let thr = f32::from_bits(c.value.i);
                if st.1 <= thr {
                    return None;
                }
                let span = st.1 - st.0;
                if span.abs() > EPS {
                    let f = (thr - st.0).max(0.0) / span;
                    Some((win.0 + (win.1 - win.0) * f, win.1))
                } else {
                    Some(win)
                }
            }
            1 | 4 => {
                // sub_1400C23D0 -> sub_1400C7500. Kind 1 reads the evaluating state's list over
                // the frame window; kind 4 the instance list over an unbounded window.
                let (scope, w) = if c.kind == 1 {
                    (self.scope, win)
                } else {
                    (None, (f32::MIN, f32::MAX))
                };
                let mut hit: Option<(f32, f32)> = None;
                for &(state, id, s, e) in self.events {
                    if id != c.id || scope.is_some_and(|sc| !self.g.within(state, sc)) {
                        continue;
                    }
                    if s - w.1 < EPS && w.0 - e < EPS {
                        hit = Some(match hit {
                            None => (s, e),
                            Some((a, b)) => (a.min(s), b.max(e)),
                        });
                    }
                }
                if c.mode == 2 {
                    // Absent: whole window. Present: holds only when it ends inside the window.
                    return match hit {
                        None => Some(w),
                        Some(_) if w.0 == f32::MIN => None,
                        Some((_, e)) => (e + EPS < w.1).then_some((e, w.1)),
                    }
                    .map(|(a, b)| if c.kind == 4 { win } else { (a, b) });
                }
                match hit {
                    // The caller intersects with the found interval (sub_1400C23D0 kind 1).
                    Some((a, b)) if c.kind == 1 => Some((win.0.max(a), win.1.min(b))),
                    Some(_) => Some(win),
                    None => None,
                }
            }
            _ => None,
        }
    }

    /// `sub_1400C3400`: strictly left to right, no precedence; windows intersect under `and`.
    fn chain(&mut self, items: &[Cond], win: (f32, f32), st: (f32, f32)) -> Option<(f32, f32)> {
        if items.is_empty() {
            return Some(win);
        }
        let (orig_lo, orig_hi) = win;
        let (mut cur_lo, mut cur_hi) = win;
        let (mut lo, mut hi) = win;
        let mut ok = true;
        let mut any = false;
        let mut join = 0;
        for c in items {
            if join == 0 {
                if ok {
                    match self.cond(c, (cur_lo, cur_hi), st) {
                        Some((a, b)) => {
                            lo = lo.max(a);
                            hi = hi.min(b);
                            cur_lo = lo;
                            cur_hi = hi;
                            any = true;
                            if lo - hi > EPS {
                                ok = false;
                            }
                        }
                        None => ok = false,
                    }
                }
            } else if join == 1 && !ok {
                cur_lo = orig_lo;
                cur_hi = orig_hi;
                if let Some((a, b)) = self.cond(c, (orig_lo, orig_hi), st) {
                    ok = true;
                    if !any {
                        lo = a;
                        hi = b;
                        any = true;
                    } else {
                        lo = lo.min(a);
                        hi = hi.max(b);
                    }
                    if orig_lo > a {
                        cur_lo = a;
                    }
                    if orig_hi < b {
                        cur_hi = b;
                    }
                }
            }
            join = c.join;
        }
        ok.then_some((lo, hi))
    }

    fn expr(&mut self, e: &Option<Vec<Cond>>, st: (f32, f32)) -> bool {
        match e {
            None => true,
            Some(items) => self.chain(items, self.window, st).is_some(),
        }
    }
}

/// The interpreter: one root machine and its runtime.
pub struct Player {
    pub graph: Graph,
    pub root: usize,
    pub values: Vec<Value>,
    pub clips: HashMap<u64, ClipInfo>,
    pub diag: Diagnostics,
    rt: StateRt,
    events: Vec<(usize, u32, f32, f32)>,
    window: (f32, f32),
    clock: f32,
    pub rng: u32,
    pub leaves: Vec<Leaf>,
}

/// Measured crossfade shape (ACU-RE-0069): smoothstep over the transition's seconds.
fn fade_weight(elapsed: f32, duration: f32) -> f32 {
    if duration <= EPS {
        return 1.0;
    }
    let t = (elapsed / duration).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

impl Player {
    pub fn new(graph: Graph, root: usize, clips: HashMap<u64, ClipInfo>) -> Result<Self, String> {
        if !matches!(
            graph.states.get(root).map(|s| &s.body),
            Some(Body::Machine { .. })
        ) {
            return invalid("root state is not a state machine");
        }
        let values = graph.params.iter().map(|p| p.default).collect();
        Ok(Player {
            graph,
            root,
            values,
            clips,
            diag: Diagnostics::default(),
            rt: StateRt::Empty,
            events: Vec::new(),
            window: (0.0, 0.0),
            clock: 0.0,
            rng: 1,
            leaves: Vec::new(),
        })
    }

    pub fn set(&mut self, name: &str, v: Value) -> bool {
        match self.graph.param(name) {
            Some(i) => {
                self.values[i] = v;
                true
            }
            None => false,
        }
    }

    /// Enter the machines along `path` (child indices from the root) directly; below the path
    /// the normal entry rules apply. Used to start where an observed run started; this is not a
    /// native entry path.
    pub fn start_in(&mut self, path: &[u16]) {
        let mut ctx = Ctx {
            g: &self.graph,
            values: &self.values,
            clips: &self.clips,
            events: &[],
            window: self.window,
            scope: None,
            diag: &mut self.diag,
            rng: &mut self.rng,
        };
        fn forced(ctx: &mut Ctx, s: usize, path: &[u16]) -> StateRt {
            let (Some(&current), Body::Machine { states, .. }) =
                (path.first(), &ctx.g.states[s].body)
            else {
                return enter(ctx, s);
            };
            let Some(&child) = states.get(current as usize) else {
                return enter(ctx, s);
            };
            StateRt::Machine(MachineRt {
                current,
                time: 0.0,
                child: Box::new(forced(ctx, child, &path[1..])),
                fade: None,
            })
        }
        self.rt = forced(&mut ctx, self.root, path);
    }

    /// Active state path from the root (state indices), for inspection.
    pub fn path(&self) -> Vec<usize> {
        let mut out = vec![self.root];
        let mut rt = &self.rt;
        let mut s = self.root;
        loop {
            match (rt, &self.graph.states[s].body) {
                (StateRt::Machine(m), Body::Machine { states, .. }) => {
                    s = states[m.current as usize];
                    rt = &m.child;
                }
                (StateRt::Layer(inner), Body::Layering { base, .. }) => {
                    s = *base;
                    rt = inner;
                }
                _ => break,
            }
            out.push(s);
        }
        out
    }

    /// Advance one step of `dt` seconds and rebuild [`Player::leaves`].
    pub fn step(&mut self, dt: f32) {
        let mut rt = std::mem::replace(&mut self.rt, StateRt::Empty);
        let events = std::mem::take(&mut self.events);
        {
            let mut ctx = Ctx {
                g: &self.graph,
                values: &self.values,
                clips: &self.clips,
                events: &events,
                window: self.window,
                scope: None,
                diag: &mut self.diag,
                rng: &mut self.rng,
            };
            if matches!(rt, StateRt::Empty) {
                rt = enter(&mut ctx, self.root);
            }
            update(&mut ctx, self.root, &mut rt, dt);
        }
        // Graph pass: collect weighted leaves and advance clip phases.
        let mut leaves = Vec::new();
        {
            let mut ctx = Ctx {
                g: &self.graph,
                values: &self.values,
                clips: &self.clips,
                events: &[],
                window: (0.0, 0.0),
                scope: None,
                diag: &mut self.diag,
                rng: &mut self.rng,
            };
            collect(&mut ctx, self.root, &mut rt, 1.0, dt, &mut leaves);
        }
        self.rt = rt;
        // Event list of this step, in graph time [clock, clock + dt] (sub_1400CAD80).
        let t0 = self.clock;
        let mut ev = Vec::new();
        for l in &leaves {
            let Some(info) = self.clips.get(&l.clip) else {
                continue;
            };
            let c1 = l.time;
            let c0 = c1 - l.rate * dt;
            for &(id, s, e) in &info.events {
                for shift in [-info.duration, 0.0] {
                    let (s, e) = (s + shift, e + shift);
                    if e + EPS >= c0 && s - EPS <= c1 {
                        let map = |c: f32| {
                            if l.rate > EPS {
                                t0 + (c - c0) / l.rate
                            } else {
                                t0
                            }
                        };
                        ev.push((l.state, id, map(s.max(c0)), map(e.min(c1))));
                    }
                }
            }
        }
        // sub_14010E0F0: every active state's f88 ids span the whole step window.
        let mut active = Vec::new();
        active_states(&self.graph, self.root, &self.rt, &mut active);
        for s in active {
            for &id in &self.graph.states[s].events {
                ev.push((s, id, t0, t0 + dt));
            }
        }
        self.events = ev;
        self.window = (t0, t0 + dt);
        self.clock += dt;
        self.leaves = leaves;
    }
}

/// Active states (current and fading) of the runtime tree.
fn active_states(g: &Graph, s: usize, rt: &StateRt, out: &mut Vec<usize>) {
    out.push(s);
    match (rt, &g.states[s].body) {
        (StateRt::Machine(m), Body::Machine { states, .. }) => {
            if let Some(f) = &m.fade {
                active_states(g, states[f.from as usize], &f.child, out);
            }
            active_states(g, states[m.current as usize], &m.child, out);
        }
        (StateRt::Layer(inner), Body::Layering { base, .. }) => active_states(g, *base, inner, out),
        _ => {}
    }
}

fn enter(ctx: &mut Ctx, s: usize) -> StateRt {
    match &ctx.g.states[s].body {
        Body::Machine {
            states, entries, ..
        } => {
            let mut target = None;
            for (e, t) in entries {
                if ctx.expr(e, (0.0, 0.0)) {
                    target = Some(*t);
                    break;
                }
            }
            let current = target.unwrap_or_else(|| {
                ctx.diag.note("no entry rule passed: state 0 (CHOICE)");
                0
            });
            let child = states
                .get(current as usize)
                .map(|&c| enter(ctx, c))
                .unwrap_or(StateRt::Empty);
            StateRt::Machine(MachineRt {
                current,
                time: 0.0,
                child: Box::new(child),
                fade: None,
            })
        }
        Body::Graph { .. } => StateRt::Graph(GraphRt::default()),
        Body::Layering { base, layers } => {
            if !layers.is_empty() {
                ctx.diag
                    .note("masked layers of a layering state not evaluated");
            }
            let base = *base;
            StateRt::Layer(Box::new(enter(ctx, base)))
        }
        Body::Other => {
            *ctx.diag
                .unsupported
                .entry(ctx.g.states[s].class)
                .or_default() += 1;
            StateRt::Empty
        }
    }
}

fn update(ctx: &mut Ctx, s: usize, rt: &mut StateRt, dt: f32) {
    if let (StateRt::Layer(inner), Body::Layering { base, .. }) = (&mut *rt, &ctx.g.states[s].body)
    {
        let base = *base;
        return update(ctx, base, inner, dt);
    }
    let StateRt::Machine(m) = rt else {
        return;
    };
    let Body::Machine {
        states, machine, ..
    } = &ctx.g.states[s].body
    else {
        return;
    };
    let cur_state = states[m.current as usize];
    // sub_14010EAE0 + sub_1401260C0: own transitions first, then machine transitions.
    let own = &ctx.g.states[cur_state].transitions;
    ctx.scope = Some(cur_state);
    let mut chosen = None;
    for (rule, t) in own
        .iter()
        .chain(machine.iter().filter(|t| t.target != m.current))
        .enumerate()
    {
        if t.target as usize >= states.len() {
            ctx.diag.note("transition to a special target skipped");
            continue;
        }
        if ctx.expr(&t.expr, (m.time, m.time + dt)) {
            ctx.diag
                .transitions
                .push((ctx.g.states[s].at, m.current, t.target, rule));
            chosen = Some(t.clone());
            break;
        }
    }
    if let Some(t) = chosen {
        let child = enter(ctx, states[t.target as usize]);
        let old = std::mem::replace(&mut m.child, Box::new(child));
        m.fade = Some(Fade {
            from: m.current,
            child: old,
            elapsed: 0.0,
            duration: t.blend,
        });
        m.current = t.target;
        m.time = 0.0;
    } else {
        m.time += dt;
    }
    if let Some(f) = &mut m.fade {
        f.elapsed += dt;
        if f.elapsed >= f.duration {
            m.fade = None;
        }
    }
    let cur_state = states[m.current as usize];
    update(ctx, cur_state, &mut m.child, dt);
}

fn collect(ctx: &mut Ctx, s: usize, rt: &mut StateRt, w: f32, dt: f32, out: &mut Vec<Leaf>) {
    match rt {
        StateRt::Machine(m) => {
            let Body::Machine { states, .. } = &ctx.g.states[s].body else {
                return;
            };
            let a = m
                .fade
                .as_ref()
                .map(|f| fade_weight(f.elapsed, f.duration))
                .unwrap_or(1.0);
            if let Some(f) = &mut m.fade {
                let from = states[f.from as usize];
                collect(ctx, from, &mut f.child, w * (1.0 - a), dt, out);
            }
            let cur = states[m.current as usize];
            collect(ctx, cur, &mut m.child, w * a, dt, out);
        }
        StateRt::Graph(g) => graph_leaves(ctx, s, g, w, dt, out),
        StateRt::Layer(inner) => {
            if let Body::Layering { base, .. } = &ctx.g.states[s].body {
                let base = *base;
                collect(ctx, base, inner, w, dt, out);
            }
        }
        StateRt::Empty => {}
    }
}

/// A pose value of the data pass: weighted leaves and the sync duration (+140).
#[derive(Default)]
struct Pose {
    leaves: Vec<(usize, f32)>,
    duration: f32,
}

fn graph_leaves(ctx: &mut Ctx, s: usize, g: &mut GraphRt, w: f32, dt: f32, out: &mut Vec<Leaf>) {
    let Body::Graph {
        nodes,
        outputs,
        root,
    } = &ctx.g.states[s].body
    else {
        return;
    };
    let Some(root) = *root else {
        ctx.diag.note("graph state without root node");
        return;
    };
    let Some(p0) = nodes[root].inputs.first() else {
        return;
    };
    let mut ev = Eval {
        ctx,
        nodes,
        outputs,
        damp: &mut g.damp,
        latch: &mut g.latch,
        pick: &mut g.pick,
    };
    let pose = ev.pose(p0);
    if pose.duration <= EPS {
        return;
    }
    if g.started {
        g.phase += dt / pose.duration;
    }
    g.started = true;
    let phase = g.phase;
    for (n, lw) in pose.leaves {
        let clip = nodes[n].clip.unwrap_or(0);
        let d = ev.ctx.clips.get(&clip).map(|c| c.duration).unwrap_or(0.0);
        // CHOICE: every clip loops at the shared normalized phase.
        let time = (phase * d).rem_euclid(d.max(EPS));
        out.push(Leaf {
            state: s,
            node: n,
            clip,
            weight: w * lw,
            time,
            rate: d / pose.duration,
        });
    }
}

struct Eval<'c, 'a> {
    ctx: &'c mut Ctx<'a>,
    nodes: &'c [Node],
    outputs: &'c [usize],
    damp: &'c mut HashMap<usize, f32>,
    latch: &'c mut HashMap<usize, f32>,
    pick: &'c mut HashMap<usize, usize>,
}

impl Eval<'_, '_> {
    fn linked(&self, p: &Port) -> Option<usize> {
        matches!(p.kind, 9 | 11)
            .then(|| self.outputs.get(p.q as usize).copied())
            .flatten()
    }
    /// `sub_1400A26E0` and the per-type port reads.
    fn scalar(&mut self, p: &Port) -> f32 {
        match p.kind {
            0 => self
                .ctx
                .values
                .get(p.q as usize)
                .map(|v| v.scalar())
                .unwrap_or(0.0),
            2 => f32::from(u8::from(p.q != 0)),
            3 => f32::from_bits(p.j),
            4 => p.q as u32 as i32 as f32,
            9 => match self.linked(p) {
                Some(n) => self.node_scalar(n),
                None => 0.0,
            },
            11 => match self.linked(p) {
                Some(n) => self.node_pose(n).duration,
                None => 0.0,
            },
            _ => 0.0,
        }
    }
    fn flag(&mut self, p: &Port) -> bool {
        match p.kind {
            2 => p.q != 0,
            _ => self.scalar(p) != 0.0,
        }
    }
    fn pose(&mut self, p: &Port) -> Pose {
        match self.linked(p) {
            Some(n) => self.node_pose(n),
            None => Pose::default(),
        }
    }
    fn node_scalar(&mut self, n: usize) -> f32 {
        let node = &self.nodes[n];
        match node.class {
            "AtomDampRTCPNode" => {
                let (hash, _rate) = node.damp.unwrap_or((0, 0.0));
                let target = self
                    .ctx
                    .g
                    .hash_to_param
                    .get(&hash)
                    .and_then(|&i| self.ctx.values.get(i))
                    .map(|v| v.scalar())
                    .unwrap_or(0.0);
                // CHOICE: the damp-table updater is not located (ACU-RE-0069); pass through.
                self.ctx.diag.note("damp node passes its target through");
                self.damp.insert(n, target);
                target
            }
            "AtomCacheValueNode" => {
                // sub_1400EAF90: computes once, then holds (flag at f84 is never cleared here).
                if let Some(v) = self.latch.get(&n) {
                    return *v;
                }
                let p = self.nodes[n].inputs.first().cloned();
                let v = p.map(|p| self.scalar(&p)).unwrap_or(0.0);
                self.latch.insert(n, v);
                v
            }
            "AtomCompareValuesNode" => {
                // sub_1400EB4D0 -> sub_14009DAC0 (type 1): port0 operator, a = port1, b = port2.
                let inputs = self.nodes[n].inputs.clone();
                if inputs.len() < 3 {
                    return 0.0;
                }
                if self.nodes[n].field("f80") != 1 {
                    self.ctx
                        .diag
                        .note("compare-values non-float type compared as float");
                }
                let op = self.scalar(&inputs[0]) as u32;
                let (a, b) = (self.scalar(&inputs[1]), self.scalar(&inputs[2]));
                f32::from(u8::from(compare_f(a, b, op)))
            }
            "AtomGraphConditionNode" => {
                // sub_1400EB6B0 (CHOICE: evaluated every frame against the step's events).
                let e = self.nodes[n].expr.clone();
                let scope = self.ctx.scope.take();
                let r = self.ctx.expr(&e, (0.0, 0.0));
                self.ctx.scope = scope;
                f32::from(u8::from(r))
            }
            "AtomIfNode" => {
                let inputs = self.nodes[n].inputs.clone();
                if inputs.len() < 3 {
                    return 0.0;
                }
                let c = self.flag(&inputs[0]);
                self.scalar(&inputs[if c { 1 } else { 2 }])
            }
            c => {
                *self.ctx.diag.unsupported.entry(c).or_default() += 1;
                0.0
            }
        }
    }
    fn node_pose(&mut self, n: usize) -> Pose {
        let node = &self.nodes[n];
        match node.class {
            "AtomAnimationDataNode" => {
                let d = node
                    .clip
                    .and_then(|c| self.ctx.clips.get(&c))
                    .map(|c| c.duration)
                    .unwrap_or(0.0);
                if d <= EPS {
                    self.ctx.diag.note("clip absent from the loaded package");
                }
                Pose {
                    leaves: vec![(n, 1.0)],
                    duration: d,
                }
            }
            "AtomAxialBlendNode" => self.axial(n, false),
            "AtomAngularBlendNode" => self.axial(n, true),
            "AtomIfNode" => {
                // sub_1400EB790 type 7: port 1 when the condition holds, else port 2.
                let inputs = self.nodes[n].inputs.clone();
                if inputs.len() < 3 {
                    return Pose::default();
                }
                let c = self.flag(&inputs[0]);
                self.pose(&inputs[if c { 1 } else { 2 }])
            }
            "AtomSelectFromIntervalNode" => {
                // sub_1400ECF70 / sub_1400A0730: first k with x < t[k]; child port 2k + 1.
                let inputs = self.nodes[n].inputs.clone();
                if self.nodes[n].field("f80") != 1 {
                    *self
                        .ctx
                        .diag
                        .unsupported
                        .entry("SelectFromInterval int mode")
                        .or_default() += 1;
                    return Pose::default();
                }
                let x = self.scalar(&inputs[0]);
                let count = inputs.len() / 2 - 1;
                let mut k = 0;
                while k < count {
                    let t = self.scalar(&inputs[2 * k + 2]);
                    if x < t {
                        break;
                    }
                    k += 1;
                }
                match inputs.get(2 * k + 1).cloned() {
                    Some(p) => self.pose(&p),
                    None => Pose::default(),
                }
            }
            "AtomRandomSelectionNode" => {
                // sub_1400EC860 / sub_1400E09B0: weighted pick from the global LCG.
                let k = match self.pick.get(&n) {
                    Some(&k) => k,
                    None => {
                        let w: Vec<f32> = self.nodes[n]
                            .array("f80")
                            .iter()
                            .map(|&b| f32::from_bits(b))
                            .collect();
                        if self.nodes[n].field("f174.b0") != 0
                            || self.nodes[n].field("f174.b2_3") != 0
                        {
                            self.ctx
                                .diag
                                .note("random selection repeat/refill modes not modelled");
                        }
                        let total: f32 = w.iter().sum();
                        *self.ctx.rng = self
                            .ctx
                            .rng
                            .wrapping_mul(1_664_525)
                            .wrapping_add(1_013_904_223);
                        let mut r =
                            ((*self.ctx.rng >> 8) as i32 as f32 * 0.000_000_059_604_65) * total;
                        let mut k = w.len().saturating_sub(1);
                        for (i, &wi) in w.iter().enumerate() {
                            if wi >= r {
                                k = i;
                                break;
                            }
                            r -= wi;
                        }
                        self.ctx
                            .diag
                            .note("random selection latched per graph entry (CHOICE)");
                        self.pick.insert(n, k);
                        k
                    }
                };
                let p = self.nodes[n].inputs.get(k).cloned();
                p.map(|p| self.pose(&p)).unwrap_or_default()
            }
            "AtomTimeScaleNode" => {
                // sub_1400EE6B0: port0 bool (value is a duration), port1 value, port2 pose.
                let inputs = &self.nodes[n].inputs;
                let (Some(p0), Some(p1), Some(p2)) = (inputs.first(), inputs.get(1), inputs.get(2))
                else {
                    return Pose::default();
                };
                let (p0, p1, p2) = (p0.clone(), p1.clone(), p2.clone());
                let is_duration = self.flag(&p0);
                let mut v = self.scalar(&p1);
                if v <= EPS || v >= 10_000.0 {
                    v = 1.0;
                }
                let mut pose = self.pose(&p2);
                let d = pose.duration;
                let (mut new_d, ratio) = if is_duration {
                    (v, v / d)
                } else {
                    (d / v, 1.0 / v)
                };
                if new_d <= EPS || ratio <= EPS {
                    new_d = 0.1;
                }
                pose.duration = new_d;
                pose
            }
            "AtomAnimationRootNode" => {
                let p = self.nodes[n].inputs.first().cloned();
                p.map(|p| self.pose(&p)).unwrap_or_default()
            }
            c => {
                *self.ctx.diag.unsupported.entry(c).or_default() += 1;
                Pose::default()
            }
        }
    }
    /// `sub_1400EA930` with `sub_1400BDF30` (axial) or `sub_1400BDCF0` (angular), then
    /// `sub_1400FD7A0`. Returns children (a, b) blended as (1 - w)·a + w·b.
    fn axial(&mut self, n: usize, angular: bool) -> Pose {
        let inputs = self.nodes[n].inputs.clone();
        if inputs.len() < 5 {
            return Pose::default();
        }
        let count = (inputs.len() - 3) / 2;
        let threshold = |e: &mut Self, k: usize| e.scalar(&inputs[3 + 2 * k]);
        let child = |k: usize| inputs[4 + 2 * k].clone();
        let mut x = self.scalar(&inputs[0]);
        // Some((a, b, w)) to blend, or Err(k) for one child alone.
        let pick: Result<(usize, usize, f32), usize> = if count <= 1 {
            Err(0)
        } else {
            let first = threshold(self, 0);
            let mut wrapped = None;
            if angular {
                // fmodf(x, 360) - 360, then +360 below -180 (0x142E822F8, 0x142E82300).
                x = x % 360.0 - 360.0;
                if x < -180.0 {
                    x += 360.0;
                }
                let last = threshold(self, count - 1);
                if !(x >= first && x <= last) {
                    let span = (360.0 - last) + first;
                    let w = if x >= first {
                        (x - last) / span
                    } else {
                        1.0 - (first - x) / span
                    };
                    wrapped = Some(Ok((count - 1, 0, w)));
                }
            }
            wrapped.unwrap_or_else(|| {
                let mut prev = first;
                for k in 1..count {
                    let t = threshold(self, k);
                    if x < t {
                        let r = ((x - prev) / (t - prev)).clamp(0.0, 1.0);
                        return if r.abs() <= EPS {
                            Err(k - 1)
                        } else {
                            Ok((k - 1, k, r))
                        };
                    }
                    prev = t;
                }
                Err(count - 1)
            })
        };
        match pick {
            Err(k) => {
                let c = child(k);
                self.pose(&c)
            }
            Ok((ka, kb, w)) => {
                let (ca, cb) = (child(ka), child(kb));
                let a = self.pose(&ca);
                let b = self.pose(&cb);
                let mut leaves: Vec<(usize, f32)> = a
                    .leaves
                    .into_iter()
                    .map(|(n, x)| (n, x * (1.0 - w)))
                    .collect();
                leaves.extend(b.leaves.into_iter().map(|(n, x)| (n, x * w)));
                Pose {
                    leaves,
                    // +140 blended linearly (ACU-RE-0067 obs. 6)
                    duration: (1.0 - w) * a.duration + w * b.duration,
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cond(kind: u32, op: u32, join: u32, id: u32, v: Wrapped) -> Cond {
        Cond {
            kind,
            op,
            join,
            second_param: false,
            mode: 0,
            id,
            value: v,
            children: vec![],
        }
    }

    #[test]
    fn chain_is_left_to_right_without_precedence() {
        let g = Graph {
            params: vec![],
            states: vec![],
            by_at: HashMap::new(),
            hash_to_param: HashMap::new(),
            parent: vec![None],
        };
        let values = [Value::Int(1), Value::Bool(false)];
        let clips = HashMap::new();
        let mut diag = Diagnostics::default();
        let mut ctx = Ctx {
            g: &g,
            values: &values,
            clips: &clips,
            events: &[],
            window: (0.0, 1.0),
            scope: None,
            diag: &mut diag,
            rng: &mut 1,
        };
        let int1 = Wrapped { ty: 2, i: 0, j: 1 };
        let t = Wrapped { ty: 0, i: 0, j: 1 };
        // (p0 == 1) or (p1 == true) and (p1 == true)  ==  ((true or false) and false) == false
        let items = vec![
            cond(0, 0, 1, 0, int1),
            cond(0, 0, 0, 1, t),
            cond(0, 0, 0, 1, t),
        ];
        assert!(ctx.chain(&items, (0.0, 1.0), (0.0, 0.0)).is_none());
        // (p1 == true) or (p0 == 1)  ==  true
        let items = vec![cond(0, 0, 1, 1, t), cond(0, 0, 0, 0, int1)];
        assert!(ctx.chain(&items, (0.0, 1.0), (0.0, 0.0)).is_some());
    }

    #[test]
    fn event_windows_must_overlap_under_and() {
        let g = Graph {
            params: vec![],
            states: vec![],
            by_at: HashMap::new(),
            hash_to_param: HashMap::new(),
            parent: vec![None],
        };
        let clips = HashMap::new();
        let mut diag = Diagnostics::default();
        let events = [(0, 7, 0.0, 0.3), (0, 8, 0.5, 1.0), (0, 9, 0.2, 0.6)];
        let mut ctx = Ctx {
            g: &g,
            values: &[],
            clips: &clips,
            events: &events,
            window: (0.0, 1.0),
            scope: None,
            diag: &mut diag,
            rng: &mut 1,
        };
        let w = Wrapped::default();
        assert!(
            ctx.chain(
                &[cond(1, 0, 0, 7, w), cond(1, 0, 0, 8, w)],
                (0.0, 1.0),
                (0.0, 0.0)
            )
            .is_none()
        );
        assert_eq!(
            ctx.chain(
                &[cond(1, 0, 0, 7, w), cond(1, 0, 0, 9, w)],
                (0.0, 1.0),
                (0.0, 0.0)
            ),
            Some((0.2, 0.3))
        );
        assert!(
            ctx.chain(&[cond(1, 0, 0, 5, w)], (0.0, 1.0), (0.0, 0.0))
                .is_none()
        );
    }

    #[test]
    fn float_equality_uses_native_tolerance() {
        assert!(compare_f(1.0, 1.000_005, 0));
        assert!(!compare_f(1.0, 1.000_02, 0));
        assert!(compare_f(1.0, 1.000_02, 5));
    }

    #[test]
    fn fade_is_smoothstep() {
        assert_eq!(fade_weight(0.0, 0.6), 0.0);
        assert!((fade_weight(0.3, 0.6) - 0.5).abs() < 1e-6);
        assert_eq!(fade_weight(0.7, 0.6), 1.0);
        assert_eq!(fade_weight(0.0, 0.0), 1.0);
    }
}
