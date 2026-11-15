# ACU-RE-0092 — Hosting a co-op mission without the UI, and who simulates a mission NPC

Date / investigator / related task: 2026-11-16 / claude-session, live / #85, co-op plan Phase 4 (combat, AI).
Testbed: the ACU-RE-0090 two-game LAN setup on one PC (`tools/coop_lan/`).
State: partially established.
- Observed: one live `SendFightDamageEventToMaster` send, by the game that simulated the attacking NPCs.
- Observed: the Private-hosting call chain, its live emulation, and discovery joining by itself.
- Observed: one live fight with the intention traffic it caused.
- Not settled: the caller of that send, and where the master writes health.

## Provenance

Binary: installed `ACU.exe` SHA-256 `748abdc6…27e40`. Mapped runtime image SHA-256 `9a1124f1…80520`, base
`0x140000000`.

Corpus: `ACU - Decompiled/<VA>.c`, whole files: `140D6D440`, `14250CD00`, `14256CC80`, `142552BC0`, `14256F380`,
`14255A080`, `14256ADC0`, `14256AE70`, `1410968F0` and the message type getters `1406803xx`–`1406820A0` listed below.

Tools:
- the native probe's `CALLAT`, `READ` and hardware hooks;
- the focus shim's in-process pad;
- `teleport.py`, `world_entities.py` and `player_state.py`;
- the asset index (`resource` table, type `5fdacba0`, confidence `unknown`);
- `local/coop/net_protocol_atlas.json` (type record VA → name).

No IDA or REA session.

Runs and files: `local/native-probe/runs/overnight-20261116/`:
- `privhost.jsonl` (the Private call);
- `lancall3.jsonl` (discovery);
- `damage2.jsonl` and `damage2-console.txt` (hooks during the fight);
- `msg_vtable_names.json`;
- `registered-entities.json` (the fight's NPC pointers).

Game PIDs: host 122956 (ports moved by 10), joiner 76404.

## Result

1. **The map's "Play in Private" route** (static):
   - The map handler `0x140D6D440` ends with `(*(*qword_145219C80) + 136)(qword_145219C80, *(*(v1+240)+16), *(u8*)(v1+172))`:
     - the second argument is the mission id;
     - the third is the private flag (1 = Private).
   - Slot 136 of that object's vtable `0x143604F60` is `0x14250CD00`, a thunk to `sub_14256CC80(qword_145137928,
     mission, flag)`.
   - `sub_14256CC80` allocates an `OnlineRequestMatchmakingEvent` (vtable `0x1435FB050`) with the allocator
     `0x142529570`, stores the mission at +32 and the flag at +40, and posts it.
   - The `PRIVATE_MATCH_HOST` state entry `0x142552BC0` sets the matchmaking context string through `0x14256F380`.
     `PRIVATE_MATCH_HOST_START_ALONE` is used instead when the matchmaker byte `+1472+236` is set.
   - Before the call, the handler runs UI work (`sub_140903BC0`) and a player-count check against the mission database
     (`qword_1451CA240`, lookup `0x1410968F0`, count at +288). The emulation skips both.
2. **The mission id.**
   - While the host sat in the Private lobby of "The Austrian Conspiracy" (objective "DEFEND Danton"), its matchmaker
     (`*qword_145137930`) held `000000104ccb592a` at `+0x718`.
   - The asset index names that ID `COOP_RB050`, type `5fdacba0`, confidence `unknown`.
   - The same type holds the other co-op missions: `COOP_LB010`–`LB060`, `COOP_RB010`–`RB060`,
     `COOP_MH010`–`MH070`, the heists `HE01`–`HE09_M010` and `COOP_DLC_M010`. Their IDs are in the index.
3. **Live emulation, no UI.**
   - A fresh host stood in a building in free roam. One `CALLAT 0x14250CD00(obj, 0x104ccb592a, 1)` on the game thread
     returned within 0.03 s; the probe piggy-backed on Storm interface slot 12, `0x141C626C0`.
   - 0.2 s later the context read `PRIVATE_MATCH_HOST` and `+0x718` held the mission. No lobby menu appeared.
4. **Discovery alone joins.**
   - After the joiner ran the LAN matchmaker's discovery (slot 4, `0x1425406D0`, `lancall.py`), it took the host's
     game session id by itself. No `JoinSession` call was made (`lanjoin2.py` not run).
   - The host showed "Waiting for the other players…" and then both games loaded into DEFEND Danton with two players
     on the HUD, about 45 s after the discovery call.
   - This differs from runs 10–13 of ACU-RE-0090, where the joiner called `JoinSession` itself. The host here was a
     fresh `PRIVATE_MATCH_HOST` lobby created by the emulated call.
5. **Teleport inside a co-op mission.**
   - `teleport.py` moved the joiner 40 m and back, and later the host 40 m, while the mission ran. Both games kept two
     players and the objective.
   - The session loss seen after the earlier teleport (ACU-RE-0090 run 15 follow-up) is therefore not caused by
     teleporting in itself. Its cause is unknown.
6. **One fight: the joiner simulated the enemy.**
   - The enemy fighting Danton beside the courtyard statue had host entity `0x13A895150` and joiner entity
     `0x189BAEAC0`; both use entity vtable `0x143017D60`.
   - Its `[E+0x130]+0x10` id is a runtime id (`00000000f002b23c` on the host, `00000000f0035b63` on the joiner), not an
     asset file ID.
   - The joiner killed it; the objective completed in both games.
   - In the 60 s around the fight the host's shared intention receive handler `0x1407269B0` received bursts of NPC
     intentions: 10–27 per type per 10 s, against 0–2 at idle. Names come from the message vtable's type getter (slot 1)
     → type record → atlas:
     - `NavigateInFight`, `NavigateToNavTarget`, `NavigateToPatrolPoint`, `NavigateInFormation`;
     - `LookAtRequest`, `CancelLookAt`, `OrientedLookAt`, `OrientateToDangerSource`;
     - `StartGuardActionWithTimeOut`, `StartLeaderIntentionModifier`, `DisableIntentionModifier`;
     - `Sheathe`/`UnsheathePrimaryWeapon`, `SheatheSecondaryWeapon`, `CancelReload`;
     - `SetLastDamageEvent`, `EnterIncapacitated`;
     - `PlayCustomActionWithParams`, `BroadcastPlayedCustomActions`.

     All carry the `NetHumanIntention_` prefix and the `MSG_NetProtocol` suffix. The joiner received only a few
     (sheathe, patrol, `AskForBroadcastPlayedCustomActions`).
   - The three builder blocks hooked in both games had **0 hits** through this fight: `SendFightDamageEventToMaster`
     `0x140722B60`, `DoHurt` `0x140722643` and `DoHurtInternal` `0x14072275B`. Earlier, Danton lost health to NPCs
     with the same hooks armed and none ran either.
   - HUD after the fight: each game showed the *remote* player's health bar partly empty and its own full.

7. **Correction: the earlier hooks covered only the broadcast branch; the first live send.**
   - ACU-RE-0091's blocks `0x140722B60` / `0x140722643` / `0x14072275B` load only the *broadcast* wire ID. They sit
     after `test r12b, r12b / je`, so a unicast send skips them.
   - Each message block in the NetHumanIntention send builder `0x14071F090` (frame `0x1120` + 5 pushes) has the same
     shape. It compares the intention id `[rbx+8]` with the message type's id, calls the type getter, then branches
     on `r12b`.
   - The shared points before the branch, verified with Capstone 5.0.3:

     | Builder | Message | Shared point | Branch |
     |---|---|---|---|
     | NetHumanIntention | `SendFightDamageEventToMaster` | `0x140722B4D` | `test r12b` |
     | NetHumanIntention | `DoHurt` | `0x140722630` | `test r12b` |
     | NetHumanIntention | `DoHurtInternal` | `0x140722748` | `test r12b` |
     | NetBehaviorAI (`0x14069F200`, frame `0x2290`) | `SendFightDamageEventToMaster` | `0x1406A01AD` | `test r14b` |
     | NetBehaviorAI | `DoHurt` | `0x1406A03F2` | `test r14b` |

   - The 327-byte per-message functions (e.g. `0x1407798C0`, `0x1407700A0`, `0x140775FD0`) only call
     `sub_141C42310`/`42370`/`42550` with the wire IDs. That is static RFC registration, not sending.
   - Live, with hooks on the four shared points in both games during the spy fight ("KILL the Austrian spies",
     9 of 10 killed):
     - the **joiner** ran the NetHumanIntention `SendFightDamageEventToMaster` block once (tid 12376, 10 s after
       arming, `damage5.jsonl`);
     - the host's health was falling at the time;
     - in the same 60 s, no NetBehaviorAI block and no `DoHurt` block ran in either game.
   - That hook snapshot covered only the builder's own frame, so its caller is not known. `damagewatch6`
     snapshots at the builder's entry RSP (`RSP+0x1148` / `RSP+0x22B8`) to catch the caller next time.
8. **One damage record, two lanes** (static, `local/coop/intention_payloads.json`):
   - `SendFightDamageEventToMaster` and `SetLastDamageEvent` carry the same 16 fields, taken from the same source
     intention offsets: `+0x8`, `+0x14`, `+0x30`, `+0x70` (vec3), `+0x80`…`+0x90`, `+0xa8`, the bools
     `+0xac`…`+0xaf`, and `+0xc8`.
   - Message vtable `0x142EC3D90` sits next to `SetLastDamageEvent`'s `0x142EC3D28`. Type getter `0x140681740`;
     type id cache `dword_14517CFD0`.
9. **Master-side receive** (static):
   - The only user of the type check `0x1405F5920` (depth ≥ 5 and hash `+80` == `dword_14517CFD0`) is the shared
     intention receive handler `0x1407269B0`, at corpus lines 2398–2434.
   - That branch decodes the message fields at `+120` … `+688` into a local intention. It builds the id from
     `"<type>SendFightDamageEventToMasterMSG_NetProtocol_ID"` and starts the intention locally with
     `sub_1405DA9E0(manager, out, id, &intention, …)`.
   - `0x1405CF7F0` is the message's `FactoryEntry`, registered at startup by `0x140685FD0` together with every other
     intention message.
   - What the started intention does on the master (where health is written) was not traced.

## Inferences

- **NPC simulation sits with one machine at a time, and it was not the mission host.** The enemy's intentions flowed
  joiner → host. The joiner, which was next to it and fighting it, ran its AI. The host ran a replica driven by those
  intentions.
  - This matches the GDC 2015 description of per-NPC ownership. The ownership rule (proximity, engagement or
    assignment) and the hand-over were not observed.
- **A hit by the owner on its own NPC needs no damage message to the master.** The owner applies the damage locally and
  broadcasts the outcome as intentions (`SetLastDamageEvent`, then `EnterIncapacitated`). This is why the
  `SendFightDamageEventToMaster` block stayed silent.
  - Prediction, not yet tested: the block runs when a player hits an NPC that the other machine owns, or when an NPC
    hits the other machine's player.
- **The request lane exists and runs (observation 7).** In the spy fight, the joiner, which simulated the attacking
  spies, sent `SendFightDamageEventToMaster` while the host's player was being hurt. One hit, with no caller
  captured, so this is consistent with the prediction but does not prove it. The master rebuilds the event as an
  intention and runs it locally (observation 9), then broadcasts the outcome on the `SetLastDamageEvent` lane with
  the identical record (observation 8).
- **For the AC4 mod:** a hit on a puppet does not have to be replayed on the puppet. The machine that owns the
  victim applies it and sends the result as an intention. F7's "the damage lane does not exist yet" matches
  `SendFightDamageEventToMaster`'s role. The outcome lane (`SetLastDamageEvent`, `EnterIncapacitated`) is separate,
  and the mod needs both.

## Unknowns / incomplete coverage

- the ownership rule and hand-over for mission NPCs;
- whether ownership was the joiner's because of proximity or for another reason (the host was 5 m away, the joiner 1 m);
- the HUD health asymmetry: replica health may be local-only, or the HUD may show each game's view of the partner;
- the meaning of each field of the shared damage record (`+0x30`, `+0x80`…`+0x90`, `+0xa8`…`+0xaf`, `+0xc8`);
- the caller of the one observed `SendFightDamageEventToMaster` send, and where the master writes health;
- whether the emulated Private call works from every free-roam state (one run, from inside a building);
- any mission id other than `COOP_RB050` (the asset-index name is not verified against the map's own id table).

## Independent comparison / falsification test

- Repeat the fight with the host player attacking an NPC whose intentions flow joiner → host. Expected: the host's
  `SendFightDamageEventToMaster` block runs and the joiner's NPC loses health.
- Negative control: the joiner attacks the same NPC. Expected: no send, only `SetLastDamageEvent`.
- Emulate Private with `COOP_LB010`'s id. If the lobby context appears with that id at `+0x718`, the index ID is the
  map's mission id.

## Reproduction

```
python tools/coop_lan/lanoffset2.py <out.json> --port-offset 10   # host, background, then launch at fps cap 30
python tools/coop_lan/privhost.py <host_pid> <out.jsonl>          # Private for COOP_RB050, no UI
python tools/coop_lan/lanflag.py <out.json> --timeout 120         # joiner, background, then launch at fps cap 60
python tools/coop_lan/lancall.py <joiner_pid> <out.jsonl>         # discovery; the joiner then follows by itself
```
