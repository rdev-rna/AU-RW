# ACU-RE-0072: How does ACU identify, register and dispatch its network messages?

Date / investigator / related task: 2026-11-15 / Claude (Opus 5.5), static only / #85, co-op plan Phase 1 ([COOP_RE_PLAN.md](../COOP_RE_PLAN.md))
State: partially established. The registry, the ID scheme and the type records are observed and cross-checked. Several callback roles are still inferred (listed below).

## Provenance

Binary path / SHA-256 / image base / VA / RVA: mapped runtime view
`E:/ReverseEngineering/Projects/Unity/local/native-probe/motion-link-20261020/runtime-view/capture/main-image.mapped.bin`,
SHA-256 `9a1124f1c4c368939ad3d08315b3dc968e6e48cc22f7b096f34a94a5e7380520`, image base `0x140000000`
(the same capture as ACU-RE-0060). IDA analysis view `ACU.runtime-analysis.pe`, SHA-256
`47a81dc9f29c48a709cbdc1ef60a7e7db9eb8caefaabf30940dd4e1ee755d8c2`. `analysis-view.json` binds this view to
the mapped SHA above and to disk `ACU.exe` `748abdc6…7e40`.
Corpus path and lines / export identity status: not used. All VAs below were read in IDA on the analysis view.
The corpus binds to this build only by headers and resources (ACU-RE-0031).
Archive identity / byte range / range hash / file ID (hex string): not applicable.
Stored and decoded lengths / SHA-256: not applicable.
Tool/provider/version / evidence IDs or saved output: IDA Pro 9.3 via the ida-pro-mcp idalib server, headless,
session `acu85`. It ran on a private copy of the IDB in `local/85-coop-ida/` with no auto-analysis,
no renames, no patches and no save. Lock #31 was acquired first.
`tools/net_protocol_atlas.py` (stdlib only, tool SHA-256
`9efc0467034ca0713744dc080f27223c43a606b233d5c30ebcf95dc09e83d9b0`) writes
`local/coop/net_protocol_atlas.json`. Two runs produced byte-identical output.
`tools/net_census.py` (SHA-256 `6ae3e884…f01678`) gives the layer counts in the plan.
Exact commands and search boundary:

```powershell
python tools/net_protocol_atlas.py `
  --image "E:/ReverseEngineering/Projects/Unity/local/native-probe/motion-link-20261020/runtime-view/capture/main-image.mapped.bin" `
  --expect-sha256 9a1124f1c4c368939ad3d08315b3dc968e6e48cc22f7b096f34a94a5e7380520 `
  --output local/coop/net_protocol_atlas.json
```

The tool scans the whole `.rdata` for strings, the whole `.data` for type records and the whole `.text` for
RIP-relative LEAs. It uses `.pdata` for function bounds.

## Result

### Observations

1. **The ID hash is standard CRC-32.** `sub_1426D10D0` hashes a C string through `sub_1426B5E40`, a
   reflected table CRC-32 with initial value `0xFFFFFFFF` and a final NOT. The table at `0x143632DC0` has
   entry 1 = `0x77073096` and entry 2 = `0xEE0E612C`, which is the zlib table. Python `zlib.crc32` reproduces
   every hash quoted below.
2. **Net type records.** A lazy getter (for example `sub_14067F9E0`) fills a static record through
   `sub_14012A540(record, name, parent_record)`. The record layout is `+0x00` name pointer, `+0x08` u32 depth
   `n`, and `+0x10` `n` entries of 16 bytes each (u32 CRC-32, then a qword that is 0 in this capture). The
   entries copy the parent's chain and then add CRC-32(own name). Example: record `0x14517B4C0`
   (`NetFightJIPManager_RFC_RequestJIPMSG_NetProtocol`) has depth 5 and the CRCs
   `A0217162 956E4C8E 1284CF25 630A821C 8EC5A41A`. These are, in order, `storm::DataContainer`,
   `NetObjectDataContainer`, `NetObjectMessage`, `NetObjectRemoteFunctionCall` and the message itself. The
   getters `sub_140128FE0`, `sub_140129060`, `sub_140172430` and `sub_140172480` build those four ancestors.
   The tool accepts a `.data` record only if its last CRC equals CRC-32 of its own name. **1,508 records**
   pass that check.
   - Roots: `storm::DataContainer` 1,219, `storm::Event` 230, `NetObject` 21, other `storm::*` 38.
   - Direct bases: `NetObjectRemoteFunctionCall` 958, `NetObjectMemento` 118, `NetObjectDescriptor` 17
     (parent `storm::echo::ObjectDescriptor`), `NetObjectRemoteFunctionCall[Diff|DiffBase]` 15.
     Several `storm::echo::{session,object,voicechat,registry,simulation,peerChannel}` event bases also appear.
3. **The wire ID of a message is a CRC of a name string.** The encoder `sub_1406DD580` compares an input
   key with CRC-32(`zenitar_NetFightJIPManager_RFC_RequestJIPMSG_NetProtocol_ID`) = `0x49D997D7`. It then
   creates the message object and writes its ID at `+0x08`. With the unicast flag (4th argument) set, it
   writes `0xBB746201`, which is CRC-32 of the same string with `_UNICAST` appended. That suffixed string
   does not exist in the image, so the code builds it at runtime.
4. **Static registrar list.** `sub_1427488D0(node, category, …, register_fn, unregister_fn)` links a
   `{register, unregister, next}` node into the list at `off_14525FAE8[category]` (for example thunk
   `sub_142D80170` → node `0x145176A28`). Category 1 holds **62 nodes**. Every registrar holds the broadcast
   and unicast CRCs of exactly one `zenitar_*_NetProtocol_ID` string as imm32 values, and **62 of the 63**
   such strings have a registrar. The exception is `zenitar_GcLAbstractNetData_NetProtocol_ID`.
5. **What a registrar does** (`sub_140683350`, JIPFailed). It takes the protocol interface at
   `qword_145139D00 + 0x110` and calls three virtuals on it:
   - vtable `+0x38` with each of the two keys `0xB29E970A` (broadcast) and `0x0BD55A54` (unicast), passing
     a pair `{sub_1406DE4C0, sub_1401525E0}`.
   - vtable `+0x40` with the broadcast key, passing `{sub_1406DD580, sub_14022EC80}`.
   - vtable `+0xC0`, which returns a dispatcher. `sub_140151D50(dispatcher, key, sub_1406DEE30, 1)` then
     allocates a 64-byte entry `{vtable off_142E84060, key, owner, flag, callback}` and inserts it into the
     hash map at `dispatcher + 0x10`.

   The four `NetFightJIPManager` registrars and the four `MissionMatchmakingNetHelper` registrars all pass
   the same owner-wide pair at `+0x38`/`+0x40` (`0x1406DE4C0`/`0x1406DD580` and `0x1404ED470`/`0x1404ED0A0`).
   The final callback is different for each message.
6. **The send primitive** is `sub_14044B4C0(dispatcher, out, id, payload, options)`. It looks up `id` in the
   same `dispatcher + 0x10` map (`sub_14277AA00`) and returns a null result if `id` is missing, so sending
   only works for a registered ID. It numbers each send with `_InterlockedIncrement(&dword_145212F3C)` and
   puts that counter together with the ID into a single 64-bit tag. IDA shows **217** direct code xrefs to
   this function.
7. **Object-scoped messages, the large family.** Send sites build the ID at runtime. They concatenate a
   string returned by a virtual on the sending object (vtable slot 0 in `sub_140A5A490`, slot `+0x10` in
   `sub_140735A90`) with a literal such as `RequestDetectiveOwnership_RFCMSG_NetProtocol_ID` or
   `StartCLRiftWanderMSG_NetProtocol_ID`. They CRC the result and call `sub_14044B4C0`. The dispatcher comes
   from the object's net handle (`+0x230` in `sub_140A5A490`, `+0x718` in `sub_140735A90`) through vtable
   `+0xC0`. In `sub_140A5A490`, when the handle is null, the function skips sending and calls the handler
   body `sub_140A5A190` directly. Per-object channel subscription: `sub_140746BF0` builds `vf0()` +
   `_NetFightJIPManager_NetProtocol_ID`, CRCs it and registers two callbacks through the handle's vtable
   `+0x38`. `sub_140758180` reverses this through vtable `+0x48`.
   - The image has **1,639** object-scoped `…_NetProtocol_ID[_UNICAST]` literals, and every one is loaded
     by at least one function. It has **93** `_<Owner>_NetProtocol_ID` channel literals.
   - **901 of 974** non-factory RFC type records match a literal of the form `<message>_NetProtocol_ID`
     (with or without the owner name). Of the 73 that don't, 62 are the static `zenitar_` set; the rest are
     bases and a few unexplained names.
8. **The three acceptance owners resolve by two independent routes.** Route 1 is the registrar list or the
   send-site literal. Route 2 is the self-checked type record.

   | Owner | Route 1 | Route 2 |
   |---|---|---|
   | `NetFightJIPManager` | 4 registrars, each with 2 keys and its own callback: `0x1406DF0D0` RequestJIP, `0x1406DF2E0` SendJIPReply, `0x1406DF510` SendJIPSnapshot, `0x1406DEE30` JIPFailed | 4 records at `0x14517B4C0`, `+0x60`, `+0xC0`, …, each under `NetObjectRemoteFunctionCall` |
   | `MissionMatchmakingNetHelper` | 4 registrars (`0x1404E5B10` to `0x1404E5E70`) | 4 records from `0x1451668C0` |
   | `NetHumanIntention` | object-scoped; for example `StartCLRiftWanderMSG_NetProtocol_ID` is sent from `0x140735A90` | 96 records (94 RFC); channel literal `0x142EC6160` |

### Inferences and alternatives

- The pair registered at `+0x38` (`sub_1406DE4C0`, used for both keys) is the owner-wide **decoder**. It
  branches on the CRC of each message ID string, which the IDA string references show. The pair at `+0x40`
  (`sub_1406DD580`, which writes fields into the new object and sets dirty flags) is the **encoder**. The
  `sub_140151D50` callback is the **RFC body** run on receipt. All three roles are inferred from code shape.
  The receive path that calls them has not been traced yet.
- The 118 records with base `NetObjectMemento` (for example `HumanAvatar_NetProtocol`,
  `AnimComponentNetData_NetProtocol`, `BhvGenericNPC_NetProtocol` and most `GcL*NetData`) look like
  replicated **state**, not calls. The alternative is a snapshot used only for join-in-progress. Phase 3
  should settle this.
- The prefix `zenitar_` is probably the vf0 name of objects that are not entity-scoped (managers and
  helpers). That would make the static set a special case of the object-scoped scheme. This is not checked:
  no vf0 implementation that returns `zenitar` has been read yet.
- A null net handle makes the game run the call locally. This suggests single-player and co-op share one
  code path in which networking is optional. This matters for porting. It has been seen at one site only.

### Unknowns / incomplete coverage

- The receive path: the place where Storm hands a packet to the decoder. That is Phase 2/3.
- The meaning of the per-registrar helpers `sub_1401525E0` and `sub_14022EC80`, and of the `options` fields
  (`reliable = 1`, `4`, `-1`, `-1`).
- The vf0 prefix strings, so object-scoped IDs cannot yet be computed offline. The atlas lists the literals
  and their functions, not final ID values.
- The second qword in each record entry is 0 in this capture. Its meaning is unknown.
- Why `zenitar_GcLAbstractNetData_NetProtocol_ID` has no registrar.
- `.pdata` misses some function starts (for example around `0x140746F13`). Those references are reported by
  instruction address rather than function.
- This is one capture of one build. Other builds are not checked.

### Independent comparison / falsification test

- The record hashes were recomputed with Python zlib, not with game code, and all of them match. The CRC
  table matches zlib at entries 1 and 2.
- The registrar keys were first unexplained: `0xB29E970A` did not match the obvious owner strings. A
  brute-force CRC over every string in the image found exactly one match,
  `zenitar_NetFightJIPManager_RFC_JIPFailedMSG_NetProtocol_ID`, and its `_UNICAST` CRC equals the second
  key `0x0BD55A54`. A wrong theory about the hash would fail this test.
- Record detection checks itself: a random `.data` qword that points at a string passes only if the
  16-byte-stride CRC at depth `n` equals that string's CRC-32.

### Reproduction steps

1. Run the atlas command above, then compare `summary` with the counts in this finding.
2. In IDA on the analysis view, decompile `0x1406730E0`, `0x14067F9E0`, `0x14012A540`, `0x1406DD580`,
   `0x140683350`, `0x1427488D0`, `0x140151D50`, `0x14044B4C0`, `0x140A5A490` and `0x140735A90`. Read only;
   don't paste their code into the repo.

### Implementation implications and finding-linked constants

- `REGISTRAR_LISTS = 0x14525FAE8`, category 1, node = `{register, unregister, next}`.
- Message ID = CRC-32(`<prefix><message>_NetProtocol_ID`), and the unicast ID adds the `_UNICAST` suffix
  before hashing. Our transport can carry these 32-bit IDs unchanged.
- The message type hierarchy roots in `storm::DataContainer`. Game messages sit on top of Storm's container
  and serializer, so the Storm/Echo layer is not optional for an ACU-side shim (seam options A and B in the
  plan).
- For porting (option C), the unit of work is the owner: one decoder/encoder pair plus N message bodies.
  The atlas gives the owner → message → record list.
