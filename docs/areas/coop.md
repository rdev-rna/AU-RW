# Area: coop

Epic: #26 (label `area:coop`). Owner: whoever holds the open claim on this area's slices (docs/PROJECT_BOARD.md).
Edit this file only in the PR that lands this area's work.

- **Status:** started (plan, census and Phase 1 registry [ACU-RE-0072](../findings/ACU-RE-0072-net-protocol-registry.md), 2026-11-15; see [../COOP_RE_PLAN.md](../COOP_RE_PLAN.md))
- **Scope:** co-op, heists, club, matchmaking (deferred, M6+)
- **Anchors and reuse (leads, not proof):** matchmaking ~0x1404DCxxx; `tools/net_census.py` layers (zenitar NetProtocol / Storm-Echo / Ubisoft backend); classes NetSessionController, NetGameManager, NetComponent, HumanReplica, NetFight2*, DominoJIPSnapshot. Ubisoft backend calls are recorded, never decoded.

Evidence rules: string hits are not proof of mechanics (AGENTS.md).

Milestone: none. ROADMAP.md defers online/co-op for the Bevy rewrite; the co-op RE track (user request 2026-11-15) is separate and adds no rewrite requirement.
