//! Compare `acu_motion::modifier_order::build_lod_lists` with a traced native
//! frame (input from `tools/native_tap/modifier_order_export.py`).
//!
//! Each slot's host is taken to be its bone and slots on one bone keep the
//! export's class order: the bone objects' own modifier array order was not
//! captured, so equal-key neighbours on one bone are reported as ties, not
//! mismatches. Observed = model-record stores in the first traced frame; a
//! listed slot that never stored (zero weight, early return) is reported.
//! Usage: modifier_order_check <order.json>

use acu_motion::modifier_order::{HierarchyEntry, ModifierSlot, build_lod_lists};
use serde_json::Value;

fn field(v: &Value, name: &str) -> u64 {
    v[name]
        .as_u64()
        .unwrap_or_else(|| panic!("missing integer field {name}"))
}

fn main() {
    let path = std::env::args()
        .nth(1)
        .expect("usage: modifier_order_check <order.json>");
    let text = std::fs::read_to_string(&path).expect("read order json");
    let doc: Value = serde_json::from_str(&text).expect("parse order json");
    let hierarchy: Vec<HierarchyEntry> = doc["hierarchy"]
        .as_array()
        .expect("hierarchy")
        .iter()
        .map(|e| HierarchyEntry {
            parent: field(e, "parent") as u16,
            lod: field(e, "lod") as u8,
        })
        .collect();
    let raw = doc["slots"].as_array().expect("slots");
    let class = |i: usize| raw[i]["cls"].as_str().expect("cls").to_owned();
    let mut order: Vec<usize> = (0..raw.len()).collect();
    order.sort_by_key(|&i| field(&raw[i], "bone"));
    let slots: Vec<ModifierSlot> = order
        .iter()
        .map(|&i| {
            let bone = field(&raw[i], "bone") as usize;
            ModifierSlot {
                host: bone,
                bone: (bone != 0xFFFF).then_some(bone),
                key: field(&raw[i], "key") as u16,
                enabled: raw[i]["enabled"].as_bool().expect("enabled"),
            }
        })
        .collect();
    let label = |s: usize| (class(order[s]), slots[s].bone.unwrap_or(usize::MAX));
    let observed: Vec<(String, usize)> = doc["observed"]
        .as_array()
        .expect("observed")
        .iter()
        .map(|o| {
            (
                o["cls"].as_str().expect("cls").to_owned(),
                field(o, "bone") as usize,
            )
        })
        .collect();
    let lists = build_lod_lists(&hierarchy, &slots).expect("build lists");
    println!("{} slots, {} observed applies", slots.len(), observed.len());
    for (level, list) in lists.iter().enumerate().skip(1) {
        let predicted: Vec<(String, usize)> = list.iter().map(|&s| label(s)).collect();
        let unobserved: Vec<&(String, usize)> =
            predicted.iter().filter(|p| !observed.contains(p)).collect();
        let missing = observed.iter().filter(|o| !predicted.contains(o)).count();
        let kept: Vec<&(String, usize)> =
            predicted.iter().filter(|p| observed.contains(p)).collect();
        let (mut exact, mut ties, mut mismatches) = (0usize, 0usize, Vec::new());
        for (i, (p, o)) in kept.iter().zip(&observed).enumerate() {
            if *p == o {
                exact += 1;
            } else if p.1 == o.1 {
                ties += 1;
            } else {
                mismatches.push((i, (*p).clone(), o.clone()));
            }
        }
        println!(
            "level {level}: listed {} | observed missing from list {missing} | listed but not stored {} | \
             positions exact {exact}, same-bone ties {ties}, mismatches {}",
            list.len(),
            unobserved.len(),
            mismatches.len()
        );
        for m in mismatches.iter().take(5) {
            println!(
                "    mismatch at {}: predicted {:?} observed {:?}",
                m.0, m.1, m.2
            );
        }
        if level == 5 {
            for (cls, bone) in &unobserved {
                let s = (0..slots.len())
                    .find(|&s| label(s) == (cls.clone(), *bone))
                    .expect("slot");
                println!(
                    "    not stored: {cls} bone {bone} lod {} key {:#06x} weight {}",
                    hierarchy[*bone].lod, slots[s].key, raw[order[s]]["weight"]
                );
            }
        }
    }
}
