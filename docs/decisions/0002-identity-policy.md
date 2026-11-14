# ADR-0002 — Identity policy: key on resource file IDs, never on entity addresses

Date: 2026-11-14
Status: accepted
Related task / findings: ACU-RE-0047 (placements), ACU-RE-0049 (world travel, obs. 15–16),
`docs/REWRITE_READINESS.md`.

Context and constraints:
- Native observation (ACU-RE-0049 obs. 15): every world arrival creates a new World object and a
  new player entity, and the player's runtime ID (`[[E+0x130]+0x10]`, e.g. `F000494A`,
  `F0019960`, `F00270DC`) is new each time and absent from the asset index. Within one process the
  world resource handles, the World holder and the travel manager stay the same.
- Native observation (ACU-RE-0047): placements carry resource file IDs that match live entities
  to under 1 mm.
- AGENTS.md: preserve stable asset identities through rendering; uint64 file IDs as 16-digit hex
  strings in JSON.

Options considered:
1. Key runtime state, caches, oracles and replays on resource file IDs (world, portal, entity
   data, mesh), with runtime instances as disposable handles.
2. Mirror native runtime IDs or addresses.

Chosen approach and reason: option 1. Native runtime IDs and addresses do not survive a world
change, so nothing that must persist or be compared can use them. File IDs are what the data and
the native game agree on.

Consequences / limitations:
- `acu_sim` entities get their own runtime handles; persistent data (player gear, progression)
  lives outside any World and is keyed by resource IDs (where it lives natively is not yet
  established; see ACU-RE-0048 / 0049 follow-ups).
- Oracle fixtures store file IDs and relations (same/new), never pointers.
- Duplicate IDs across archives still need patch precedence (#35) to pick one resource.

Validation: `data/oracles/world_travel.json` relations (`new_world_object`, `new_player_entity`)
and `world_handle_stable_within_process`.

Revisit trigger: evidence that some native system persists entity identity across worlds (for
example a save system keyed on runtime IDs).

Native observations are labelled above; the choice of keys for the Rust side is an
implementation decision.
