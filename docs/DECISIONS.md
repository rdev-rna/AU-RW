# Decision index

Use `docs/templates/DECISION.md`; store accepted decisions under `docs/decisions/`.

| ID | Decision | Status / rationale |
| --- | --- | --- |
| SETUP-001 | Preserve reference inputs and keep generated data local | Accepted for preparation; reproducibility and small source tree |
| SETUP-002 | Inspect/adapt For Honor runtime reader before building another reader | Accepted direction from user; compatibility remains a research gate |
| SETUP-003 | Separate asset decoding, simulation and Bevy presentation | Initial design; revisit as the first slice establishes interfaces |
| ADR-0001 | [Runtime decoder workspace](decisions/0001-decoder-workspace.md) | Rust 1.99.0 MSVC, reserved Bevy 0.19.1; assets-only initial workspace |
| ADR-0002 | [Identity policy](decisions/0002-identity-policy.md) | Key state, caches and oracles on resource file IDs; native entity addresses/runtime IDs change on every world arrival (ACU-RE-0049) |
| ADR-0003 | [Coordinates and units](decisions/0003-coordinates-and-units.md) | Native X right, Y forward, Z up, metres in data and simulation; Bevy gets (x, z, −y) at the boundary; compass headings atan2(x, y). Handedness inferred |
| ADR-0004 | [Tick policy](decisions/0004-tick-policy.md) | Fixed 1/60 s tick (native-parity at a 60 fps cap), 1/30 s parity mode; native game is variable-step (ACU-RE-0051) |
| ADR-0005 | [Root-motion ownership](decisions/0005-root-motion-ownership.md) | Simulation owns the transform; per-state velocity source (controller or clip root + warp). Native owner unknown; write-watch probe named |

Collision backend is pending. Tick policy: ADR-0004. Patch precedence: front-insert mount order
observed statically, with live reads agreeing (ACU-RE-0053). Version pins and
codec dependencies are implementation choices, not native engine facts.
