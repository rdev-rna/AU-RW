# ACU-RE-0097 — What code hands NPC and object ownership between peers?

Date / investigator / related task: 2026-11-16 / Claude (Opus 5.5), static (capstone and IDA) / #85, coverage ledger
"Ownership and migration" gap, and the 11 empty messages of ACU-RE-0093
State: partially established.
- Established: the hand-over primitive, four of its five callers, the distance rule's structure, and the payloads
  of all 963 messages.
- Not established: the sender of the fight load-balance request, and the exact radius table.

## Provenance

Binary path / SHA-256 / image base / VA / RVA: captured mapped main image, SHA-256
`9a1124f1c4c368939ad3d08315b3dc968e6e48cc22f7b096f34a94a5e7380520`, image base `0x140000000`. Decompilation used the
private IDB `local/85-coop-ida/ACU.runtime-analysis.pe.i64` (the runtime-analysis view of the same capture, as in
ACU-RE-0072), opened without auto-analysis.
Corpus path and lines / export identity status: not used.
Archive identity / byte range / range hash / file ID (hex string): not applicable.
Stored and decoded lengths / SHA-256: `local/coop/message_payloads.json` regenerated, SHA-256
`6d9f832ff8791023776a9cd63379fe9a45db3f7a32df10ff7a4b6fccc37359d1` (was `9e688df7…` in ACU-RE-0093).
Tool/provider/version / evidence IDs or saved output: `tools/net_intention_payloads.py`, SHA-256
`ee23048e7800bd4db88ef89e2b7cd6fb290890c864447927d7bbcd0781824d38`, capstone 5.0.3; 9 unit tests pass. IDA Pro
(idalib, Hex-Rays) through ida-pro-mcp: decompile and xref reads only.
Exact commands and search boundary:
- the 11 empty blocks listed in ACU-RE-0093;
- the strings `Give Master Role`, `zenitar_CrowdHerderGroupMigration_TransferGroupMSG_NetProtocol_ID` and
  `RFC_RequestLoadBalanceMigrationMSG_NetProtocol_ID`, and their references;
- every caller of `sub_141C56030`.

## Result

### Observations

1. **All 963 messages now have a layout.** Two decoder gaps closed:
   - **Shared tail.** Ten `FightDirector` blocks store their wire ID, load and compare the header, then jump into
     one shared tail at `0x1404A73E6`. That covers the nine lock and target RFCs and
     `RFC_RequestLoadBalanceMigration`. The decoder now follows that jump. All ten carry the header plus two u64:
     `0x78 ← +0x30` and `0x98 ← +0x48`.
   - **Source register.** The decoder assumed the source object is in RBX. Builders that keep it elsewhere (RDI in
     `FightDirector`, among others) had their owner fields labelled "derived". The source register is now taken from
     the header load (`[reg+0x8]`):
     - 242 messages gained owner offsets;
     - fields with an owner offset rose from 3,180 to 4,310 of 4,657;
     - "derived" fell from 1,435 to 345.

     ACU-RE-0093's counts of "derived" fields are superseded.
   - **`CrowdHerderGroupMigration_TransferGroup`, decoded by hand** (block `0x140480CB0`; source in RBP, which the
     decoder treats as a frame register):
     - header;
     - `0x78` u64 ← `+0x28`, the **destination peer id** (observation 4);
     - `0x98` u32 ← `+0x30`;
     - `0xB8` f32 ← `+0x34`;
     - a member count, a virtual slot at `0xE0` ← u16 `+0x42`;
     - a member array: slot objects of 0x20 bytes at message `+0x108`, each holding an entity handle made from the
       owner's pointer array at `+0x38`;
     - `0x120` ← an entity handle from `+0x48`, the crowd herder.
2. **"Give Master Role" is the hand-over primitive.** `sub_141C56030(netobject+32, peer, flag)`:
   - reads the Storm object at `+440`;
   - proceeds only if byte `+60` is set (this peer holds the object) and byte `+193` is clear (no hand-over pending);
   - sets `+193`;
   - posts Storm's `Give Master Role` event for the given peer through `sub_141B2F9B0`;
   - unless `flag` is set, also calls `sub_141C6ADC0` on the object.

   `sub_141C55F80` is the same request through `+256 → +472`. ACU-RE-0092 observation 18 saw both peers' role
   callbacks run in pairs; that is consistent with this request completing.
3. **Five callers decide when to hand over.**

   | Caller | Trigger | Gives master role to |
   |---|---|---|
   | `sub_140F81210`, a net component update (registered by `sub_140F9DF00` ← `sub_140F980F0`) | **Distance.** Only on the master (`sub_141C50B40`). R = `(int)sub_141DF4190(world+1976)`. Release distance R − 40, claim distance R − 45. Distances are **planar** (x, y). If the owner's player is at least R − 40 from the object, the nearest peer that has the object in scope (`sub_141C51730`) and is closer than R − 45 gets it. | The nearest qualifying peer |
   | `sub_1404AFB50`, the receive handler of `FightDirector_RFC_RequestLoadBalanceMigration` (registered in `sub_1404EF310`; fields decoded in `sub_1404A95A0`) | **Fight.** Arguments are the entities from `+0x30` and `+0x48`. When the first is mastered here and the second is absent or a replica here | The peer encoded in the second entity's handle, `(dword[+8] >> 2) & 0x1F` |
   | `sub_140AA0970` (door interaction) and `sub_140AB3140` (interaction) | **Interaction.** The coordinator's master, on an interaction event. The door also hands over every linked entity (`sub_141CDD2A0`), skipping those where `sub_141C53EF0` is true | The interacting player's peer (`event+24 → +24`) |
   | `sub_14110EE90` / `sub_14110EF50` (crowd herder) | **Crowd groups** (observation 4) | Another peer's herder |

4. **Crowd-herder groups migrate as a unit.**
   - **Sending** (`sub_14110EF50`):
     - for each member of a group that is not a replica here, request "Give Master Role" for the destination peer;
     - collect the members whose request was accepted;
     - if more than one was, send `TransferGroup` through `sub_140481270`. The message carries the destination peer
       (`peer+20`), the group's u32 parameter and the herder (`herder+376`).
   - **Three triggers:**
     - `sub_141112BF0`, the herder reset: every group is handed off before the lists are cleared;
     - `sub_14110EE90`: a single member, when the group has no record;
     - `sub_14111F0C0`, distance: the group is beyond the herder's radius from a reference position. The radius
       comes from `sub_1411076E0`, **15.0 m by default**. A group still inside a second radius (**60.0 m by
       default**, `0x42700000`) is kept if `sub_141E5B050` is true, which is probably on screen.
   - **Receiving:**
     - `sub_140480F30` (needs a protocol version of at least 5) rebuilds the message into a local struct;
     - the execute callback `sub_140481220` resolves the herder;
     - `sub_1410F0EC0` acts only if the struct's first field equals the local peer id (session virtual `+216`;
       ACU-RE-0083 found the same virtual returning the local peer). It then creates a new group (counter
       `+384`), copies the members, appends the group to the herder's list (`+256`, count `+266`), and takes control
       of each member it now masters (`sub_14111E760`).
5. **The radius is per entity.** *(Superseded by ACU-RE-0103: the "per-class table" is a world load-distance grid looked up at the local focus position, so R is the same for every entity on a peer.)* `sub_141DF4190` returns a u16 distance:
   - an override at `+340`, when set; otherwise
   - a per-class table (`sub_1420F1EA0` index into `+56`, default `+20`);
   - capped by `+176`.

   It is a streaming or load distance, not a network constant.

### Inferences and alternatives

- **The measured hysteresis is this rule.**
  - ACU-RE-0092 observation 20 measured a claim boundary between 49.83 and 52.56 m. The owner kept the entity at
    55.48 m and released it at 59 m (Danton) or 88.8 m.
  - With claim = R − 45 and release = R − 40, both bounds hold only when R is 96 or 97 m. That also fits the unload
    seen between 100 and 111 m, if unloading happens a little beyond R.
  - The live distances were not all planar, so the bounds carry some error.
- **Danton has no special rule.** He follows the same distance rule with his class's R. The fight path can also move
  him to a fighting player's peer.
- **Why the joiner simulated the enemy** (ACU-RE-0092 observation 6). Either the fight request handed the NPC to
  the attacking player's peer, or the distance rule did. Which one was not observed.
- **Mission coordinators do not move**, because none of these callers covers them. Only distance-, fight-,
  interaction- and crowd-driven entities migrate.

### Unknowns / incomplete coverage

- The sender of `RequestLoadBalanceMigration`. No caller uses the name or the wire ID directly. Next step: hook the
  builder block (`0x1404A6E2C`) during a fight on the non-owning peer, and read the return addresses.
- The radius table's contents per class, and what `world+1976` is.
- What `sub_141C53EF0` (skip linked entities) and `sub_141E5B050` (the 60 m exception) test.
- Storm's side of "Give Master Role": acceptance, refusal and timeouts (`+193` pending).

### Independent comparison / falsification test

- The R − 40 / R − 45 structure was read statically. The ACU-RE-0092 distances were measured live before it.
  Together they constrain R to 96–97 m.
- Live tests:
  - read `sub_141DF4190`'s result for the alarm bell and Danton (expect 96 or 97);
  - check that a hand-over happens when a planar distance crosses R − 45 / R − 40;
  - hook the load-balance builder during a fight.

### Implementation implications and finding-linked constants

- **For a port:** ownership moves for four reasons, and the library needs all four:
  - distance, with a claim radius 5 m inside the release radius, both tied to the entity's own load distance;
  - fight engagement, by request from the attacking peer;
  - interaction, where the interacting peer claims the object and its linked objects;
  - group transfer for crowd herds, where all members move together.

  Each is a request ("give master role") that can be refused while another is pending.
- **Modern improvements:**
  - make the radii data per entity class, as ACU does, but include height for vertical play;
  - send an ownership epoch with each request so a late answer cannot double-assign. The library's tie-break covers
    this.
- Constants: `sub_141C56030` / `sub_141C55F80` (`Give Master Role`); `sub_140F81210` (distance, R − 40 / R − 45);
  `sub_141DF4190` (per-entity R); `sub_1404AFB50` (fight request handler); `sub_140AA0970` and `sub_140AB3140`
  (interaction); `sub_14110EF50`, `sub_1410F0EC0` and `sub_14111F0C0` (herder; 15 m and 60 m defaults); local peer id
  through session virtual `+216`.
