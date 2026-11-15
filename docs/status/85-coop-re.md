# #85 Co-op reverse engineering (area: coop)

Branch `acu-85-coop-re-plan`. Plan: [../COOP_RE_PLAN.md](../COOP_RE_PLAN.md). Files: `docs/COOP_RE_PLAN.md`,
`docs/areas/coop.md`, `tools/net_census.py`, `tools/net_protocol_atlas.py`,
`tools/test_net_protocol_atlas.py`, `docs/findings/ACU-RE-0072-net-protocol-registry.md`.

## Done (2026-11-15)
- **Phase 0:** IDB identity bound. The analysis view `47a81dc9…` maps to the mapped image `9a1124f1…`. I used
  a private IDB copy in `local/85-coop-ida/`, read-only, under lock #31.
- **Phase 1 (registry format):** [ACU-RE-0072](../findings/ACU-RE-0072-net-protocol-registry.md).
  - Message IDs are CRC-32 of name strings. A unicast ID appends `_UNICAST` before hashing.
  - The image has 1,508 self-checked net type records, all rooted in `storm::*` or `NetObject`. Of these,
    958 are RFC messages and 118 are mementos.
  - 62 static registrars map one-to-one to the `zenitar_` messages.
  - 1,639 object-scoped ID literals are each traced to the functions that use them.
  - There are 217 direct send sites. When a call site has no net handle, the call runs locally.
  - Acceptance owners `NetFightJIPManager`, `MissionMatchmakingNetHelper` and `NetHumanIntention` resolve by
    two independent routes.
- `tools/net_protocol_atlas.py` writes `local/coop/net_protocol_atlas.json` deterministically.
  `tools/test_net_protocol_atlas.py` has 11 synthetic tests (CRC values, record self-check, list cycle,
  missing `.pdata`, function bounds, bad PE). All pass.

## Choice
The full atlas, about 810 KB of identifiers, stays in ignored `local/` and is regenerated from the capture.
The repo is public, so only the tool and the finding are committed. This overrides the plan's earlier
`data/coop/` location.

## Not done / next
- Phase 1 remainder: confirm the decoder, encoder and body roles by tracing the receive path from Storm to
  the decoder. Recover the vf0 prefix strings so object-scoped IDs can be computed offline.
- Phase 2: session seam and a static test of the LAN path.
- No backend calls were met in Phase 1, so the boundary register is still empty.
