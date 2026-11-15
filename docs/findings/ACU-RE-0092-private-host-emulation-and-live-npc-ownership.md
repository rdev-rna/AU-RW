# ACU-RE-0092 — Hosting a co-op mission without the UI, and who simulates a mission NPC

Date / investigator / related task: 2026-11-16 / claude-session, live / #85, co-op plan Phase 4 (combat, AI).
Testbed: the ACU-RE-0090 two-game LAN setup on one PC (`tools/coop_lan/`).
State: partially established.
- Observed: live `SendFightDamageEventToMaster` sends with their callers in both games, and the attacker-side hit
  routine that starts them.
- Observed: the Private-hosting call chain, its live emulation, and discovery joining by itself.
- Observed: one live fight with the intention traffic it caused.
- Not settled: the hand-over from a received damage intention to the master's damage handler.

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

10. **The hit routine always routes damage through the intention system** (static, then live):
    - `sub_140576270` (3,202 bytes; only caller `0x1405760E0`) is the attacker-side hit resolution. For each victim
      it takes a `FightDamageEvent` from the event pool (`sub_140BC7BE0`), fills it with
      `sub_140BDE710(event, …, attacker, victim, …, damage)` and `sub_140BDB050`, then delivers it (corpus lines
      509–539):
      - **victim kind 2** (`(victim+212 & 7) == 2`, sets `v149`; live: NPC humans, observation 17) and bit
        `0x40000000000000` of `victim+136` is set:
        direct apply via `sub_140FDEEC0(component(victim+96), victim, event)`;
      - **victim kind 2** otherwise: NHI `SendFightDamageEventToMaster` via `sub_1407348A0`, using the victim's
        NetHumanIntention component (`sub_140708FC0`). `sub_1407348A0` builds
        `"<type>SendFightDamageEventToMasterMSG_NetProtocol_ID"` and starts the intention with mode 4;
      - **any other kind** (the `else` branch; live, kind 1 is the player avatar, observation 17): NBAI
        `SendFightDamageEventToMaster` via the component that `sub_140C17990` → `sub_141ACDF40` returns →
        `sub_1406D40D0` → starter `0x1405DA480`.
      - Correction (observation 17): this finding first called kind 2 "human" and kind 1 "AI NPC". The live census
        shows the opposite assignment of players and NPCs, so the *names* NetHumanIntention / NetBehaviorAI do not
        say who the victim is.
    - Mode 4 is ACU-RE-0081's case table. The intention router decides the delivery from the victim component's
      state: on a replica the message goes to the master; on the master it stays local. The hit code itself does not
      check ownership.
    - Live caller chains, from `damagewatch6`, which snapshots at the builder's entry RSP. All are
      starter → `0x140569520` → object path `0x141C66360` → `0x141C47130` (dispatcher `+0x80`) → builder:

      | Game | Message | Starter | Count |
      |---|---|---|---|
      | joiner | NHI `SendFightDamageEventToMaster` | `0x1405DA9E0` | 3 |
      | joiner | NBAI `SendFightDamageEventToMaster` | `0x1405DA480`, above it `0x1426F4740` | 1 |
      | host | NBAI `SendFightDamageEventToMaster` | `0x1405DA480`, above it `0x1406D3FA0` | 1 |
      | joiner | NHI `DoHurt` | `0x1405E93C0` | 1 |
      | host | NBAI `DoHurt` | `0x1405E93C0` | 1 |

      Run: the restarted mission's first wave with both players beside Danton, t = 1204–1254 s. Each player hit
      NPCs, and the NPCs hit both players.
    - Other callers of the starters:
      - `0x1405DA9E0` is also called by the receive handler `0x1407269B0` and by the thin wrapper `0x1406698E0`;
      - `0x1405DA480` by `0x1406691E0`, `0x1406ADE30` and `0x1406D40D0`;
      - `0x1405E93C0` by six functions, including `0x14071B270`, `0x14071B410` and the receive handler.

11. **Health: the master owns life, replicas display the replicated value** (static, then live):
    - `CSrvHealth` reflects `Life` (field 6) with accessors: getter `0x140EBA2E0` → `sub_141206A80` and setter
      `0x140B1F9E0` → `sub_14122E430(health, life)`. The setter checks the value against the max at `+236` (int16).
    - The getter returns `sub_14066B710(*(health+240))` when byte `+260` is set, and otherwise the local int16 at
      `+256`. A health component therefore either reads a replicated value or holds its own.
    - `sub_140B25550` is the RFC receive handler for `ZenHealthReplication_RFCSetMasterLife`. It is reached from
      the ZenHealthReplication RFC dispatch `0x140B237E0`, which matches the incoming ID against
      `"<type>RFCSet…MSG_NetProtocol_ID"`.
      - When the owner entity's role bits `(entity+212 & 0xC0000) == 0x80000`, it calls the life setter on its
        `CSrvHealth` (`+72`) with the master's value.
      - It caches the value at `+24` and notifies through its vtable `+112`.
    - Role bits: the hit routine (observation 10) tests `(attacker+212 & 0xC0000) == 0x40000` for the locally
      controlled case. `0x80000` is the replica case here. Kind bits `& 7`: 1 = player avatar, 2 = NPC human
      (live census, observation 17; corrected).
    - Live (`damage7.jsonl`, life setter hooked in both games, about 100 s during the "STEAL the documents" phase):
      - **Regeneration.** The setter ran about once per second for each player-health object in both games, called
        by the health state machine (`0x140ED04A0` ← `0x1427605F0`, from rule updates `0x142102100` /
        `0x142760410`).
      - **Players.** Each game holds two such objects, its own player and the remote player's copy. The values
        agreed across games: 46 and 35 on both sides, with no damage in that window.
      - **Replicated NPC life.** Four calls in the joiner came through the RFC path instead
        (`0x14022EC80` → `0x140B237E0` → `0x141B6BB70` → `0x14021D730` → `0x140B25550` → setter), each writing
        life 60 to a different object: the host sending its NPCs' authoritative life to the joiner's replicas.
      - **Both directions, over 636 s:**
        - the joiner applied 95 replicated life values to 79 objects;
        - the host applied 78 to 73 objects. The host therefore also holds replicas whose master is the joiner.
        - The values were mostly NPC maxima (15, 20, 30, 60, 100, 134). `0xFFFFFFFF` (−1) marks a dead entity.
      - **Damage steps.** One replica in the joiner went 30 → 22 → 14 → 6 → −1, one RFC per step: its master
        applying successive hits and pushing each result.
      - Other life writers seen, not yet classified (possibly respawn and checkpoint reset):
        - `0x140ECB240` ← `0x1427556F0` (state-machine action);
        - `0x140EA22D0` and `0x14124BC30` ← `0x141212900`.

12. **What `HumanReplica` changes** (static; `local/symbol_index.sqlite` method table plus corpus):
    - The destructor `0x140BF9E80` sets four vtables, so the class has extra bases:
      - `0x142F18FC0` at `+0`;
      - `0x142F19120` at `+456` (40 slots, 15 of them `nullsub_7` or `return 0`);
      - `0x142F19260` at `+464`;
      - `0x142F19298` at `+2448`.
    - The interface query (slot 30) returns `this+456` for id 6 and `this+2448` for id 64, and otherwise defers to
      `HumanBase` (`0x141EC18C0`). On `Human`, slot 30 is `0x14131B5E0`, the interface query that the debug
      teleport uses (ACU-RE-0046).
    - Slot 32 is the behaviour reset that `teleport.py` calls on `Human` (`0x14131A530`). On the replica it is
      `sub_140C1D020(this+456)`, which drives the `+456` sub-object. Slot 14 runs `HumanBase`'s slot-14 body
      (`0x140C20960`) and then the same driver call.
    - Slots 13 and 16 do work in `HumanBase` (`0x141EB1C50`, `0x141EC3190`) and are `nullsub_7` on the replica.
    - Slot 40 grows from 71 bytes (`0x1402900E0`) to 720 bytes (`0x140C2B190`). Slot 39 stores a byte at `+1856`
      and pushes it to a looked-up object's `+41`.
    - Class names for the three extra vtables are not in the symbol index.

13. **The master's damage apply site** (live, then static):
    - In the host, the life setter wrote 22 → 14 → 6 → −1 to one health object (`0x126188870`) at t = 393–411 s.
      The chain was `0x140EBBFD0` → `0x141212900` → `0x14124BC30` → setter.
    - The joiner's replica received the same values over `RFCSetMasterLife` (observation 11). So the host was this
      NPC's master, and each applied hit was pushed to the replica.
    - No `SendFightDamageEventToMaster` ran in that window, so the hit was local to the host. The attacker was not
      identified.
    - `sub_140EBBFD0(handler, event)` (3,439 bytes; no static callers and no vtable slot, so it is invoked as a
      registered handler) reads these event fields:
      - `+8`, an entity handle;
      - `+16`, the field the hit routine sets to 6 or 8 in observation 10;
      - `+20` (values 6 and 26 are tested), `+24` (5 and 18 are tested) and `+32`;
      - the flag word at `+40` (bits `0x1`, `0x8`, `0x20`, `0x200`, `0x400`).

      It then broadcasts the new life with `sub_141212900(listeners, life)` (corpus line 541).
    - `sub_141212900` iterates the entity's listeners registered under `dword_1451D7D78`, until one returns a value
      other than 3. `sub_14124BC30(listener, life)` writes the life setter on `*(listener+24)` and returns 2.
    - Inference: `0x140EBBFD0` is the health component's `DamageEvent` handler, and the master runs it for a
      damage event. A forwarded hit (observation 9) is expected to arrive here too. The link from the rebuilt
      intention to this handler was not traced.

14. **Each intention message has an execute callback, and only the master runs it** (static, then live):
    - The registration function for NetHumanIntention messages, `0x140746EF0` (corpus line 1804), registers three
      functions for `SendFightDamageEventToMaster`:
      - the receive handler `0x1407269B0`;
      - the send builder `0x14071F090`;
      - an execute callback `sub_140734650`, registered with `sub_1405FBC70`.

      NetBehaviorAI does the same in `0x140738E30` (line 2752), with `sub_1406D3F30` registered by `sub_1405F8310`.
    - Both callbacks are master guards. They read the victim (`a3`) and continue only if `victim+212 & 0xC0000` is
      `0x40000` (local master):
      - NHI `0x140734650` → `sub_1407346F0`;
      - NBAI `0x1406D3F30` → `sub_1406D3FA0`.
    - Both executors do the same thing:
      1. take a `FightDamageEvent` from the pool (`sub_140BC7BE0`);
      2. fill it from the intention with `sub_140BDE710(event, a2+64, attacker, victim, a2+84, a2+125, a2+80)`;
      3. copy `+92` → event `+20`, `+96` → event `+28`, `+104` → event `+368`, and the flags `+126`/`+127` →
         event `+40` bits `0x20`/`0x1`;
      4. NHI only, when the victim is an AI master: `+120` → event `+16`;
      5. post the event to the victim with `sub_141E2E640(event, victim)`. That function picks the victim's event
         target and calls `sub_1420F5B40`. NHI with `a4` ≠ 0 uses `sub_141E2E7A0` instead, which takes an extra
         argument.
    - **Where the callback runs** (static, `sub_1405DA9E0` corpus lines 66–216):
      1. The starter looks the intention id up in the component's map at `a1+16`. `sub_1405FBC70` stores the
         64-byte callback record (`sub_1405BDE10`) there.
      2. It builds a context from its last argument `a7`:
         - `a7+12` is the source; −1 means generate locally;
         - `a7+16` is copied to record dword 7, the target peer;
         - `a7+20`;
         - `a7+24`, an existing record;
         - byte `a7+32`, the execute flag.
      3. When the execute flag is set, it calls the record's delegate (`rec+56`) with
         `(rec+32, object, a4, payload, a6, context)`. That is the master guard → executor.
      4. Only after that does it route the message: `sub_141C66360`/`sub_141C66660(a1, path, intention, record
         dword 7)`, where the peer argument is the `a4` of observation 15's table.
      5. So the execute step happens before the send, which matches the live order on one thread.
    - Inference: when the master restarts a received intention, the peer in its context is the sender. That would
      make the host's `_ID_UNICAST` message (observation 15) a reply to the requesting peer. The receive handler's
      context `v4` was not decoded.
    - The event reaches the health handler `0x140EBBFD0` through the entity event dispatcher `0x1420F3200`. That
      dispatcher binary-searches the handler table at `a1+16` (count u16 at `a1+26`, 16-byte entries `{u32 type, ptr}`)
      using the type id at `+36` of the event's type record.
    - Three paths reach the damage apply:
      - the NBAI sender `sub_1406D40D0` (line 54) calls `sub_1406D3FA0` directly after starting the intention when the
        victim is local master;
      - the hit routine's "direct" branch `sub_140FDEEC0`, the only caller of which is `0x140576270`, appends
        `{event, handle}` to a pending array on the victim's component; it does not post the event;
      - in every other case the event goes out as an intention.
    - Live (`damagewatch9`, `damage9.jsonl`), with hooks on both executors and both shared send points:
      - The NHI executor runs for kind-2 victims (NPC humans, observation 17). So in both cases the victim was an
        NPC owned by the joiner, most likely hit by the joiner's player. The first draft said "the joiner's own player"
        and was corrected.
      - Each time, the NHI executor ran (return `0x1407346D2`, inside the guard). Its intention held `+84` = 7,
        `+92` = 11, `+120` = 8 and `+125`/`+126` = 1, with `+80` = 0 the first time and 30 the second.
      - In the same poll and on the same thread (tids 111372 and 130688), the NHI `SendFightDamageEventToMaster` block
        ran with `r12b` = 0. That branch loads `0x6527A678`, the message's `_ID` hash (the atlas calls it the
        broadcast ID; see observation 15 for the routing). Its caller chain was
        builder ← `0x141C4718D` ← `0x1405695C7` ← `0x141C664D4` ← `0x1405695C7` ← starter `0x1405DA9E0`.
      - The executors never ran in the host, and the host sent nothing.
    - So a hit on an entity the game owns is applied by that game and then sent on under the same message name (route
      2, no target peer). A replica's guard fails, so the broadcast copy cannot apply damage a second time. Health then reaches
      the other game through `RFCSetMasterLife` (observation 11).

15. **A forwarded hit, live: request to the master, execute there, reply to one peer** (live, then static):
    - `damagewatch9` (`damage9.jsonl`, t = 667.97 s and 688.4–688.5 s) caught the same pattern twice. Both players
      had godmode on, so they were not taking damage, and were attacking enemies beside Danton.
    - **Joiner:**
      - the NetBehaviorAI `SendFightDamageEventToMaster` block ran with `r14b` = 0, which loads `0x42E553D6`, the
        message's `_ID` hash;
      - its caller chain was builder ← `0x141C4718D` ← `0x1405695C7` ← `0x141C6655E` ← `0x1405695C7` ← starter
        `0x1405DA480` (return `0x1405DA8B7`) ← `0x1426F480E`;
      - no NBAI execute ran in the joiner. The victim NPC was a replica there.
    - **Host, in the same poll:**
      - the NBAI execute `sub_1406D3FA0` ran (its master guard passed). Its intention held `+80` = 30, `+84` = 7,
        `+92` = 11, `+120` = 8 and `+125`/`+126` = 1, the same values as the joiner's local hits;
      - on the same thread, the NBAI block ran with `r14b` = 1, which loads `0x4BE42FC0`, the `_ID_UNICAST` hash.
    - The NBAI path is the hit routine's `else` branch, which covers non-kind-2 victims; live, the player avatars are
      kind 1 (observation 17). So the victim was the *host's player*, which is a replica in the joiner. The attacker
      was most likely an NPC that the joiner simulates:
      1. the joiner sent the damage request to the player's master;
      2. the host rebuilt it and executed it, so it was applied on the master;
      3. the host then sent the message on, addressed to one peer.

      Godmode was on in both games, and the host's life change was not checked. The first draft read this as "the
      joiner's player hit a host-owned NPC" and was corrected.
    - **Static: where the branch flag comes from.**
      - The builder's flag is its fourth argument (`movzx r14d, r9b` at `0x14069F21C`, `movzx r12d, r9b` at
        `0x14071F0AC`).
      - It is the last argument of the object's message factory, vtable `+128`, called from the object path
        `sub_141C66360` (corpus lines 51–183).
      - The path takes the intention's mode `a3[5]`. Mode 4, used by `SendFightDamageEventToMaster` (ACU-RE-0081),
        becomes route 1 when `obj+56` (relative to the object base `*a1-136`) is non-zero. Otherwise it becomes route 2
        when the target peer `a4` is −1, and route 1 when `a4` is a peer. Mode 5 becomes route 3 when `obj+56` is 0,
        and is not sent when `obj+56` is non-zero.
      - Each route builds a 24-byte send record with a route type at `+4`:

        | Route | Condition | Factory flag | Wire ID | Record |
        |---|---|---|---|---|
        | 1 | `obj+56` = 1 | 0 | `_ID` | type 1 |
        | 1 | `obj+56` ≠ 1 | 1 | `_ID_UNICAST` | type 1, target `+16` = `a4` |
        | 2 | — | 0 | `_ID` | type 2, `+20` = `a3[6]` ≠ 0 |
        | 3 | — | 1 | `_ID_UNICAST` | type 3, target `+16` = `a3[7]` |

    - Inference: `obj+56` = 1 marks a replica of the object, so the request goes to its master on the plain `_ID`.
      A master uses `_ID_UNICAST` to address one peer, here most likely the requester, or `_ID` (route 2) to address
      everyone.
      - The joiner's hits on its own NPCs (observation 14) fit route 2: master, flag 0, no target peer.
      - The atlas labels "broadcast"/"unicast" only name the two hashes (`_ID`, `_ID_UNICAST`). They do not mean
        "to all" and "to one" in every case.

16. **The reverse direction: a joiner-owned NPC's death replicated to the host** (live, `damage10.jsonl`):
    - The last spy of "KILL the Austrian spies" was killed by the joiner.
    - **Joiner, at t = 631.72 s:**
      - the damage handler `0x140EBBFD0` ran (event `+16` = 41, `+20` = 1, `+24` = 1, `+32` = 20, flags `0x598`);
      - the life setter then wrote 0 through `0x140EBCCFB` → `0x14124BC30` → `0x14121295D` → `0x14124BC3D`.
    - **Joiner, at 635.95 s:** a second event (`+24` = 5, `+32` = 0, flags `0x1598`) wrote −1.
    - **Host:**
      - its copy wrote 0 at 631.80 s and −1 at 635.95 s, through the `RFCSetMasterLife` receive handler (return
        `0x140B2558F`, dispatch `0x140B237E0`);
      - with 0.5 s polling, the first value arrived within one poll (80 ms apart).
    - No NBAI request or execute ran in either game. The kill was local to the NPC's owner, the joiner.
    - The mission then advanced to "STEAL the documents" in both games.
    - Together with observation 13 (host-owned NPC → joiner replica), life replication is now observed in both
      directions, with each NPC owned by the game that fought it.

17. **Role census and where NPC roles are set** (live, then static):
    - **Census.** `world_entities.py` (80 m radius) listed the animated entities in both games, then `READ entity+212`
      in each. The run was in the "STEAL the documents" phase, both players near (930–935, 228–230):

      | Game | Animated | Kind 2, master | Kind 2, replica | Kind 1 |
      |---|---|---|---|---|
      | host | 313 | 289 | 9 | `0x40001` at its own player's position, `0x80001` at the joiner's player |
      | joiner | 310 | 296 | 5 | `0x40001` at its own player, `0x80001` at the host's player, matched within 0.3 m |

      - Kind 1 is the player avatar (`player` flag from `world_entities.py`).
      - Kind 2 entities are NPC humans. Nearly all are local masters in *both* games: crowds are simulated per game,
        not replicated.
      - The few kind-2 replicas are the mission NPCs. Raw values `0x8000a`, `0x80012` and `0x80102` were seen; the
        bits above the kind are unknown. Several replicas in the host (for example (901–903, 239–240) and
        (918.7, 230.9)) had no entity within 1.5 m in the joiner's scan. Positions are only roughly synchronised, or
        the master was outside the scan.
    - **Static: roles are written once, at creation.** Two of the three functions that clear bits 18–19 in the corpus
      (`0xFFF3FFFF`) are relevant:
      - `sub_140C1E360` (line 51) creates an entity (`sub_141CC3230`) and sets `entity+212` bits 18–19 to
        `2 - (a3 != 0)`: `a3` ≠ 0 gives master (`0x40000`), `a3` = 0 gives replica (`0x80000`). It then adds the
        entity to the world (`sub_141CBEC30`) and starts its behaviour (`sub_140D0AAD0`). Its only callers are:
        - `0x1419F2DE0`, which passes 1 (local creation, from `0x141988870` / `0x1419C7610`);
        - `0x14191A730`, which passes 0 and copies a creation record (`r8`, fields `+84`…`+104`). It has no direct
          caller and is referenced by data pointers at `0x144098B60` and 5 more.
      - `sub_140C92B80` unpacks the same bitfield from a stream (`(v & 3) << 18`), so the role can also arrive as
        serialized data.
      - Which system owns `a1+7304` / `a1+3380` in those callers is not identified. Hooks on both callers did not fire
        while the players were teleported between courtyard positions (`spawn1.jsonl`), so they are not the crowd
        spawner.
    - Inference: ownership is decided when an entity is created, by whichever creation path runs. There is a local
      creator that makes masters and a data-driven creator that makes replicas. Crowd humans are created locally
      in every game and never shared. No live role change of an existing entity was observed.

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
- **Damage is always an intention addressed to the victim's owner (observation 10).** The attacker's game never
  writes the victim's health. It starts `SendFightDamageEventToMaster` on the victim's own net component: kind-2
  victims (NPC humans) use NetHumanIntention, and other kinds (player avatars are kind 1) use NetBehaviorAI. The ownership-aware intention router then either
  runs it locally or sends it to the master. This one mechanism covers player→NPC, NPC→player and NPC→NPC hits, and
  the gameplay code needs no co-op branches.
- **The full damage loop, as far as observed (observations 7–11):**
  1. the attacker's hit routine starts `SendFightDamageEventToMaster` on the victim's net component;
  2. the router delivers it to the victim's master;
  3. the master rebuilds the event as an intention and applies it (handler `0x140EBBFD0` broadcasts the new life,
     observation 13);
  4. the master's health state machine owns `Life`;
  5. replicas receive it through `ZenHealthReplication_RFCSetMasterLife`, and their getter reads the replicated
     value.

  Step 3 runs as a master-gated execute callback. The callback rebuilds a `FightDamageEvent` and posts it to the
  victim's event dispatcher (observation 14). It was caught live for local hits. A forwarded hit was caught
  live for a player victim (observation 15): replica request on `_ID` → master execute → master message on
  `_ID_UNICAST` to one peer.
- **The request lane exists and runs (observation 7).** In the spy fight, the joiner, which simulated the attacking
  spies, sent `SendFightDamageEventToMaster` while the host's player was being hurt. One hit, with no caller
  captured, so this is consistent with the prediction but does not prove it. Correction (observation 17): that block was
  the NHI one, which serves kind-2 victims (NPC humans), so its victim was an NPC, not the host's player. The master rebuilds the event as an
  intention and runs it locally (observation 9), then broadcasts the outcome on the `SetLastDamageEvent` lane with
  the identical record (observation 8).
- **For the AC4 mod:** a hit on a puppet does not have to be replayed on the puppet. The machine that owns the
  victim applies it and sends the result as an intention. F7's "the damage lane does not exist yet" matches
  `SendFightDamageEventToMaster`'s role. The outcome lane (`SetLastDamageEvent`, `EnterIncapacitated`) is separate,
  and the mod needs both. For health, ACU never lets a replica compute its own life. The puppet's health should be
  a display of the owner's value (an `RFCSetMasterLife` equivalent), not a second health pool.

## Unknowns / incomplete coverage

- the ownership rule and hand-over for mission NPCs;
- whether ownership was the joiner's because of proximity or for another reason (the host was 5 m away, the joiner 1 m);
- the HUD health asymmetry: replica health may be local-only, or the HUD may show each game's view of the partner;
- the meaning of each field of the shared damage record (`+0x30`, `+0x80`…`+0x90`, `+0xa8`…`+0xaf`, `+0xc8`);
- a forwarded hit on an *NPC* victim (NHI, kind 2). Only a player victim (NBAI) was caught forwarded
  (observation 15). Godmode hid whether the forwarded hit lowered the player's life;
- what `obj+56` is (inferred: the replica marker), and who the host's `_ID_UNICAST` target peer was;
- what the `victim+136` bit `0x40000000000000` means, and who drains the pending `{event, handle}` array that
  `sub_140FDEEC0` fills;
- the meaning of the executor's intention fields `+80` (0 / 30), `+84` (7), `+92` (11), `+120` (8);
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
