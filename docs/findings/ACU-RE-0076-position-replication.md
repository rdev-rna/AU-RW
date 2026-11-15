# ACU-RE-0076: How does ACU replicate entity position and heading?

Date / investigator / related task: 2026-11-15 / Claude (Opus 5.5), static only / #85, co-op plan Phases 3/4 ([COOP_RE_PLAN.md](../COOP_RE_PLAN.md))
State: partially established. The proxy protocol's sender, thresholds and encodings are observed. The
receiver's interpolation, the bit widths, and whether player avatars go through this path are not established.

## Provenance

Binary path / SHA-256 / image base / VA / RVA: same capture and analysis view as ACU-RE-0072 to 0075:
mapped image `9a1124f1…0520`, analysis view `47a81dc9…d8c2`, image base `0x140000000`.
Corpus path and lines / export identity status: not used.
Archive identity / byte range / range hash / file ID (hex string): not applicable.
Stored and decoded lengths / SHA-256: not applicable.
Tool/provider/version / evidence IDs or saved output: IDA Pro 9.3, ida-pro-mcp idalib, headless session
`acu85`, on the private IDB copy. Read only, closed without saving, lock #31 held. Type records, wire IDs
and class layouts come from `local/coop/net_protocol_atlas.json` and `local/symbol_index.sqlite`.
Exact commands and search boundary: the VAs listed below. The constants were read with `get_bytes`.

## Result

### Observations

1. **Snapshots seen so far carry no position.** The `AnimComponentNetData` codec has the same five-helper
   pattern as `HumanAvatar`:
   - registrar `sub_14012A610`; read `sub_1401286C0`; write `sub_140128700`;
   - fields: source `+0x18` f32 (changed at ≥ 0.001), `+0x1C` u32, `+0x20` u32, `+0x24` f32 and `+0x28` f32;
   - sequence number at `+0x2C`, with the same "apply only if not older" rule.

   Neither this nor `HumanAvatar` (ACU-RE-0075) holds a position.
2. **Snapshot registry key = the wire ID.** The key `sub_14012A610` registers is `0x1A11C194`, which is
   CRC-32(`zenitar_AnimComponentNetData_NetProtocol_ID`). The avatar's key is `0xA92DBEA6`, which is
   CRC-32(`zenitar_HumanAvatar_NetProtocol_ID`). This corrects ACU-RE-0075, which hand-converted that key
   wrongly as `0xA92E6CA6`.
3. **The `NetPlayer` object type is a container.** `sub_140F99FB0` registers object type `NetPlayer`
   (key `CRC("NetPlayer")` = `0x5E02482C`) with descriptor `NetPlayerDescriptor` (`0x4D2BA204`). Its only
   fixed channel is the generic dynamic RFC protocol, `NetObjectRemoteFunctionCallDynamic` with protocol
   IDs `NetObjectRemoteFunctionCallDynamicProtocolId` (`0xB8E7D06D`) and `…_UNICAST` (`0x36CAFCA9`).
4. **Entity replicas bind by ID to local entities.** `sub_141C5B2D0` handles incoming objects:
   - For a `NetEntityObjectDescriptor` (record `0x1451C6110`), it searches the local entity list for the
     entity whose `+0x10` equals the descriptor's `+0x110`, and passes the descriptor to that entity's
     component (vtable `+0xB0`).
   - Otherwise it routes by type key (`+0xF0`) through the handler map, or creates the object from the
     descriptor's type record.
5. **The proxy pool.** `sub_141C676C0` registers object type `NetProxyManager` with **30** `ProxiesProtocol`
   snapshot entries (record `0x145214E30`, wire ID `0x99EA0E79`), recording each slot index in
   `qword_1452136E0`. The writer `sub_141C78190` serves **10 sub-entries of 584 bytes each** per entry, so
   the pool can hold up to 300 entities.
6. **Per-entity writer logic (`sub_141C78190`).** For each sub-entry bound to an entity (reference at
   `entity_slot + 0x1F0`, sent as a u64 field at sub-entry `+0x70`):
   - **Current position** is the translation row of the entity's world matrix, with w cleared by the mask
     `xmmword_1435EDEE0`.
   - **Thresholds** depend on a per-proxy precision flag (`proxy + 0x30`):

     | Mode | Prediction error² | Distance² since last send |
     |---|---|---|
     | precise | 0.0025 (5 cm) | 6.25 (2.5 m) |
     | normal | 0.25 (50 cm) | 16 (4 m) |

   - **Prediction** applies when the proxy has already sent once and more than 100 ms have passed.
     `sub_141C4F3C0` predicts `p = p_last + v_last·(dt_ms·k·0.001)`, caps the step length when a cap is given,
     and clamps z to ±2000.
   - **Velocity estimate:** `v = 1000·(p − p_last)/dt_ms`. The constant `xmmword_142FFC880` is 1000.0.
   - **Position and velocity are re-sent** when the entity has just stopped (`|v|` ≤ 1e-5 in every axis
     while `v_last` was not), when it moved further than the distance threshold, or when the prediction error
     exceeds the error threshold. A change of the 5-bit context value `(a3[0x200] >> 2) & 0x1F` also resets
     the proxy and forces a send.
   - **After a send,** the proxy's `p_last`/`v_last` are read back from the **quantized** fields
     (`sub_141C507D0`, `sub_141C50CF0`). The sender therefore predicts from exactly what receivers
     reconstruct.
   - **A u32 timestamp** (sub-entry `+0x1C8`) is written whenever position is sent.
   - **Heading** is a scalar taken from matrix row 1 by `sub_141CF4160`. It is re-sent when it changes by
     more than 0.175 rad (about 10°), into sub-entry `+0x168`.
   - **A flag** comes from the entity's virtual `+0x78`, into sub-entry `+0x1A8`.
7. **Encodings:**
   - **Position** (`sub_141C72630`): per-axis signed fixed point, x clamped to ±4000, y to ±3000 and z to
     ±2000. For x, `code = trunc(v·(1/4000)·(2^bits − 1))` (`sub_141C707D0`). Each axis is re-sent only on
     a change of at least 0.01.
   - **Velocity** (`sub_141C72820`): split into a unit direction and a speed. The direction is sent as x
     and z, each in [-1, 1] (`sub_14018BE90`), plus 1 bit for the sign of y. The speed is clamped to 340
     and sent through `sub_141C70500`. Zero velocity is sent as direction (1, +, 0) with speed 0.

### Inferences and alternatives

- **Characters move with dead reckoning.** Snapshots carry state; the `ProxiesProtocol` pool carries
  position, velocity and heading for up to 300 entities, with extrapolation on both sides. Combined with
  intention RFCs (ACU-RE-0075), this gives the full movement model. The alternative is that proxies serve
  only distant or low-detail entities and nearby players use another path. The precise/normal flag hints
  at such tiers.
- **Coordinates are probably metres with z up.** The ±2000 clamp applies to z, and the ranges match a city
  map. This is not established; the project rules forbid guessing native axes, so a live check is needed.
- **Static world entities (NPCs, props) are matched by entity ID,** not spawned. Both peers therefore need
  the same world data. This has to hold in any port.

### Unknowns / incomplete coverage

- The receiver: how incoming proxies are applied (interpolation, snapping), and which code reads
  `ProxiesProtocol`.
- The bit widths of each axis and of yaw and speed. They are set outside the codecs.
- The meaning of the precision flag and of the 5-bit context value. The context value may be a zone or
  teleport counter.
- Whether the local player's avatar is assigned a proxy slot.
- What `sub_141C4F3C0`'s scale `k` and step cap (passed by the caller) are when called from the sender.

### Independent comparison / falsification test

The writer reads back the quantized values (`sub_141C507D0` and `sub_141C50CF0`) right after quantizing.
That only makes sense if the receiver extrapolates from the same values, so it is consistent with dead
reckoning. Each threshold is a literal constant visible in the code. A live check would log
`sub_141C72630` calls for a walking NPC and expect sends roughly every 50 cm in normal mode.

### Reproduction steps

Decompile `0x141C78190`, `0x141C72630`, `0x141C72820`, `0x141C707D0`, `0x141C4F3C0`, `0x141C676C0`,
`0x141C5B2D0`, `0x140F99FB0` and `0x14012A610`/`0x140128700`. Read the constants at `0x142FFC880`,
`0x1435F2DB0` and `0x1435EDEE0`. Read only.

### Implementation implications and finding-linked constants

- Proxy pool: 30 entries × 10 sub-entries. Prediction error thresholds 0.0025 / 0.25 (squared); distance
  thresholds 6.25 / 16 (squared); minimum prediction age 100 ms.
- Heading threshold 0.175 rad. Position change threshold 0.01. Speed clamp 340. Position clamps ±4000,
  ±3000, ±2000.
- Velocity = unit direction (x, z, sign of y) plus clamped speed. The sender predicts from the quantized
  values.
