# ACU-RE-0094 — What does a parkour leap message carry, and does the replica follow it?

Date / investigator / related task: 2026-11-16 / Claude (Opus 5.5), live, two-game LAN testbed / #85, coverage
ledger phase 3a ([../coop/COVERAGE.md](../coop/COVERAGE.md))
State: established for `ParkourLeap` and `ParkourClimbLeap` on one PC. The meaning of the leap-type number, the
flags and the derived fields is not.

## Provenance

Binary path / SHA-256 / image base / VA / RVA: running `ACU.exe`, SHA-256
`748abdc6456397fe7d8080f975f2a5ed3123bce548e7bf1c68b23f47e8b27e40` (reported by `capture_run.py`). Expected hook bytes
came from the captured mapped image `motion-link-20261020/runtime-view/capture/main-image.mapped.bin`, SHA-256
`9a1124f1c4c368939ad3d08315b3dc968e6e48cc22f7b096f34a94a5e7380520`, image base `0x140000000`. The probe checks
16 bytes at each site before hooking, and all four matched.
Corpus path and lines / export identity status: not used; disassembly with capstone 5.0.3.
Archive identity / byte range / range hash / file ID (hex string): not applicable.
Stored and decoded lengths / SHA-256 (ignored outputs in `local/coop/leap-20261116/`):
- `leap1.jsonl` `6629e1916130e74b77a74cbb956bcf1085c69a7b105eea5be43ac63ca0ded996` (19 leaps, master positions only);
- `leap2.jsonl` `cf3a29a2d9ac25d6f1d990f6865ade65af4ecd4b7b9cfe334218ee9e7f5560ce` (14 leaps; its replica reads were
  stale, see the run notes);
- `leap3.jsonl` `b8370fcb63388f301266ffd7317e64d5a4ba9c71ae824a30bb080496a47270a1` (19 leaps, master and replica).
Tool/provider/version / evidence IDs or saved output: `tools/coop_lan/leapwatch.py`, SHA-256
`6c42eda086505726ca5a94818206630f83a758a3f2110134b44fd4b155457bd7`. The run used the scratchpad version
(`2d69fe93…`, saved beside the outputs), which differs only in its paths. The tool uses the native probe and
`navigate.Navigator` from branch `acu-multi-launch-direct`.
Exact commands and search boundary: the ACU-RE-0092 testbed (`tools/coop_lan/README.md` steps 1–4); game A joined
game B's `PRIVATE_MATCH_HOST` lobby by discovery and both loaded DEFEND Danton (`COOP_RB050`) with two players on the
HUD. Then, on game A (the joiner, master of its own avatar):

```powershell
python tools/coop_lan/leapwatch.py <pidA> leap1.jsonl --seconds 60 --headings 0,60,120,180,240,300 --leg 2.5
python tools/coop_lan/leapwatch.py <pidA> leap3.jsonl --seconds 60 --headings 0,60,120,180,240,300 --leg 2.5 `
  --rpid <pidB> --rent <B's entity for A's avatar>
```

## Method

- **Hook sites.** In the `NetBehaviorAI` send builder `0x14069F200`, every message block starts with
  `test r14b, r14b` (the unicast/broadcast choice), then `mov edx, <wire id>`. RBX holds the source message object;
  the block copies `+0x8`, `+0x14`, `+0x30` … `+0x134` from it (ACU-RE-0093).
  - The four sites hooked: `ParkourLeap` `0x1406A625D`, `ParkourClimbLeap` `0x1406A9371`, `ParkourHangLeap`
    `0x1406A77B7`, `ParkourVoidLeap` `0x1406AB385`.
  - Each hit saved 256 bytes at RBX+0x28.
- **Driving.** The player free-ran (right trigger + A) for 2.5 s at a time, in six world headings in turn.
- **Positions.**
  - The tool read the master's position (`player_state.chain`) every 0.1 s, with `time.perf_counter`. On Windows that
    clock is QPC, the same clock as the probe's event `qpc`.
  - In run 3 it also read the world matrix (`+0x20`) of game B's entity for A's avatar. That entity's role word
    (`+212`) read `0x00080001`: a replica of kind 1, a player avatar (ACU-RE-0092).

## Result

### Observations

1. **52 leap messages in three runs.**

   | Message | Leap type `+0x50` | `+0x124` | Count | Takeoff → landing, horizontal |
   |---|---|---|---|---|
   | ParkourLeap | 8 | 1 | 23 | 4.3–8.5 m |
   | ParkourLeap | 3 | 1 | 9 | 0.5–2.6 m |
   | ParkourLeap | 35 | 1 | 5 | 2.5–8.3 m |
   | ParkourLeap | 40 | 1 | 1 | 6.6 m |
   | ParkourClimbLeap | 69 | 2 | 5 | 0.4–1.4 m |
   | ParkourClimbLeap | 39 | 2 | 3 | 1.3–4.9 m |
   | ParkourClimbLeap | 73 | 1 or 3 | 6 | 0 m (vertical) |

   - `ParkourHangLeap` and `ParkourVoidLeap` did not fire.
   - The bools at `+0x120`–`+0x122` were 0 in all 52.
   - The message is sent once, at the start of the jump. The player reached the landing point 0.4–3.0 s later.
2. **The message has 14 vectors, not 15.** ACU-RE-0093 said 15; the decoded layout has 14 `vec3_f32` (`+0x30`,
   `+0x40`, `+0x60` … `+0x110`) and one u32 at `+0x50`. They hold three points and their directions:

   | Offset | Role | Evidence |
   |---|---|---|
   | `+0x30` | Player position when the message is sent | Within 0.14 m of the sampled position in all 52 |
   | `+0x40` | Player facing, a unit vector (z = 0 for leaps) | Length 1.000 |
   | `+0x60` | Takeoff point | 0–1.17 m ahead of `+0x30` for leaps. Exactly 1.36 m away for climb leaps 39 and 69. In one pair of leaps it equals the previous leap's landing point. |
   | `+0x70` | Direction at the takeoff point, unit | Length 1.000 |
   | `+0x80` | **Landing point**, predicted before the jump | Types 3, 8 and 40: the master came within 0.0–0.43 m of it. Type 35: 1.1–1.4 m (it probably keeps moving on landing). Climb leaps: 0.0–0.26 m, one at 0.68 m. |
   | `+0x90` | Direction at the landing point, unit (horizontal for leaps, `0,0,1` for climb leaps) | `+0xB0` always equals it |
   | `+0xA0`, `+0xD0`, `+0xE0`, `+0x110` | Equal to `+0x80` in all 52 | — |
   | `+0xC0` | Equal to `+0x80` for `ParkourLeap`; equal to `+0x60` for `ParkourClimbLeap` | — |
   | `+0xF0` | Horizontal unit direction from `+0x60` to `+0x80` for `ParkourLeap` (error 0.000); exactly reversed for climb leaps 69 and 73 | — |
   | `+0x100` | Equal to `+0xF0` for leap types 3 and 8; different for types 35 and 40 and for climb leaps | — |

3. **The replica follows the master closely and lands on the same point** (run 3, 490 paired samples):
   - Game B's copy of A's avatar matched A's position best with about 0.05 s of lag: median error 0.24 m, 90th
     percentile 1.57 m. Both games ran on one PC, so network delay was close to zero.
   - For each of the 19 leaps, the replica came about as close to `+0x80` as the master did. It came within 0.0–0.4 m
     for types 3, 8 and 40 and most climb leaps, 0.53 m for the climb leap the master missed by 0.68 m, and 1.04 m for
     type 35.
   - Its closest sample was −1.3 to +0.8 s from the master's closest sample (the samples are 0.1 s apart).

### Inferences and alternatives

- **The leap message is a jump contract, not a sampled path.**
  - The master sends where the jump starts, where it takes off and where it lands. It also sends the facing at each
    point and a leap type, before the jump happens.
  - The replica can then play the matching animation from takeoff to the known landing point, on the same handhold.
    That answers the ledger's question "how does a replica's parkour stay on the right handholds": the landing point
    is decided by the master and sent ahead.
  - The repeated slots (`+0xA0`/`+0xC0`/`+0xD0`/`+0xE0`/`+0x110` and `+0xB0`) are probably intermediate anchors used
    by longer moves: a second handhold, a mid-air correction, or a roll-out point. In these 52 leaps they collapsed
    onto the takeoff or landing point.
  - The alternative, that some slots are hand or foot targets, is not excluded for moves not seen here (hang leaps,
    void leaps, swing leaps).
- **Replica tracking here does not show which stream moved the replica.** The 0.05 s lag and the matching landings
  are consistent with the replica replaying the leap contract. They are equally consistent with a separate position
  stream moving it. Telling the two apart needs a hook on the replica's receive and apply path, or a test with
  added delay or loss.
- **The leap type at `+0x50` selects the move.** Types 3 (short hop), 8 (long gap jump), 35 and 40 (roll or drop
  variants), and 39, 69 and 73 (climb leaps) are inferred from distances only. Their names are unknown.

### Unknowns / incomplete coverage

- `ParkourHangLeap`, `ParkourVoidLeap`, `ParkourSwingLeap` and `ParkourClimbLeapFromWallEject` share the layout but
  were not seen.
- The derived u64 fields (`0x888`–`0x8E8`) and `+0x128`, `+0x130`, `+0x134`, `+0x198` and `+0x19C` are outside the
  256-byte snapshot, or derived, and were not read.
- How the replica applies the message, and what it does when the landing point is not reachable in its own world
  state (for example a moved object).
- Behaviour under real latency (one PC, loopback only) and packet loss.

### Independent comparison / falsification test

- The field roles were stated after run 1 (19 leaps) and then checked unchanged against runs 2 and 3 (33 more).
  They held for every leap: `+0x30` = position at send, `+0xF0` = direction from takeoff to landing, `+0x80` = landing.
- The master's own path, sampled independently of the hook, confirms each landing point.
- The replica's path comes from the other process's memory, independent of the hooked message.

### Run notes

- In run 2 the replica entity found by an earlier census (`world_entities.py`) gave a constant non-position
  (`2651.7, 0, 0`). Before run 3, a fresh census found the replica at a new address, 0.2 m from A's position.
  Between the census and run 2, A's position had jumped back to the mission start area. A replica avatar can
  therefore be re-created during a mission (here probably after a respawn or out-of-area reset). Not verified.
- Both games were closed with `capture_run.py --close`. The original loader was restored. Saves were backed up
  before the run (`local/game-backups/save3dm-parkour-20261116-092736/`). The game wrote new saves during the run.

### Reproduction steps

1. Set up the testbed: `tools/coop_lan/README.md` steps 1–4, with the custom loader. Use tools from branch
   `acu-multi-launch-direct`.
2. On the joiner, in the mission, run `leapwatch.py` as above. For the replica, find B's animated entity nearest A's
   position with `world_entities.py <pidB> out.json --radius 120`, check that `+212` reads `0x00080001`, and pass it
   as `--rent`.
3. Compare `leap` events with `pos`/`rpos` rows by time.

### Implementation implications and finding-linked constants

- **For a port:** send a parkour leap as a contract.
  - Contents: the move type, the takeoff point, the landing point, and the facing at each.
  - Send it once at takeoff, reliably.
  - The receiver plays the matching move and blends onto the landing point.
- **Modern improvement:** ACU sends 14 × 12-byte vectors (168 bytes) per leap, and 8 of them repeated a point already
  sent in all 52 leaps.
  - A port can send 3 points and 3 directions, quantised to the existing `vec3_q4000` precision. Points are relative
    to the sender's position, which the position stream already carries.
  - It can add a field only when a move has a distinct intermediate anchor.
  - It should also send a timestamp, so a late replica can skip ahead to the landing.
- Constants:
  - hook sites: `0x1406A625D`, `0x1406A9371`, `0x1406A77B7`, `0x1406AB385` (each `45 84 F6`);
  - source object in RBX;
  - player avatar role word `0x00080001` (replica) / `0x00040001` (master).
