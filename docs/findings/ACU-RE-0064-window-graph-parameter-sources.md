# ACU-RE-0064 — Where the window-entry graph parameters come from

Date: 2026-11-14. Related: [ACU-RE-0063](ACU-RE-0063-newdemo-window-selection.md) (the graph
rules that read these parameters), [ACU-RE-0058](ACU-RE-0058-root-delta-extraction.md) (mirror bit),
[ACU-RE-0062](ACU-RE-0062-automated-window-entries.md) (replay).
State: **observed in the binary** (static), except where marked. No live run.

## Provenance

- `ACU.exe` SHA-256 `748abdc6456397fe7d8080f975f2a5ed3123bce548e7bf1c68b23f47e8b27e40`, base
  `0x140000000`.
- Sources:
  - pseudocode `ACU - Decompiled/<VA>.c`;
  - the mapped image `Unity/local/native-probe/motion-link-20261020/runtime-view/capture/main-image.mapped.bin`
    (constants, tables).
- Parameter names: the code passes CRC-32(name) to `sub_14277AA00(table, crc, 0)`, which
  resolves a graph parameter. The names come from NewDemo's `AtomRTCPDescriptor`s (ACU-RE-0063).
  - `Direction` `bcbb5310`, `TargetHeight` `dc434c51`, `TargetDistance` `97a0d819`,
    `TargetWidth` `5b7a8356`;
  - `SyncA_TargetAngle` `1f1ca3dc`, `SyncA_TargetHeight` `03eb5eb2`, `SyncB_TargetHeight`
    `7f8a7b69`, `HeightDifferential` `5842c4cc`.
- Search: byte search of the image for each CRC as a u32 immediate. Each hit is mapped to its
  containing corpus function.
- Classes: two reflected method tables with 32-byte records (fn, 0, u32 name hash, u32 slot).
  Class A's table holds `0x1401B0A50` at slot 0x5e (record `0x143A71B90`); class B's holds
  `0x1401A0AC0` (record `0x143A5BF60`). Their names are not recovered.

## Observations

1. **Target object setup** (`0x1401B1770`). It copies a target descriptor `a4` into the targeting
   object and measures it against the character position `a2`:
   - `+0x30` P = `a4+16`; `+0x40` n = `a4+32`; `+0x180`/`+0x190`/`+0x1A0` = `a4+96/112/128`.
   - `+0xC0` = (P − a2).z, `+0xC4` = |(P − a2).xy|, `+0xC8` = |P − a2|.
   - `+0x90` = normalize(P − a2).
2. **Climb window setter** (class A, slot 0x5e, `0x1401B0A50`) writes:
   - `TargetDistance` = `+0xC4`, `TargetHeight` = `+0xC0`, `TargetWidth` = `+0x290`.
   - `SyncA_TargetAngle` = a signed angle (degrees, ×180/π) built from `+0x40` and `+0x90`
     around z.
   - `Direction` from the mode at `+0x300`:

     | Mode | Direction (written as) | Also written |
     |---|---|---|
     | 0 | 2, direct store | `SyncA_TargetHeight` = `+0xC0` |
     | 1 | 3, via `sub_140152460` | `SyncB_TargetHeight` = `+0xC0` |
     | 2 | 0, via `sub_140152460` | `SyncB_TargetHeight` = `+0xC0` |
     | 3 | 1, via `sub_14018C3B0(…, 1, Direction, 0)` | `SyncB_TargetHeight` = `+0xC0` |

   - `HeightDifferential` = `+0x38 − +0x198` = P.z − Q.z, with Q = descriptor `+112`.
   - So **`SyncB_TargetHeight` = P.z − character.z** in the side and down modes.
3. **Mode choice** (class A, slot 0x4b, `0x1401B1BC0`).
   - `*(a8+224) − 32 ≤ 2` (the character's state code is 32–34) → mode 0.
     The target is set to P and `+0x90` to normalize(P − a2).
   - Otherwise a probe (`sub_14017A8D0`) finds a point R, projected into the plane through P
     with normal ĥ = normalize((n.x, n.y, 0)).
     - If the character's z ≥ R.z → mode 1.
     - Otherwise let s = dot(normalize(`+0x90` − (`+0x90`·ĥ)ĥ), normalize(up × ĥ)), with
       up = (0,0,1) (`0x142E75BC0`) and up × ĥ = (−ĥy, ĥx, 0). If s ≥ 0 → mode 3, else mode 2.
4. **Ground run-in setter** (class B, slot 0x5e, `0x1401A0AC0`) writes:
   - `TargetDistance` `+0xC4`, `TargetHeight` `+0xC0`, `TargetWidth` `+0x290`;
   - `TargetAngle`: a signed angle, built the same way;
   - `TopLedgeDistance`: from a virtual call (`+2896`);
   - `HeightDifferential` = `+0x188 − +0x198`.
5. **Mirror** (`0x1400E1050`, AtomAnimationDataNode slot 21). The node's input port 2 is
   written to bit 0 of `playback+0x37E` (ACU-RE-0058's mirror bit). So the clip-node flag in
   ACU-RE-0063 is the mirror flag. Combined with the table:
   - Direction 1 (mode 3, the target on the counter-clockwise side of ĥ) plays the `Lt` clips
     mirrored;
   - Direction 0 (mode 2) plays them as authored.

## Inferences and alternatives

- **Which side.** Which physical side mode 3 is depends on whether n (descriptor `+32`) points
  out of or into the building; that is not established.
  - Replay check (ACU-RE-0062 obs. 3): Arno was on the window's +row 1 side and moved to his
    left (−row 1) into it, playing an `Lt` clip.
  - If n is the outward normal, then s < 0 → mode 2 → Direction 0 → unmirrored `Lt`. That is a
    leftward clip for a leftward move, which is consistent.
  - If n points inward, the same entry would play mirrored `Lt`, which contradicts the clip name.
  - So **n outward** is the consistent reading (inference).
- **What P and Q are.** P (descriptor +16) and Q (+112) are not identified. Arno landed inside at
  z 5.9965, and `Lt-High` needs P.z − a2.z ∈ [−2.0, −1.25). If P were the interior landing
  point, a2.z would be 7.25–8.0, above the sampled feet z 7.06. Then a2 is not the feet, or P is
  something else.

## Unknowns

- How the target descriptor is built (the parkour target scan), and so what P, Q and n are
  physically.
- What the character position `a2` passed to `0x1401B1770` is (root, hips, hands?).
- The class names of A and B, and of state codes 32–34 (`a8+224`).
- `sub_14017A8D0`'s probe semantics (arguments 5.0 / 2.0, `0x3D4CCCCD` = 0.05, `0x3FC90FDB` =
  π/2).

## Reproduction

Byte-search the image for the CRC immediates. Then open the listed `<VA>.c` files and read the
records at `0x143A71B90` / `0x143A5BF60` (32-byte stride; slot index in the upper u32 of the third
qword).

## Implementation implications

- The window graph inputs can be computed from a target descriptor (P, n, Q) and the
  character's position.
- The remaining question for the rewrite is the descriptor itself. One live read of the
  targeting object during the deterministic replay would give P, n, Q, a2 and the mode at once.
  Statically it means tracing the parkour target scan.
