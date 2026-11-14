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
slice. Last full review: 2026-11-14.

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
| Archives, frames, controls, prefetch | 0001, 0002, 0004, 0006, 0038 | ✅ | ✅ | ✅ native-library frame oracle | patch precedence (#35) | `acu_assets` |
| Textures | 0003, 0005, 0007 | ✅ | 🟡 inline only | 🟡 | external/streamed variants | `acu_assets` |
| Meshes, vertices, clusters | 0008, 0011, 0012 | ✅ | ✅ | 🟡 stored boxes | cluster bit-mask semantics | `acu_assets` |
| Materials, shading | 0013, `LIGHTING_RESEARCH.md` | 🟡 | 🟡 | ❌ | material templates/shaders | `acu_assets`, `acu_bevy` |
| Skeleton, rig, skinning, bone modifiers | 0010, 0014–0017, 0028 | 🟡 | ✅ | ✅ modifiers gated live (0028) | runtime solver order beyond the gated set | `acu_assets`, `acu_motion` |
| Animation clips and tracks | 0018, 0026, 0027 | 🟡 | 🟡 codec 7 open | 🟡 | codec 7, events | `acu_assets`, `acu_motion` |
| Locomotion selection, root motion | 0019, 0022, 0025, 0029, 0032 | 🟡 | 🟡 | 🟡 live root/pose captures (not exported) | native graph selection; root-motion ownership | `acu_motion`, `acu_sim` |
| Input | 0021, 0023, 0044 | 🟡 stick is camera-relative | — | 🟡 | action bindings; camera yaw source | `acu_sim` |
| Player entity and transform | 0044 | 🟡 Z up (one sample) | — | ✅ live chain | units (metres per 0047, unconfirmed in 0044); authoritative transform owner | `acu_sim` (axis conversion) |
| Parkour candidates, ranking, actions | 0030, 0041–0043, 0045, 0046 | 🟡 | ❌ scanner input geometry | 🟡 traces (not exported) | where candidates come from; action semantics | `acu_sim`, `acu_motion` |
| World cells and placements | 0035, 0036, 0047 | ✅ Paris + Versailles | ✅ | ✅ 1 mm vs live | EntityGroup nesting (prefabs); streaming policy | `acu_assets` |
| **Collision, physics, navmesh** | 0034 (type names only), 0047 | ❌ | ❌ | ❌ | **all of it** | `acu_assets`, `acu_sim` |
| World lifecycle and travel | 0049 | ✅ lifecycle observed | ✅ world/portal IDs | ✅ `data/oracles/world_travel.json` | who sets travel state; what the player is spawned from | `acu_sim` |
| Outfit / gear | 0048 | 🟡 | 🟡 | ❌ | appearance refresh; persistence across worlds | `acu_sim`, `acu_bevy` |
| **Camera** | none | ❌ | ❌ | ❌ | everything (needed for camera-relative input) | `acu_sim`, `acu_bevy` |
| **Simulation tick and time** | none | ❌ | — | ❌ | native update rate, fixed or variable step | `acu_sim` |
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

## Preparation that is not research

- **Handoffs on every finding.** The newest ones (0042–0049) now have them. Older findings that
  feed a near-term slice should get one when that slice is planned.
- **Oracle fixtures.** One schema per kind (`acu-oracle/<kind>/<version>`), written by
  `tools/oracle_export.py` from runs under `local/native-probe/runs/`, committed under
  `data/oracles/`. Derived numbers, file IDs as 16-digit hex, no addresses, no game bytes.
- **Function dossiers.** For each critical-path routine a builder will transcribe: VA, corpus
  file, callers seen live, struct offsets read and written, constants. They belong in the finding
  that owns the routine.
- **Decisions to record before the first slice uses them:** coordinate and unit convention (Z up
  observed in 0044 and 0047; metres used by 0047, unconfirmed in 0044; handedness unproven),
  tick policy (after 2), root-motion ownership (after 4). Identity policy is recorded:
  [ADR-0002](decisions/0002-identity-policy.md).

## Live-tooling rules that keep this safe

Use the pinned save, `teleport.py keep-safe` for Invisible + God Mode, the travel guard (shim t8),
and close the game only with `capture_run.py --close` (see `tools/native_probe/README.md`).
