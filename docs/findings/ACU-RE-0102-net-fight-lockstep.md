# ACU-RE-0102 — How does the NetFight system run a fight between peers?

Date / investigator / related task: 2026-11-16 / Claude (Opus 5.5), static (capstone and IDA) / #85, the combat row
of `docs/coop/COVERAGE.md` (fight-input stream, CRC, fight join)
State: partially established.
- Established:
  - `NetFight` is a frame-locked (lockstep) fight simulation fed by per-member, per-frame input records;
  - how received fight input is stored;
  - how the simulation picks the frame it may advance to;
  - how join and leave commands are queued and expire;
  - what the CRC check compares, and that a mismatch has no effect in this build;
  - the debug fight recorder (`.nfr` files) that also receives every input.
- Not established:
  - the tick rate's unit (the frame period is 33.33, see observation 1);
  - the quantised input fields' meanings;
  - the join and leave routines' bodies;
  - the kick, master-change and join-in-progress (`NetFightJIPManager`) handlers;
  - the FightDirector staged records (observation 9).

## Provenance

Binary path / SHA-256 / image base / VA / RVA:
- Captured mapped main image, SHA-256 `9a1124f1c4c368939ad3d08315b3dc968e6e48cc22f7b096f34a94a5e7380520`, image base
  `0x140000000`.
- The private IDB `local/85-coop-ida/ACU.runtime-analysis.pe.i64`, opened without auto-analysis.

Corpus path and lines / export identity status: not used.
Archive identity / byte range / range hash / file ID (hex string): not applicable.
Stored and decoded lengths / SHA-256: not applicable (no tool output).

Tool/provider/version / evidence IDs or saved output:
- IDA Pro (idalib, Hex-Rays) through ida-pro-mcp: decompile, xref and immediate searches.
- capstone 5.0.3 linear disassembly of the capture, used wherever Hex-Rays stops early (see limitation below).
- `tools/net_event_listeners.py` (`Listeners.tables`, `Listeners.kind`), for the listener tables in observation 6.

Limitation (decompiler):
- `sub_14211F710` is a stub that calls protected (virtualised) code at `0x14569B1A0`.
- IDA marks it as never returning, so Hex-Rays drops all code after it in many NetFight functions:
  `sub_1406E2D20`, `sub_1406E4AF0`, `sub_1406E1010`, and the `NetFight_RFC` handlers.
- In the private IDB, the prototypes of `0x14211F710` and `0x14569B1A0` were changed to returning functions, and
  `sub_1406E2D20` was undefined and redefined. Neither change restored the output.
- Those functions were read from capstone disassembly instead. The observations below say which.

Search boundary: the NetFight code range `0x1406D8000`–`0x140712000`, the listener tables in `.data`, and
immediate and raw-byte searches of `.text` for each event hash named below.

## Result

### Observations

1. **Fight frame clock.**
   - `sub_1406E3FB0` computes the fight frame as `clock / dword_145176740`. The clock is `sub_141D51070(sub_141CD0D30())`,
     or `*(sub_141CD0D50() + 8)` when there is none.
   - `dword_145176740` holds `33.333332` in the capture.
   - A sender can stamp an input with its own frame (byte flag set, frame at input `+92`); otherwise the receiver
     stamps it with its current frame (`sub_1406EDB20`).

2. **Input messages.**
   - `NetFightManager_RFC_FightInput` (`0x1406EE100`) carries one input: `sub_1406DC9C0` unpacks the wire record,
     then `sub_1406DCEB0` dequantises it.
   - `AllFightInput`, `AllFightInput10`, …, `AllFightInput60` share one handler (`0x1406EDD60` → `sub_1406E2330`) that
     unpacks the same record inline.
   - Dequantisation, the same in both paths:
     - three ints × 0.25 → three floats;
     - two values from `sub_1428F8D50` and `sub_1428F8A50` (a cosine and sine pair, inferred);
     - one unsigned byte → float;
     - one signed byte × 0.20943952 (2π/30, so 12° steps);
     - one u16 × 0.01.
   - Both handlers end in `sub_1406EDB20(manager, input, flag, peer)`, which does two things (capstone; Hex-Rays
     shows only the first):
     - raises event `0x0B3EF277` on the fight-recording entity (`sub_1406F2D70`);
     - calls `sub_1406E2D20(manager, flag, peer, input+104, input)`, the gameplay apply.

3. **Applying received input (`sub_1406E2D20`, capstone).**
   - Nothing happens when the flag is set (the peer's own echo).
   - Otherwise it allocates an input object (`sub_140623C40`) and finds the fight (`sub_1406E00F0`) and the member
     (`sub_140701910`). It creates the member if needed (`sub_140708EE0`, `sub_1406F8550`).
   - It reads action-property strings on the member's current action through `sub_140F8E970`.
   - It then stores the input in the member's per-frame map:
     - `sub_1406F89B0(memberState, frame)` finds or creates a 384-byte record keyed by frame in the map at
       `+584`, and keeps a list at `+552`.
     - `sub_1406F3EE0` keeps a second, frame-sorted list at `+564` and a map at `+576`.
   - The manager also keeps a store of `{entity handle +120, frame +108}` input records at `+324` (count `+334`).
     `sub_1406E0990(manager, entity, frame)` looks them up.

4. **Which frame may run (lockstep).**
   - `sub_1406E0B60` returns the last frame, counting up from manager `+308`, for which every member has an input
     record.
   - `sub_1407036C0(fight)` picks the target frame for a fight as the minimum of:
     - every member's last-input frame (state `+500`);
     - the frame of a member's current action (`+332`), when that action is active;
     - the current frame minus a delay from `sub_1407D8AD0` minus 1.
   - When the CRC flag (observation 7) is set, the target is instead the current frame minus 5 × (`+256` of the same
     settings object).
   - `sub_140708480` (per-fight tick) and the manager tick `sub_1406E1010` call `sub_140701620(fight, target)`.

5. **The simulation step (`sub_140701620`).** For each frame from fight `+184` + 1 up to the target:
   - It collects the members: the active list at `+40` (count `+50`), plus leavers from `+52` whose leave frame
     (`+516`) is later than this frame (`sub_140702C00`).
   - It runs `sub_1407004A0(fight, frame, member, crc)` for each member, then advances `+184`.
   - `sub_1407004A0` takes the member's records for this frame from the `+584` and `+576` maps and resolves the frame
     from them. It applies:
     - health changes (`sub_140CF4310`, `sub_140D0A370`);
     - hits built with `sub_140BB6230` / `sub_140BDE1D0` and submitted with `sub_141E2E640`;
     - kills, with the `IsShootDamage` and `IsKill` action properties;
     - master-only actions where the entity's role bits are `0x40000`.
   - The member simulation `sub_1406F9320` (about 3,600 decompiled lines) uses combat action names: combos
     (`ComboChainInputWindow`, `ResetComboTick`), windups, parry and perfect parry, dodge, `ShoulderCharge_Init_Atk`,
     coups de grâce, `Dead_Vic`, `GetUp`.
   - The manager tick also creates a record for any member that lacks one for a frame (`sub_1406DD070`, then
     `sub_1406F3EE0`), before it runs the member simulation and the step.

6. **The fight recorder.**
   - Event `0x0B3EF277` has one listener table in the image (`0x143B32370`, 26 entries). That table belongs to a state
     the constructor `sub_1406F3310` names `FightRecording` with extension `nfr` (state hash `0x7A1D6A3E`).
   - Its listener `0x1405A4070` copies the input into a 112-byte record and files it per frame (`sub_1406F22A0`). A
     new input for the same entity in the same frame replaces the old one.
   - Join, leave and the victim reply also notify the recorder: events `0xE30E3C62` (`sub_1406F2ED0`), `0x538FBFCD`
     (`sub_1406F3180`) and `0x71E976B9` (`sub_1406F2FF0`).
   - On exit (`sub_1406F3420`), the state writes `FightRecording_<YYYY-MM-DD_HHhMMmSSs>.nfr` when it holds frames.
   - The idle state (`sub_140626180`, hash `0xB94BE5BA`, table `0x143B1CC80`) switches to recording on event
     `0xD3920818` (`0x1405A29B0` → `sub_1406F3AD0`). That hash occurs only in the table. No immediate in `.text`
     raises it.

7. **CRC check.**
   - Both sides are gated on byte `+296` of the object at `qword_1451B4C58 + 6328`.
   - Sender (`sub_140701620`): it finishes a CRC over the frame's member inputs (`sub_140707440` per member from that
     member's join frame `+496`, then `sub_1407074E0`). It sends the CRC with the frame (`sub_140706E60`).
   - Receiver (`NetFight_RFC_SendCRC`, `0x140706AC0`, capstone):
     - It finds the fight's record for the frame (`sub_140701A40`) and queues `{peer, crc}`.
     - `sub_140707680` waits until every session peer has reported. `sub_140707580` checks that all values are equal
       and formats them with `"0x%08X "`.
     - On a mismatch it calls `sub_140F95F30` for each member that reached the frame.
   - `sub_140F95F30` builds and destroys an empty 240-byte structure and returns 0. It does nothing.

8. **Join and leave.**
   - `RFC_JoinFight` (`0x1406EE200`) and `RFC_LeaveFight` (`0x1406EE720`) queue frame-stamped command objects in
     manager `+136` under lock `+152`:
     - type at `+16`: 0 = join, 1 = leave;
     - frame at `+24`;
     - peer at `+28`;
     - echo flag at `+20`.
   - `sub_1406EDC00` drains the queue. It executes a command (secondary-vtable slot `+80`) only if the current frame
     minus the command frame is below 1000. Older commands are deleted unexecuted.
   - The executes notify the recorder, then call `sub_1406E4AF0` (join) or `sub_1406E6140` (leave).
   - `sub_1406E4640` and `sub_1406E4700` answer "is a join (leave) pending for this entity".

9. **FightDirector staged records** (carried over from the queue-drain search in ACU-RE-0101).
   - `sub_14051A250(director, dt, …)` ticks three arrays of 80-byte records: `+832/+842`, `+844/+854` and
     `+856/+866`.
   - Each record's timer at `+52` decreases by `dt`. A record moves from `+832` to `+844` to `+856` and is then
     removed:
     - `+832` → `+844`: below threshold `+736`, after `sub_140519840` and three calls on a manager
       (`sub_141E30220` with hashes `0x9EDB3E9D` and `0xCC4C5B46`, and `sub_141E302D0`);
     - `+844` → `+856`: below `+740`, which also adds `+744` to the timer;
     - `+856` removed: at expiry, after `sub_141E35000`.
   - `sub_14049EFA0` reads the `+832` list by entity.

### Inferences

- **Fights are deterministic lockstep.**
  - Every peer simulates the same fight from the same per-member, per-frame inputs. It advances only to the last
    frame for which all inputs are present.
  - Damage, kills and health therefore come out the same on all peers without being sent as results.
  - A late peer stalls the fight for everyone, which is what `RFC_RequestKickStalled` and `RFC_KickUnresponsive`
    exist to handle (their bodies are unread).
- **The tick rate is probably 30 Hz.** If the clock is in milliseconds, the 33.33 period gives 30 fight frames per
  second. The 12° heading step fits a 30-step circle.
- **The `AllFightInputNN` variants** are batch sizes of the same record (the suffix is the count bucket); this is not
  yet checked against the send side.
- **Desync is detected only in a debug configuration and is never repaired.**
  - The CRC runs only with the `+296` flag, and that flag also delays the simulation by 5 × `+256` frames.
  - A mismatch reaches an empty stub.
- **The fight recorder is a developer tool.** No code raises its start event, so retail games never write `.nfr`
  files. It does document the full input record.

### Implementation choice

The portable library models a fight as:
- a frame clock with a configurable period;
- a per-member input map keyed by frame;
- a step that runs only to the last complete frame;
- join and leave commands that expire after 1000 frames.

Modern improvements over ACU:
- an always-on per-frame CRC that is cheap to compute, with a defined recovery (resend inputs from the last agreed
  frame, then a state snapshot);
- a stall timeout that is visible to the player, instead of a silent wait.

### Unknowns and next checks

- **The clock unit.** Hook `sub_1406E3FB0` on the testbed and log the frame over one real second.
- **The input record's fields.** Log decoded inputs during a two-player fight, and match them to stick direction,
  buttons and target.
- **The join and leave routines** (`sub_1406E4AF0`, `sub_1406E6140`), the kick protocol (`NetFightPeer_RFC_*`,
  `RFC_RequestKickStalled`, `RFC_KickUnresponsive`), `RFC_MasterChanged`, `RFC_VerifyMasterPosition`, and the
  join-in-progress snapshot (`NetFightJIPManager_RFC_*`). All are behind the decompiler limitation, so read them with
  capstone.
- **The FightDirector staged records.** Name the three stages; the hashes passed to `sub_141E30220` are a starting
  point.

## Reproduction status

Static. Each observation names its addresses. The capstone reads use the same capture.
