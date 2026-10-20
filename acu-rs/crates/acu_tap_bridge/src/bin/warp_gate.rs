//! Compare the Rust correction kernel with isolated original ACU instructions.
use acu_motion::target_warp::{TranslationInput, TranslationMode, translation_correction};
use serde_json::Value;

fn vector(value: &Value) -> [f32; 3] {
    std::array::from_fn(|i| value[i].as_f64().expect("finite vector") as f32)
}
fn main() {
    let path = std::env::args()
        .nth(1)
        .expect("warp_gate <warp-oracle.json>");
    let bytes = std::fs::read(path).expect("oracle fixture");
    let source: Value =
        serde_json::from_slice(bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(&bytes))
            .expect("JSON");
    assert_eq!(source["kind"], "isolated-native-arithmetic");
    let cases = source["cases"].as_array().expect("cases");
    assert!(!cases.is_empty());
    let mut worst = 0.0_f32;
    let mut changed = 0;
    let mut mismatches = 0;
    for (index, case) in cases.iter().enumerate() {
        let got = translation_correction(TranslationInput {
            step: vector(&case["step"]),
            remaining: vector(&case["remaining"]),
            error: vector(&case["error"]),
            dt: case["dt"].as_f64().unwrap() as f32,
            time_remaining: case["time"].as_f64().unwrap() as f32,
            mode: if case["mode"] == 0 {
                TranslationMode::RemainingMotion
            } else {
                TranslationMode::CurrentStep
            },
        })
        .expect("supported input");
        let expected = vector(&case["delta"]);
        let error = (0..3)
            .map(|i| (got.delta[i] - expected[i]).abs())
            .fold(0.0_f32, f32::max);
        worst = worst.max(error);
        if expected.into_iter().any(|v| v.abs() > 1e-6) {
            changed += 1;
        }
        if error > 1e-6 || got.horizontal_complete != case["complete"].as_bool().unwrap() {
            if mismatches < 8 {
                eprintln!(
                    "case {index}: rust={:?} native={expected:?} error={error}",
                    got.delta
                );
            }
            mismatches += 1;
        }
    }
    println!(
        "{} isolated cases; worst={worst:.9e}; mismatches={mismatches}; unwarped negative-control failures={changed}",
        cases.len()
    );
    assert!(changed > 0, "negative control must fail");
    assert_eq!(mismatches, 0);
}
