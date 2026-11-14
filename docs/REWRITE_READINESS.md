# Rewrite readiness

Goal: make porting plain. Before a system is rewritten in Rust it should have four things, so the
port is transcription plus a diff, never a guess:

1. **Spec** — a finding that says what the original does, with observation / inference / unknown
   separated and a Development handoff (`RESEARCH_TO_BUILD.md`).
2. **Data** — decoders for every input it reads (`acu_assets`), or a statement that it reads none.
3. **Oracle** — recorded native behaviour the port can be diffed against without the game
   running: a portable fixture under `data/oracles/` (derived numbers only, with provenance), plus
   the live tool that regenerates it.
4. **No open unknown on its critical path** — anything still open is either researched first or
   recorded in `DECISIONS.md` as an explicit, labelled implementation choice.

This page is the map: per system, how far each of the four is. Update it with every finding or
slice. Last full review: 2026-11-14 (after the research backlog stack).

## What we have, and how much it helps

| Source | Helps with | Limits |
| --- | --- | --- |
| Live game + DLL hooks (focus shim, native probe, teleport, travel, keyboard/pad) | Oracles: the original running, any state readable per frame, scripted input, repeatable starts (pin, teleport), world travel | Captures sit in ignored `local/`, tied to this machine and binary; they must be **exported to fixtures** to help a port (see `tools/oracle_export.py`) |
| Pseudocode corpus (217k functions) | Specs: exact control flow, constants, struct offsets for any routine we can locate | Unannotated; useful only once a routine is found (live hooks find them). Code bytes could not be compared to the packed install (ACU-RE-0031) |
| Asset index + Forge decoders | Data: every resource is listed and many families decode with provenance | Patch precedence unknown (#35); world collision/physics/navmesh not decoded |
| Existing findings (0001–0049) | Assets: strong. Behaviour: partial, mostly "what is selected", less "why" | Most findings before 0042 lack a Development handoff |

So: yes, the captures, corpus and RE all help, but differently. Assets are close to port-ready.
Behaviour is where a port would still have to guess, and that is where more RE should go: targeted
at the critical path below, not broad.

## System map

Legend: ✅ ready · 🟡 partial · ❌ missing · — not needed.

| System | Findings | Spec | Data | Oracle | Critical unknowns | Home |
| --- | --- | --- | --- | --- | --- | --- |
| Archives, frames, controls, prefetch | 0001, 0002, 0004, 0006, 0038, 0053 | ✅ | ✅ | ✅ native-library frame oracle | patch precedence: front-insert mount order (0053, 672 live reads agree); mount order not observed live | `acu_assets` |
| Textures | 0003, 0005, 0007 | ✅ | 🟡 inline only | 🟡 | external/streamed variants | `acu_assets` |
| Meshes, vertices, clusters | 0008, 0011, 0012 | ✅ | ✅ | 🟡 stored boxes | cluster bit-mask semantics | `acu_assets` |
| Materials, shading | 0013, `LIGHTING_RESEARCH.md` | 🟡 | 🟡 | ❌ | material templates/shaders | `acu_assets`, `acu_bevy` |
| Skeleton, rig, skinning, bone modifiers | 0010, 0014–0017, 0028 | 🟡 | ✅ | ✅ modifiers gated live (0028) | runtime solver order beyond the gated set | `acu_assets`, `acu_motion` |
| Animation clips and tracks | 0018, 0026, 0027 | 🟡 | 🟡 codec 7 open | 🟡 | codec 7, events | `acu_assets`, `acu_motion` |
| Locomotion selection, root motion | 0019, 0022, 0025, 0029, 0032, 0056 | 🟡 transform = physics body pose (0056) | 🟡 | 🟡 `data/oracles/locomotion.json` | native graph selection; what drives the physics body (velocity vs clip root) | `acu_motion`, `acu_sim` |
| Input | 0021, 0023, 0044, 0052 | ✅ heading = camera yaw + stick angle (0052) | — | ✅ `data/oracles/camera.json` move phases | action bindings; stick response curve | `acu_sim` |
| Player entity and transform | 0044 | 🟡 Z up (one sample) | — | ✅ live chain | units (metres per 0047, unconfirmed in 0044); authoritative transform owner | `acu_sim` (axis conversion) |
| Parkour candidates, ranking, actions | 0030, 0041–0043, 0045, 0046, 0055 | 🟡 candidates = collision ledge lips (0055) | ✅ collision (0050) | ✅ `parkour.json`, `parkour_geometry.json` | edge generator routine; per-type inclusion rules; action semantics | `acu_sim`, `acu_motion` |
| World cells and placements | 0035, 0036, 0047 | ✅ Paris + Versailles | ✅ | ✅ 1 mm vs live | EntityGroup nesting (prefabs); streaming policy | `acu_assets` |
| Collision, physics, navmesh | 0034, 0047, 0050 | 🟡 MeshShape triangles + ground rule | 🟡 meshes ✅; primitives, navmesh ❌ | ✅ `ground_versailles.json`, `ground_paris.json` (3.5 mm median) | primitive shapes; DataLayer switching; navmesh; runtime ground routine | `acu_assets`, `acu_sim` |
| World lifecycle and travel | 0049, 0054 | ✅ request chain + arrival sequence live | ✅ world/portal IDs | ✅ `data/oracles/world_travel.json` | the routine that creates the player entity (port: spawn at the destination portal) | `acu_sim` |
| Outfit / gear | 0048 | 🟡 | 🟡 | ❌ | appearance refresh; persistence across worlds | `acu_sim`, `acu_bevy` |
| Camera | 0052 | ✅ live view = player's ACUPlayerCameraComponent | — | ✅ `data/oracles/camera.json` | collision/distance policy; easing constants; frame-rate dependence | `acu_sim`, `acu_bevy` |
| Simulation tick and time | 0051 | ✅ variable step, physics rate buckets | — | ✅ `data/oracles/tick.json` (30/60/100 fps) | above 100 fps; record the port's fixed-tick choice in DECISIONS.md | `acu_sim` |
| Combat, AI, stealth, missions, UI, audio, saves | none | ❌ | ❌ | ❌ | M5 scope; not on the near path | later |

## Research backlog for the critical path

Ordered by the roadmap (M2 controllable slice → M3 traversal → M4 Paris area). Each item names its
oracle, so it ends in a fixture, not only a write-up.

1. **Ground and collision representation.** Decode what one Paris cell uses for collision (the
   `CollisionMaterial` / shadow-body / physics resources of ACU-RE-0034/0035/0047). Oracle: ground
   heights the game itself reports, by sampling the player's settled Z after teleports and the NPC
   ground points the hop scan already collects (`hop_scan.py` `map.jsonl`). Blocks M2 ground,
   M3 traversal, M4.
2. **Native tick and time step.** Hook a per-frame simulation update and measure call rate and
   `dt` at different render rates (the shim's frame cap gives 30/60/uncapped). Oracle: a
   `data/oracles/tick.json`. Blocks the `acu_sim` tick policy (today a synthetic 1/60).
3. **Camera.** Find the camera object from the player chain and record yaw/pitch/distance against
   stick input. Needed because the stick is camera-relative (ACU-RE-0044). Oracle: input → camera
   and player heading traces.
4. **Locomotion selection and root-motion ownership.** Close the open part of 0026/0027: which
   clip the native graph picks for an input, and whether root motion or simulation moves the
   entity. Oracle: export existing scripted locomotion captures (inputs, position, chosen clip
   per frame) into `data/oracles/locomotion_*.json`.
5. **Parkour scanner inputs.** Join ACU-RE-0042 candidate positions to decoded collision (1) and
   placements (0047): what geometry produces a candidate. Oracle: candidate lists per decision,
   exported from the existing traces.
6. **World-travel remainder.** The setter of travel state +644 (listeners at `qword_14521A830`,
   ACU-RE-0049 obs. 15) and the player spawn source. Lower priority: the lifecycle oracle exists.
7. **Patch precedence (#35).** Which duplicate resource the game loads, observed live (hook the
   resource load for one known duplicate). Affects every decoder's choice.

### Backlog status (2026-11-14)

| # | Item | Result | Oracle | Still open |
| --- | --- | --- | --- | --- |
| 1 | Ground and collision | MeshShape triangles from ShadowRigidBody / ReferenceListShape / RigidBody decoded; ground = highest surface at or below z + 0.5 m (ACU-RE-0050) | `ground_versailles.json`, `ground_paris.json` | primitive shapes, navmesh, DataLayer |
| 2 | Tick | one frame = one sim step, variable dt; physics one step at a rate bucket (ACU-RE-0051) | `tick.json` | above 100 fps (explicit cap only) |
| 3 | Camera | live view found; heading = camera yaw + stick; rates, pitch range, follow (ACU-RE-0052) | `camera.json` | collision/distance policy, curve |
| 4 | Locomotion | oracle exported (earlier); camera-relative heading now pinned by `camera.json` | `locomotion.json` | native clip selection, root-motion ownership |
| 5 | Parkour scanner inputs | candidates are ledge lips of the decoded mesh (92 % within 5 cm) (ACU-RE-0055) | `parkour_geometry.json` | generator routine (live probe got 0 hits in Paris), inclusion rules |
| 6 | World travel remainder | setter chain and arrival sequence observed live (ACU-RE-0054) | `world_travel.json` | player-entity creator (write watch on World +0xD8) |
| 7 | Patch precedence | front-insert mount, first-match lookup; live reads agree (ACU-RE-0053) | — | mount order observed live |

Decisions recorded before the first port slice: [ADR-0003](decisions/0003-coordinates-and-units.md)
(coordinates and units), [ADR-0004](decisions/0004-tick-policy.md) (tick),
[ADR-0005](decisions/0005-root-motion-ownership.md) (root-motion ownership, implementation choice).

## Preparation that is not research

- **Handoffs on every finding.** The newest ones (0042–0049) now have them. Older findings that
  feed a near-term slice should get one when that slice is planned.
- **Oracle fixtures.** One schema per kind (`acu-oracle/<kind>/<version>`), written by
  `tools/oracle_export.py` from runs under `local/native-probe/runs/`, committed under
  `data/oracles/`. Derived numbers, file IDs as 16-digit hex, no addresses, no game bytes.
- **Function dossiers.** For each critical-path routine a builder will transcribe: VA, corpus
  file, callers seen live, struct offsets read and written, constants. They belong in the finding
  that owns the routine.
- **Decisions before the first slice:** identity [ADR-0002](decisions/0002-identity-policy.md),
  coordinates and units [ADR-0003](decisions/0003-coordinates-and-units.md), tick
  [ADR-0004](decisions/0004-tick-policy.md), root-motion ownership
  [ADR-0005](decisions/0005-root-motion-ownership.md). Pending: collision backend.

## Live-tooling rules that keep this safe

Use the pinned save, `teleport.py keep-safe` for Invisible + God Mode, the travel guard (shim t8),
and close the game only with `capture_run.py --close` (see `tools/native_probe/README.md`).
