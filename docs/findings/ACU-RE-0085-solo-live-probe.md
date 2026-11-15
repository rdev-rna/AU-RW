# ACU-RE-0085: What does the co-op layer do in a solo game? (first live probe)

Date / investigator / related task: 2026-11-15 / Claude (Opus 5.5), live, read-only / #85, co-op plan Phase 7 ([COOP_RE_PLAN.md](../COOP_RE_PLAN.md))
State: partially established.
- Observed in one solo session: hit rates of eight co-op routines, the session object's virtuals, and the
  global sender's `+0x80`.
- Confirms in a solo game the path from replication to the network-component checks, and gives observed
  support for one inferred hop from ACU-RE-0081.
- Not covered: a co-op session, movement, combat, or any input from the player.

## Provenance

Binary path / SHA-256 / image base / VA / RVA: installed `D:/Games/Assassin's Creed Unity/ACU.exe`,
SHA-256 `748abdc6456397fe7d8080f975f2a5ed3123bce548e7bf1c68b23f47e8b27e40`, live image base `0x140000000`
(the preferred base, so VAs equal the static ones). Static follow-up on the mapped image `9a1124f1…0520`.
Loader note (found after the run): the install's active `uplay_r1_loader64.dll` is not Ubisoft's. It is
unsigned, 156,672 bytes, SHA-256 prefix `532faf5284334a65`, dated 2014-11-18. The Ubisoft-signed loader
(575,192 bytes, prefix `0e06fee61bc4090c`) sits beside it as `.dll.bak` (2026-02-09) and `.3dm`. The
project has no record of this swap. The routines measured here are inside `ACU.exe` (hash-checked), not
the loader, but the session and startup state may differ from a stock install. Treat these rates as
observed with that loader until they are repeated with the signed one.
Corpus path and lines / export identity status: not used.
Archive identity / byte range / range hash / file ID (hex string): not applicable.
Stored and decoded lengths / SHA-256: not applicable.
Tool/provider/version / evidence IDs or saved output:
- Native probe (`tools/native_probe`, hardware execute hooks with byte checks) and the focus shim,
  through `capture_run.py --until world --save-pin capture-start-1 --fps-cap 60` on branch
  `feat/parkour-candidate-trace`.
- Run `local/native-probe/runs/coop-solo-20261115b`, PID 16008: in the world after 36 s of loading,
  Arno standing still at (496.66, −7.31, −0.25).
- New tool `tools/coop_live_probe.py` (SHA-256 prefix `e8077a82199500e0`). Output
  `local/coop/live_probe_solo_20261115.json` (prefix `8e029ba2c3cde6f3`), kept in ignored local data.
- Lock #30 was held for the whole run. The game was closed with `capture_run.py --close 16008`
  (graceful, 2.5 s), and the saves were put back from snapshot `20261115-181821`.

Exact commands and search boundary: two 20-second rounds of four execute hooks, sampling every hit. The
rate guard limit was 2000 per second and never fired. Hit counts come from the probe's per-slot
counters. The event ring dropped samples (1034 in round 2), so per-hit samples are partial but the counts
are complete. Static disassembly of the vtable slots read live.

## Result

### Observations

1. **Hit counts in solo, idle in the world, 60 fps:**

   | Routine | Hits / 20 s | Rate | Caller (return address) |
   |---|---|---|---|
   | `sub_1405D17F0` StartIntention (generic) | 0 | 0 | none |
   | `sub_141C55BB0` memento push, entry | 1236 | 60.4/s | `0x141C63180` in the network tick `sub_141C630C0` |
   | `0x141C55BEC` memento push, after its session checks | 0 | 0 | none |
   | `sub_141C66360` object replicate path | 614 | 30.0/s | five typed start functions, e.g. `0x14020BBD7` (which calls `sub_141C66660` and then `sub_141C66360`), and `0x1405D760E` |
   | `sub_141C46880` batch close | 435 | 21.7/s | the same family, e.g. `0x140211D20` and `0x1405D762A` |
   | `sub_141C4CB50` record flush | 373 | 18.6/s | `0x141C77768` in the tick's list walk `sub_141C77700` |
   | `sub_141C77FE0` memento write (virtual `+0x170`) | 0 | 0 | none |
   | `0x14071F090` intention message builder | 0 | 0 | none |

2. **Intentions start at about 4 per second.** The instance counter `0x145212F3C` went from 262 to 514
   over 62.7 s.
3. **Manager byte `+0x6C` is 1 in solo.** Every batch-close sample (8 of 8, two different managers)
   shows byte `+0x6C` = `01` (snapshot of `[rcx+0x60]`, 32 bytes).
4. **The network tick runs once per frame.** At a 60 fps cap the memento push entry runs 60.4 times
   per second, always from the network tick.
5. **Session object virtuals.** The sub-object at `[qword_145212F48] + 0x20` (call it S) has its vtable
   at RVA `0x300EA50`:

   | Virtual | Target | Returns |
   |---|---|---|
   | `+0x00` (`sub_141F89160`, "session active") | `sub_141C54350` | `S+0x30 ≠ −1` |
   | `+0x10` (`sub_141F891A0`) | `sub_141C543F0` | byte `S+0x9D` |
   | `+0x30` (key of the memento-push list) | `sub_1408B1FC0` | dword `S+0x30` |
   | `+0xD8` (the local index of ACU-RE-0083) | `sub_141C4F550` | `[S+0x90]+0x10`, if `S+0x90` is set |
   | `+0x118` (the network time behind `sub_141F89950`) | `sub_141C50CA0` | dword `S+0x98` |

6. **The global sender's `+0x80` invokes a creator delegate.** The vtable of `[qword_145139D00] + 0x110`
   is at RVA `0x300F580`, and slot `+0x80` is `sub_141C47130(this, delegate, intention, unicast)`:
   - it returns 0 if the delegate is empty;
   - otherwise it creates a message through its own virtual `+0xC0`;
   - then it calls the delegate's invoker (`delegate+0x18`) with `(bound object = delegate+0x10 & ~3,
     message, intention, unicast)`.
7. **Other singletons** (vtable RVAs, saved in the output):
   - `qword_1451B1E90`: `0x30100E0`
   - `qword_145212EB8` registry: `0x300E640`
   - `qword_1452217B8` peer list: `0x306D330`
   - `qword_1451C5F78` cookie context: `0x300EA00`
   - `qword_145219860`: `0x3023CC0`
   - `qword_145213880` and `qword_1452136C0`: their first qword is not an image pointer.
   - `qword_145221CF8` and `qword_145221E08`: point into the image.
   - Clock offset `qword_14521E148`: 0.

### Corrections and confirmations

- **ACU-RE-0080/0081:** most intention starts in this session go through the **typed** start functions,
  not the generic `sub_1405D17F0` (0 hits). The typed functions call the replicate paths directly.
- **ACU-RE-0081:** the replicate branch runs in solo (`sub_141C66360` at 30 per second), and manager
  `+0x6C` is set. The builder never runs, so the object path must stop at its network-component checks
  (`+0x10`, `sub_140682520`, `sub_142100760`) before virtual `+0x80`. Solo is therefore "the same path with
  no network component", not "replication switched off" (spec R3).
- **ACU-RE-0081, the inferred `+0x80` → builder hop:** for the global sender this is now observed in code.
  Virtual `+0x80` builds a message and calls a delegate with `(component, message, intention, unicast)`,
  which matches the builder's registers (`rcx` this, `r8` intention, `r9b` unicast). Only the global
  sender's vtable was read. That the per-object dispatcher shares the implementation is inferred.
- **ACU-RE-0084:** the memento push stops at its session checks in solo. "Session active" is
  `S+0x30 ≠ −1`, and the same `S+0x30` keys the per-peer object list, so `S+0x30` is the local session
  peer id (−1 outside a session).
- **ACU-RE-0078/0083:** the network time is `S+0x98`. The "local index" (`+0xD8`) is a different field,
  `[S+0x90]+0x10`.

### Inferences and alternatives

- **The batches and flushes in solo carry no records.** No message is built, so `sub_141C4CB50` swaps an
  empty array. The flush still runs because batch close marks the manager whenever `+0x6C` is set. This
  was not checked by sampling the record arrays.
- **`S+0x30` is the session peer id and `[S+0x90]+0x10` the local player's slot.** Their values were not
  read in this run.

### Unknowns / incomplete coverage

- Values of `S+0x30`, `S+0x90`, `S+0x98` and `S+0x9D` in solo (only the vtables were read).
- Which network-component check stops the object path in solo.
- Behaviour with movement, combat or crowd events, and in a real co-op session. A two-instance capture
  needs the multi-instance tooling (PR #141), and whether Ubisoft Connect starts a second ACU is unknown.
- The per-object dispatcher's vtable (only reachable once a network component exists).

### Independent comparison / falsification test

- Each static prediction could have failed here:
  - the memento push should stop at its session checks in solo (0 hits after them: holds);
  - the builder should not run without a network component (0 hits: holds);
  - the flush should be reached only from the network tick (one caller, `0x141C77768`: holds).
- All hooks passed the probe's byte check, so the live code at each address equals the static image.

### Reproduction steps

1. `python tools/native_probe/capture_run.py local/native-probe/runs/<name> --until world --save-pin
   capture-start-1 --fps-cap 60` (from the checkout that has the capture tools), holding lock #30.
2. `python tools/coop_live_probe.py <pid> local/coop/<out>.json --seconds 20`.
3. `python tools/native_probe/capture_run.py --close <pid>`, then
   `python tools/save_pin.py putback <snapshot>`.
4. Disassemble `0x141C54350`, `0x141C543F0`, `0x1408B1FC0`, `0x141C4F550`, `0x141C50CA0` and
   `0x141C47130`.

### Implementation implications and finding-linked constants

- **Single player is the offline case of the co-op code.** A port can call its replication hooks
  unconditionally and let the absence of a session or network component turn them into no-ops, as ACU
  does.
- **The network tick runs every frame,** so a port's network update can share the frame loop. Rates in a
  real session are still unmeasured.
- **The session state a port must provide:** a local session peer id (−1 offline), a local player slot, a
  session time in ms, and one "suspended" flag (`S+0x9D`) that stops the memento push.
