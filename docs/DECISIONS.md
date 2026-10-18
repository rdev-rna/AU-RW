# Decision index

Use `docs/templates/DECISION.md`; store accepted decisions under `docs/decisions/`.

| ID | Decision | Status / rationale |
| --- | --- | --- |
| SETUP-001 | Preserve reference inputs and keep generated data local | Accepted for preparation; reproducibility and small source tree |
| SETUP-002 | Inspect/adapt For Honor runtime reader before building another reader | Accepted direction from user; compatibility remains a research gate |
| SETUP-003 | Separate asset decoding, simulation and Bevy presentation | Initial design; revisit as the first slice establishes interfaces |
| ADR-0001 | [Runtime decoder workspace](decisions/0001-decoder-workspace.md) | Rust 1.99.0 MSVC, reserved Bevy 0.19.1; assets-only initial workspace |

Collision backend, tick rate and patch precedence are pending. Version pins and
codec dependencies are implementation choices, not native engine facts.
