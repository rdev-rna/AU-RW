# ACU-RE-0099 — How are the last six co-op state types bound and encoded?

Date / investigator / related task: 2026-11-16 / Claude (Opus 5.5), static (capstone and IDA) / #85, the six state
types ACU-RE-0095 left undecoded
State: partially established.
- Established: the binds of five of the six types, and their codecs or slot layouts.
- Not established: a codec bind for `AIComponent_NetProtocol`.

## Provenance

Binary path / SHA-256 / image base / VA / RVA:
- Captured mapped main image, SHA-256 `9a1124f1c4c368939ad3d08315b3dc968e6e48cc22f7b096f34a94a5e7380520`, image base
  `0x140000000`.
- Decompilation used the private IDB `local/85-coop-ida/ACU.runtime-analysis.pe.i64`, opened without
  auto-analysis.

Corpus path and lines / export identity status: not used.
Archive identity / byte range / range hash / file ID (hex string): not applicable.

Stored and decoded lengths / SHA-256: `local/coop/memento_codecs.json` regenerated, SHA-256
`bbdd522af664958c5cd2d8d517d8dc3f0eb9c4321641550d48f6829fce09d5b2` (was `bae4ccc2…`).
- Five records changed.
- The other 113 records are byte-identical to the previous output.

Tool/provider/version / evidence IDs or saved output:
- `tools/net_memento_codecs.py`, SHA-256 `4ea91b23ece40f100079d2cde970ec622b72c48057b76c6515c78ba13a9111f7`,
  capstone 5.0.3. Its tests pass, including two new bind-shape tests: 10 tests in all, and 24 together with the
  payload and handler tests.
- IDA Pro (idalib, Hex-Rays) through ida-pro-mcp: decompile, disassembly and xref reads only.

Exact commands and search boundary:
- A whole-`.text` scan for the bind shape: `mov dword [rsp+X], imm32`, followed within 8 instructions by
  `lea r8, [rip+fn]` and preceded in the same function by `lea rdx, [rip+fn]`. It found 114 bind sites, among them
  all 112 known wire IDs.
- A whole-image byte search for each remaining wire ID (broadcast and unicast).
- References to each type record and to the strings `CrowdLife_NetProtocol_ID_%d` and `Proxy_NetProtocol_ID`.

## Result

### Observations

1. **`NetBehaviorAI_NetProtocol` (wire `0xA77567A8`).**
   - It is bound inside the NBAI registration function `sub_140738E30` (block `0x140738E53`). That function builds
     each delegate with `sub_1405F61E0(delegate, owner, &fn)`, which fills `{fn, 0, owner, invoker 0x1401B78D0}`.
     It builds the read delegate from `sub_1406C5ED0` and the write delegate from `sub_1406C66F0`. It then calls
     the registry's slot 0 with `rdx = &{0xA77567A8, 0}`, `r8 = read` and `r9 = write` (`0x140738EF1`).
   - The write delegate decodes to 119 fields:
     - 21 top-level fields from owner `+0x44C` to `+0x84A`;
     - 20 per-entry fields in an array at memento `+0x238`, filled from owner `+0x4D0` through `sub_14064DB30`;
     - 78 per-entry fields in an array at `+0x330`, filled from owner `+0x5E0` through `sub_1407625C0`.
   - The newest-wins stamp is owner `+0x858`. The registration function zeroes that field first
     (`mov [rcx+858h], ebx`, `0x140738E73`).
   - Read check: 6 inline fields match, none differ.
   - Ten setter functions are not yet named (for example `0x14018C060`, which is used 33 times, and
     `0x140782920`).

2. **`LivingBreathingManager_NetProtocol` (bind wire `0x20D5A915`).**
   - Bind `sub_1408F74C0`: read `sub_1408F4A50`, write `sub_1408F4A60`. The unbind `sub_1408F7660` and a third
     site at `0x1408FF507` use the same immediate.
   - The write delegate sends one bool from owner `+0x2E4`.
   - The init function that reaches the bind (`0x14109E370` → `jmp sub_1408F7580` → `sub_1408F74C0`) clears the
     same byte (`*(a1 + 740) = 0`, `0x14109E382`).

3. **`VillaTrophyManager_NetProtocol` (bind wire `0xCF871231`).**
   - Bind `sub_1408F7590`: read `sub_14090B910`, write `sub_14090BD50`. It is reached from init `0x141499170`
     (`jmp sub_1408F7650` → `sub_1408F7590`).
   - It has 9 fields:
     - six `vslot_u32` from owner `+0x362` to `+0x3A2`;
     - one u32 at `+0x37C`;
     - two bools at `+0x3A4` and `+0x3A8`.
   - 3 inline fields match the read side.
   - Both managers are set up by one parent, `sub_141499060`. It registers itself, then initialises the object at
     its `+16` (`0x141499170`) and the one at its `+24` (`0x14109E370`).

4. **`GcLCrowdLifeProtocolContainer` is bound once per slot.**
   - Registration `sub_140E81880` registers 40 slots for `NetGcLCrowdLifeNetContext` and 2 for `NetGclEntity`.
     Each slot's key is `CRC-32("CrowdLife_NetProtocol_ID_<i>")`; for example slot 0 is `0xFC8DC541` and slot 39
     is `0xA70DC885`.
   - Bind `sub_140E59D10`, for each slot: read `{owner, sub_140E81620}` and write `{owner, sub_140E99FE0}`, with
     the invoker `0x14249ED70`.
   - On the owning side (`+227` set), it sets 40 slots (`+364`) of 30 entries (`+360`). Elsewhere, it sets
     `min(members, 30)` entries and `ceil(members / 30)` slots, where `members` counts the crowd members it sees.
   - Write `sub_140E99FE0`:
     - checks the memento type (`record+64 == dword_1451C1F60`) and sets the entry count (`vslot`);
     - for each of the slot's 30 entries (152 bytes each, at memento `+128`), sends a u64 (the entity's
       `vfunc+8`, slot `+112`) and a u32 (its `vfunc+16`, slot `+144`);
     - clears both when the entry is empty.

5. **`ProxiesProtocol` is bound once per slot. Its codec is ACU-RE-0076/0077.**
   - Each slot's key is `CRC-32("0x%08X_%d" % (K, i))` for slots 0–29 (`sub_141C508C0`), with
     `K = CRC-32("Proxy_NetProtocol_ID") = 0xAC1B997A` (set by `sub_142DF1590`). For example slot 0 is
     `0xAF93FD78`.
   - Registration `sub_141C676C0` registers the 30 slots for `NetProxyManager`.
   - Bind `sub_141C42920` runs once per peer. It allocates a 552-byte context and binds read
     `{ctx, sub_141C66E20}` (→ `sub_141C66C10`, the receiver of ACU-RE-0077) and write `{ctx, sub_141C787F0}`
     (→ `sub_141C78190`, the dead-reckoning writer of ACU-RE-0076).
   - The unbinds are `sub_141C5E010` (peer leaves) and `sub_141C60CD0`.
   - Capacity: 30 slots × 10 proxies per peer.

6. **`AIComponent_NetProtocol` has no codec bind found.**
   - Its registration wire `0x57218EFC` occurs only in the two object-type registrations at `0x141F38CAC` and
     `0x141F38D39` (ACU-RE-0084). These register it for the object types `0x2545E4EC` and `0x5E02482C`.
   - Its type record `0x14521B5E0` is referenced only by its factory (`sub_141F18310`, `sub_141F22AC0`) and type
     function (`sub_141F2C800`).
   - No function in its module (`0x141EF0000`–`0x141F50000`) builds delegates other than the 13 known binds.

### Inferences

- **AIComponent is probably registered but never bound in this build.** Every other state type binds under an
  immediate or under a slot key, and AIComponent has neither. A runtime check would settle it: list the registry
  entries of a live `NetNpc`.
- **The two slot types scale by count.** CrowdLife covers up to 40 × 30 = 1,200 crowd members; each entry is an id
  plus a state value. Proxies cover up to 300 proxies per peer. A port can size these tables, but it should keep
  the per-slot split, so that one slot's change does not resend all of them.

### Implementation choice

`tools/net_memento_codecs.py` now handles three more cases:
- `BIND_WIRE`: wire IDs that occur only at their bind site;
- the delegate-init bind shape (read first, then write);
- `SLOT_MEMENTOS`: key format, slot counts, bind, read and write, with output status `slot memento`.

The summary is now: 115 decoded, 2 slot mementos, 1 with no bind found; 1,424 fields.

### Unknowns and next checks

- A live registry listing for a `NetNpc`, to confirm that AIComponent is never bound.
- The ten unnamed setter functions used by NBAI and the Rift components.
- The meaning of the CrowdLife entry's u32, which comes from the entity's virtual `+16`.

## Reproduction status

Static. Re-running `tools/net_memento_codecs.py` on the same capture and atlas reproduces the output.
