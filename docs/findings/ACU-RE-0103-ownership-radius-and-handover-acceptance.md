# ACU-RE-0103 — Where does the ownership radius come from, and who accepts a hand-over?

Date / investigator / related task: 2026-11-16 / Claude (Opus 5.5), static (capstone and IDA) / #85, cross-cutting
ownership items in `docs/coop/verdicts.json` (`ownership.radius_table`, `ownership.storm_acceptance`,
`ownership.state_on_handover`)
State: partially established.
- Established:
  - R, the ownership radius, is not a per-class table; it is a load-distance grid over the world, looked up at the
    local focus position;
  - R's cap and its two overrides;
  - the game's side of a hand-over: the pending flag, the posted event, the completion callbacks, and a fallback
    "give to any peer" virtual;
  - Storm owns the accept or refuse decision.
- Not established:
  - the grid's values (heap data, live check);
  - what triggers the fallback virtual;
  - where the pending flag is cleared;
  - whether any state is sent at hand-over.

Supersedes ACU-RE-0097 observation 5 on one point: R is not per entity or per class.

## Provenance

Binary path / SHA-256 / image base / VA / RVA:
- Captured mapped main image, SHA-256 `9a1124f1c4c368939ad3d08315b3dc968e6e48cc22f7b096f34a94a5e7380520`, image base
  `0x140000000`.
- The private IDB `local/85-coop-ida/ACU.runtime-analysis.pe.i64`.

Corpus path and lines / export identity status: not used.
Archive identity / byte range / range hash / file ID (hex string): not applicable.
Stored and decoded lengths / SHA-256: not applicable.

Tool/provider/version / evidence IDs or saved output:
- IDA Pro (idalib, Hex-Rays) through ida-pro-mcp: decompile and xrefs.
- capstone 5.0.3 disassembly of the capture. The grid arguments are passed in `xmm1`/`xmm2`, which Hex-Rays omits.
- Raw-byte searches of `0x141B00000`–`0x141D00000` for every access to byte `+0xC1` of the Storm object.

Backend boundary: Storm's own handling of the hand-over events was located, but its internals were not decoded
(`docs/coop/BACKEND_BOUNDARY.md`).

## Result

### Observations

1. **R comes from a world grid** (`sub_141DF4190(loader)`, capstone).
   - The ownership rule (`sub_140F81210`, ACU-RE-0097) passes `world+0x7B8` (1976), which is the world loader. It
     does not pass the entity.
   - Each frame, the loader caches a focus position at `+0xD0`: the position of the entity at `+0x98` (or `+0x90`)
     through `sub_141CD1FF0`, or a stored position at `+0xC0`.
   - It then calls `sub_1420F1EA0(grid, x, y, level 0)`:
     - the grid descriptor is `[loader+0x120]+0x18`;
     - `xmm1` is x and `xmm2` is y, both from the focus.
   - `sub_1420F1EA0` computes a cell index in a square, multi-level grid:
     - the descriptor holds the cell size, the origin x and y, and the side length;
     - it returns −1 outside the grid.
   - R is the u16 at index × 2 in the array `[loader+0x120]+0x38`. When the index is −1, or descriptor byte `+0x18`
     is set, R is the descriptor's default u16 at `+0x14` instead.
2. **The cap and the overrides.**
   - **The cap:** R is clamped to loader `+0xB0`. `sub_141DF7970` sets it to `ceil(sqrt(2·s²))`, where `s` is the int
     at `[loader+0x88]+0x20`. That is the diagonal of a square of side `s`.
   - **A scripted value** at `+0xAC`:
     - set through `sub_141DFA8E0` by four game functions: `sub_141189BB0`, `sub_14119ACF0`, `sub_1411BD550` and
       `sub_1411BE340`;
     - for example, `sub_1411BD550` sets it from its object's `+200` when that object activates;
     - `sub_141DF84B0` resets it to 0;
     - when it is non-zero and descriptor byte `+0x17` is clear, R = min(`+0xAC`, cap) and the grid is skipped.
   - **A config override** at `+0x154`:
     - set by `sub_141DFB080`, which is called from `sub_141D68080`;
     - the value is `[qword_145219860]+640`, or 0 when a feature bit is set;
     - it applies only when it is below the grid value, so it can lower R but never raise it, and it is then not
       capped.
   - `sub_141D68080` also sets the loader's focus (`sub_141DFA910`) from the entity at world `+328`.
3. **Giving master role, the game's side.**
   - `sub_141C56030` and `sub_141C43390` (below) both:
     - check that the Storm object at net object `+0x1B8` (440) is held here (byte `+0x3C`);
     - check that no hand-over is pending (byte `+0xC1` = 0);
     - set `+0xC1` = 1;
     - post `Give Master Role` through `sub_141B2F9B0` for a peer id.
   - Byte `+0xC1` is read in four game net-object functions: `sub_141C43390`, `sub_141C55F80`, `sub_141C56030` and
     `sub_141C5AB00`. It is written there only to set it to 1.
4. **Storm decides.**
   - Storm's type records include the inbound object events:
     - `GiveMasterRole`, `AcceptGiveMasterRole` and `RefuseGiveMasterRole`;
     - `AcceptReplaceGoneMasterCandidacy` and `RefuseReplaceGoneMasterCandidacy`;
     - `AcquireMasterRole`.
   - The data events are `GiveMasterRoleEvent`, `GiveMasterRoleFailEvent`, `AcquireMasterRoleEvent`,
     `AcquireMasterRoleFailEvent`, `MasterDeletedEvent` and `MasterDeleteReplicaEvent`.
   - Every code reference to the Accept and Refuse records is inside Storm:
     - the record constructors;
     - Storm's inbound-event dispatcher `sub_141B8ABF0`, which routes Accept to `sub_141B52290` and Refuse to
       `sub_141BB0180`;
     - the event-routing setup `sub_141BB0740`.
   - No game function builds either event.
5. **Completion callbacks** (ACU-RE-0092 observation 18, read again).
   - When Storm completes a hand-over, `sub_141C62F30` (becoming master):
     - sets the entity's role bits (`&= ~0x80000; |= 0x40000`);
     - calls the entity component at `+240` (virtual `+208`) with the old and new role;
     - notifies the net object's listener (`+136`, virtual `+184`) with code 3;
     - clears byte `+265` (`+0x109`), the acquisition-request latch from ACU-RE-0100.
   - `sub_141C631D0` is the replica side.
   - `sub_141C5AB00` handles a Storm result. On result 1 it notifies the listener with code 4. If neither `+192` nor
     `+193` is set, it queues the object through `sub_141C41E80`.
6. **A fallback "give to any peer" virtual.**
   - `sub_141C43390` sits in about 17 net-object vtables.
   - It walks the session peer list (`sub_140682520` → `+64`) and gives master role to the **first** peer whose record
     has `+12` set (peer id at `+8`).
   - `sub_141C43330` calls it after:
     - an early exit when the Storm object's state `+56` is 1;
     - an object hook at `+592`, which may choose the target itself.
   - `sub_141EB1740` calls it only when `sub_141C53E20` is true (no Storm object, or its state `+56` is not 1).
7. **No state is sent in the game's give path.**
   - Neither the give functions nor the completion callbacks send a memento or a message.
   - The new master starts from what its replica already holds: mementos (ACU-RE-0084, 0095) and intentions
     (ACU-RE-0080).
   - Inside Storm, `sub_141B9CBB0` runs while `+193` is set and forwards packets of kind 2 (`sub_14267A8D0`). What
     those packets are was not decoded (backend boundary).

### Inferences

- **R is regional, and the same for every entity on a peer.**
  - The owner computes R at its own focus, so a dense area (smaller load distance) migrates objects sooner.
  - The 96–97 m derived in ACU-RE-0097 from the live hysteresis is one region's grid value, capped by the
    load-square diagonal.
  - Missions can pin R (the scripted value), and the config can lower it.
- **A peer cannot veto a hand-over in game code.** Acceptance happens in the receiving peer's Storm, probably
  according to the object's migration mode (ACU-RE-0100: Disable, Active, SessionHostOnly). The game learns the
  result only from the role callbacks or a fail event.
- **The fallback virtual is probably the leave and unload hand-off.** It ignores distance and picks the first valid
  peer, which fits ACU-RE-0092's unload hand-off at 100–111 m. Its trigger was not found.
- **A hand-over moves authority, not state.** Whatever the old master changed after its last memento flush, and
  had not sent, is lost unless Storm's pending-state forwarding carries it.

### Implementation choice

The library models the radius as data the adapter supplies at a position: `ownership_distance(position)`. It is not
a per-class constant. The defaults are a cap and a per-mission override.

Modern improvements over ACU:
- An explicit give, accept or refuse handshake with a timeout. The pending latch clears on timeout, and the refusal
  reason is reported.
- The old master flushes dirty state before it gives up authority, so a hand-over never loses an unsent change.
- The fallback target is the nearest eligible peer, not the first in the list.

### Unknowns and next checks

- **The grid values.**
  - Live: dump `[loader+0x120]+0x38` and its descriptor on the testbed.
  - Log `sub_141DF4190`'s result while walking from the alarm-bell area to Danton's area. Expect 96 or 97 there, if
    the cap allows it.
- **What triggers the fallback.** Find the vtable slot index of `sub_141C43390` and its callers, or hook it during a
  peer leave and an unload.
- **Where `+0xC1` is cleared.** Probably in Storm, so observe it live: read it before and after a refused hand-over.
- **Storm's pending-state forwarding** (`sub_141B9CBB0`): what kind 2 is.

## Reproduction status

Static. Each observation names its addresses. The capstone reads use the same capture.
