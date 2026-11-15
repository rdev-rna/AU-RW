# ACU-RE-0077: How does an ACU peer apply incoming proxy position updates?

Date / investigator / related task: 2026-11-15 / Claude (Opus 5.5), static only / #85, co-op plan Phases 3/4 ([COOP_RE_PLAN.md](../COOP_RE_PLAN.md))
State: partially established. The receiver, the per-entity proxy state, the extrapolation query and three
consumers are observed. Smoothing above the proxy layer and the network clock source are not established.

## Provenance

Binary path / SHA-256 / image base / VA / RVA: same capture and analysis view as ACU-RE-0072 to 0076:
mapped image `9a1124f1…0520`, analysis view `47a81dc9…d8c2`, image base `0x140000000`.
Corpus path and lines / export identity status: not used.
Archive identity / byte range / range hash / file ID (hex string): not applicable.
Stored and decoded lengths / SHA-256: not applicable.
Tool/provider/version / evidence IDs or saved output:
- IDA Pro 9.3, ida-pro-mcp idalib, headless session `acu85`, on the private IDB copy. Read only, closed
  without saving, lock #31 held.
- Capstone 5.0.3, used offline on the mapped image for the heading decode window and the constants.

Exact commands and search boundary: callers of the dequantizers `sub_141C507D0` and `sub_141C50CF0` (exactly
two each: the sender `sub_141C78190` and `sub_141C66C10`); callers of the predictor `sub_141C4F3C0`; 3 of
the 25 callers of `sub_141C4F340`.

## Offsets

ACU-RE-0076 gave sub-entry slot offsets relative to the first slot at sub-entry `+0x78`. In absolute
sub-entry offsets:

| Field | ACU-RE-0076 (relative) | Absolute |
|---|---|---|
| position | `+0x00` | `+0x78` |
| heading | `+0x168` | `+0x1E0` |
| flag | `+0x1A8` | `+0x220` |
| timestamp | `+0x1C8` | `+0x240` |

The entity reference is the u64 at absolute `+0x70`.

## Result

### Observations

1. **Receiver.** `sub_141C66C10` is reached through the thunk `sub_141C66E20`, which passes the global
   `qword_145212870`. It takes a `ProxiesProtocol` entry and the 584-byte sub-entries. For each sub-entry
   whose entity reference (`+0x70`) is not the null sentinel `qword_1452134F8`, it does the following:
   - It gets or creates the per-entity proxy state with `sub_141C4FD70(mgr, entity, 3)`. The state is a
     hash map keyed by entity reference at `mgr[9]`.
   - It continues only if the state's role (`+0x1C`) is not 0 or 2, and if the state's last-processed
     local time (`+0x28`) is older than now. It then sets `+0x28` to now.
   - If the sub-entry's timestamp (`+0x240`) is newer than the stored one (`+0x34`), using a wrap-around
     comparison (`ts > old && ts − old ≤ 0x7FFFFFFF`), it stores:
     - the 5-bit context value at `+0x38`;
     - the dequantized position at `+0x40` and the dequantized velocity at `+0x50`;
     - "has data" = 1 at `+0x70`;
     - the timestamp at `+0x34`.

     Older and duplicate updates are ignored.
   - It always decodes the **heading** and builds a facing direction at `+0x60`:
     - `angle = code · 4.0 / (2^bits − 1)`. The code is the slot's code at `+0x1F8`, the bit width is at
       `+0x200`, and the constant 4.0 is the double at `0x142E74FD8`. The heading range is therefore
       ±4 rad, which covers ±π.
     - `sub_141CF3FE0` rotates the unit vector (0, 1, 0) (`0x1435F22E0`) about the axis (0, 0, 1)
       (`0x142E75BC0`) by that angle.
   - It sets the flag at `+0x71` to (sub-entry byte `+0x220` == 1).
2. **Per-entity proxy state** (128 bytes, created by `sub_141C4FD70`):

   | Offset | Field |
   |---|---|
   | `+0x00` | set by local registration |
   | `+0x08` | entity reference |
   | `+0x10` | link to the local component |
   | `+0x1C` | role (3 = created by the receiver, 4 = registered locally; `sub_141C75AD0` stores 1 when the component's `+0x38` is 1) |
   | `+0x20` | reference count |
   | `+0x28` | last processed (local clock) |
   | `+0x30` | precision flag (the same `+0x30` the sender reads) |
   | `+0x34` | timestamp of the last accepted update |
   | `+0x38` | context, initially 0xFF |
   | `+0x40` | position |
   | `+0x50` | velocity |
   | `+0x60` | facing vector, initially (1, 0, 0, 0) |
   | `+0x70` | has data |
   | `+0x71` | flag |

3. **Extrapolation query.** `sub_141C4F340(state, out, scale)` returns
   `position + velocity · (now − timestamp)·scale·0.001`, through the same predictor `sub_141C4F3C0` the
   sender uses.
   - `now` comes from `sub_141F89950`, the same clock that stamps updates. If `now` is earlier than the
     timestamp, the elapsed time is 0.
   - For roles 1 and 3, the squared step is capped at 16.0 (4 m) when the precision flag is set, otherwise
     at 48.0 (about 6.9 m). Other roles have no cap.
   - z is clamped to ±2000.
4. **Clock.** `sub_141F89950` returns the result of virtual `+0x118` of the object at `qword_145212F48 + 0x20`
   when that object reports ready. Otherwise it falls back to a local clock plus the offset in
   `qword_14521E148`. Both peers compute `now − timestamp` with this function on values stamped by
   another peer, so the result is only meaningful if the clock is shared across peers.
5. **Consumers read the prediction directly.**
   - `sub_141EF7300` is a position getter. When the entity's flags (`+0xD4`) masked with `0xC0000` equal
     `0x40000`, it returns the world-matrix translation. Otherwise, if the proxy state has data, it returns
     `sub_141C4F340`'s prediction.
   - `sub_141AAB0E0`: when the flags masked with `0xC0000` equal `0x80000` and the proxy has data, it writes
     the prediction into the entity transform (`sub_1420F7540`) and the facing vector (`sub_1420F7A20`). No
     blending is done in this function.
   - `sub_1410F4630`: when `+0x71` is set, it takes a direct path (`sub_142076C30`). Otherwise it tests the
     prediction and facing through `sub_1410F4280`.

### Inferences and alternatives

- **The receiver applies the extrapolated position as is.** It keeps only the newest update and recomputes
  the position each time it is queried. In the consumers read, the transform is set straight from the
  prediction, so a correction appears as a jump of at most the sender's error threshold (5 cm precise,
  50 cm normal). Smoothing may still happen in animation or locomotion code above this layer; that code
  has not been read.
- **`sub_141F89950` is probably the Storm synchronized network time** (ACU-RE-0073 lists a Storm time
  module), because the timestamps cross peers. The fallback branch would cover single-player. Our
  transport must provide a shared millisecond clock.
- **Entity flag bits `0xC0000`:** `0x40000` probably marks an entity simulated locally, and `0x80000` an
  entity driven by a proxy.
- **Axes.** The decoded heading is a rotation of +y about +z, and the sender takes heading from matrix
  row 1. Together, this shows that the proxy layer treats z as up and local +y as forward. World units
  remain unverified.

### Unknowns / incomplete coverage

- What `sub_141F89950`'s provider object is, and how it is synchronized.
- The `scale` argument each caller passes to `sub_141C4F340` (22 callers not read).
- Whether locomotion or animation smooths the jump after a correction.
- The meaning of role values 1 to 4 and of the `+0x71` flag.
- The bit widths (still set outside the codecs).

### Independent comparison / falsification test

The receiver dequantizes with the same two helpers the sender uses for read-back (ACU-RE-0076
observation 6), and extrapolates with the same predictor. A receiver that blended toward a target, or that
used its own predictor, would contradict ACU-RE-0076's inference that both sides extrapolate identically;
none was found. A live check: on a replica, read the state's `+0x40` and `+0x50` against the entity
transform each frame and look for jumps at update times.

### Reproduction steps

1. Decompile `0x141C66C10`, `0x141C4FD70`, `0x141C75AD0`, `0x141C4F340`, `0x141C4F3C0`, `0x141CF3FE0`,
   `0x141EF7300`, `0x141AAB0E0` and `0x1410F4630`.
2. Disassemble `0x141C66D48` to `0x141C66DB4` and `0x141F89950`.
3. Read the constants at `0x142E74FD8`, `0x1435F22E0`, `0x142E75BC0` and `0x142E77FE0`. Read only.

### Implementation implications and finding-linked constants

- Receiver: keep the newest update per entity (wrap-around u32 timestamp). Store the dequantized position
  and velocity. Each query returns `p + v·(now − ts)·0.001`, with the squared step capped at 16 or 48 and
  z clamped to ±2000. Set the replica's transform directly from it.
- Heading: `angle = code·4/(2^bits − 1)`. Facing = +y rotated about +z by that angle.
- The transport must give peers a shared millisecond clock.
