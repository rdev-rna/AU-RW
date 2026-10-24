# ACU-RE-0039 — Map, texture, mesh and LOD streaming: inventory from the runtime dump

Status: in progress. Every claim below is from `local/dump/ACU_dump_raw.i64`
(md5 `d333718cc428c8dbf10c3fddf90c1947`, sha256 `4a6f7026…c0d`, imagebase `0x0`,
image size `0x7825000`, 215,244 functions, 90,641 strings, single segment `seg000`).

Method: strings → xrefs → decompile. The dump retains engine symbol strings and
frame-task names, so the subsystems can be found by name instead of guessed.

## Verified anchors

| Anchor | Address | Evidence |
|---|---|---|
| Cell reader (9-byte form) | `sub_1DEF490` | guards `cursor+9 > end`, then `sub_270FF90(ctx, …, -1406419096 /*0xAC2BBF68*/, …, 1156174592, …)` |
| Cell reader (18-byte form) | `sub_1DEEBB0` | guards `cursor+18`, reads `0x4180B964` then `0xAC2BBF68` |
| Generic typed-field reader | `sub_270FF90` | called by both cell readers with a type id immediate |
| Type descriptor table | `0x4e9c800` región; `0xAC2BBF68` at `0x4e9d308` and `0x4e9d340` | entries carry type id + size (`0x12`, `0x1c`) + alignment; no static xref (looked up by hash at runtime) |
| Frame task registration | `sub_21A3930` | registers `Graphic::PreUpdateNodes` (`sub_219F4D0`), `Graphic::UpdateNodes` (`sub_21E75B0`), `Graphic::PostUpdateNodes` (`sub_219F450`), `Graphic::EndGraphicTasks`, `Graphic::GlobalUpdates` (`sub_219BA80`) |
| Texture streaming stage | task name `Graphic::TextureStreamingManager` → `sub_21A8AC0` | `sub_21A3930` |
| Texture streaming dispatch | `sub_21A8AC0` | asm: `mov rcx, cs:qword_523C6D0; test rcx,rcx; jz; mov rax,[rcx]; jmp qword ptr [rax+0C0h]` — a virtual call through the manager singleton |
| Manager singleton | `qword_523C6D0` | holds `0xEBD97260`, a heap pointer **outside** the dumped image (`0x0–0x7825000`), so the object's vtable and fields are not in the dump |
| Manager shutdown | `sub_220DCD0` | `qword_523C6D0 = 0` after `sub_1D0A810(...)`; one of 8 xrefs to the singleton |
| `GridCellDataBlock` | string `0x302e412` | RTTI/name string |
| `CompiledMeshInstanceStream` | string `0x3095daa` | RTTI/name string |
| `MeshInstanceData` / `SubMeshInstanceData` / `MeshInstanceMaterialInfo` | `0x3091b10` / `0x3091be1` / `0x3091f98` | RTTI/name strings |
| `Graphic::TextureStreamingManager` | string `0x30971f0` | task/class name |
| `UpdateTextureStreamingManager` | string `0x30971d0`, xref `0x21a3be8` | inside `sub_21A3930` |

## Open, with the exact blocker

1. **Texture streaming policy.** `sub_21A8AC0` calls through `MEMORY[0xEBD97260] + 192`.
   That global is outside the dumped image (image ends at `0x7825000`), so the target
   cannot be read statically. Resolve by finding the registration site that writes
   `0xEBD97260`, or by a live probe of the vtable.
2. **Descriptor table layout.** Entries near `0x4e9c800` mix 64-bit pointers
   (`0x144EA3810`) with type-id/size records. Stride and field meaning are not yet
   established; the table is hashed at runtime so there is no static xref to the base.
3. **Mesh instance stream.** `CompiledMeshInstanceStream` is only a name so far; its
   producer/consumer functions are not yet located.

## What this changes for the rewrite

- The original renderer is built on **compiled mesh instances**: pre-baked instance
  streams per mesh, not per-placement entities. Our instanced renderer
  (`instanced.rs`, one entity per (piece, part) with an instance matrix buffer) is the
  same model, now confirmed against the binary rather than inferred.
- **Texture streaming is an explicit frame stage with its own manager**, scheduled
  between `PostUpdateNodes` and `EndGraphicTasks`. Our loader uploads every texture up
  front, which is exactly why the whole-map run exhausted an 8 GB card while the
  engine does not.
- **LOD selection is per placement** (`51dc6b80` record order = level, `+22` = switch
  distance). Our loader still applies one global `--map-lod`.

## Mesh vertex layout (new)

| Anchor | Address | Evidence |
|---|---|---|
| Mesh type reader (thin) | `sub_21CFC10` | reads type `1096652136` (`0x41603B68`) then `-2050484285` (`0x85C0A2C3`) with a 9-byte guard — same shape as the cell readers |
| Mesh vertex-stream reader | `sub_1E176C0` | reads flag bits into `a4+112`, then **three** element arrays: base `a4+64` count `u16@a4+74`, base `a4+76` count `u16@a4+86`, base `a4+88` count `u16@a4+98`; each element is **8 bytes** (`v22 += 8`) |
| Ruled out | `sub_2A87740` | references `Cannot convert vertex format from` — this is **Havok** `hkMeshVertexBufferUtil.cpp`, not the Anvil mesh decoder; our `format 2 stride 40` is not in this table |

Three 8-byte element streams are consistent with a per-attribute declaration
(semantic + format per element) rather than a fixed 40-byte interleaved vertex.

## Build pin

| Item | Value |
|---|---|
| `ACU.exe` | 30,907,608 bytes, SHA-256 `748ABDC6456397FE7D8080F975F2A5ED3123BCE548E7BF1C68B23F47E8B27E40` |
| Version resource | none — the exe is packed/stripped; registry has only `installdir`, `Language`, `SKU` |
| Patch archives present | `DataPC_ACU_Paris_assets_patch_01/02/03.forge`, `DataPC_ACU_Paris_gi_patch_01.forge` (consistent with 1.5.0; unconfirmed until read off the menu) |
| Dump (the actual pin for every address) | `ACU_dump_raw.i64`, md5 `d333718cc428c8dbf10c3fddf90c1947`, imagebase `0x0`, size `0x7825000` |

## Live probe: blocked

Launching `ACU.exe` spawns Ubisoft Connect, which demands a password
re-entry ("Please re-enter your Ubisoft account password to play this game").
No probing is possible without a signed-in session. Deliberately **not** worked
around: bypassing the launcher's authentication is DRM circumvention, which is
out of scope regardless of tooling.

To unblock: sign in to Ubisoft Connect once, load any save into the world, and
leave the game running. Then the probe is three reads —
`qword[base+0x523C6D0]` -> manager, `qword[manager]` -> vtable,
`qword[vtable+0xC0]` -> the texture-streaming update. That last value minus the
module base is an RVA into the IDB, which turns the policy into a decompile.

## LOD selection (measured, semantics partly open)

`sub_43360` (1,784 B) walks a component collection and, for type id `1373399936`
(`0x51DC6B80`), picks a sub-level per record:

```
sq = *(float*)(record + 8); sq = sq * sq;
v = 5; while (1) { t = dword_4E7E678[v]; if (sq < t) break; ... if (--v < 1) skip; }
choose(v);
```

`dword_4E7E678` (f32): `FLT_MAX, FLT_MAX, 400.0, 144.0, 64.0, 16.0`
=> cut-offs at distances **4, 8, 12, 20 m** (squared 16/64/144/400), band 5 nearest,
band 1 beyond 20 m.

Open: these bands are much tighter than the 20–320 m switch distances recorded in
the `51dc6b80` selector records (RE-0037), so this table is probably a different
band set (per-class or per-detail pass) rather than the world LOD ladder. The
record here is 12 bytes with the distance at `+8`, not the 35-byte record described
in RE-0037 — the two must not be conflated.

## Texture streaming policy — FOUND (live probe, 2026-10-24)

Resolved through the running game (pid 7080, module base `0x140000000`):

| Step | Value |
|---|---|
| `qword[base+0x523C6D0]` | manager object `0x6CAE6F00` |
| `qword[manager]` | vtable `0x14309F1E0` → **RVA `0x309F1E0`** |
| `qword[vtable+0xC0]` | **RVA `0x2222780`** — the streaming update |

`sub_2222780` (130 lines) is the policy. It is **not** a "load everything" path:

1. Accumulates elapsed time across frames (`+0x40`, `+0x48`), converts it with `% 0x2710`
   (10 000 units) and compares the result against a float at `+0x18` (live value **10.0**).
2. When the slice is exceeded it runs three passes: `sub_21F6B10`, `sub_22277B0`,
   `sub_2227640`, then resets the accumulators.
3. Walks a request array (`u16` count at `+0x2C`) and **stops as soon as
   `dword[+0x118] >= dword[+0x1C]`** — a used-vs-budget comparison. Live object also
   holds `2000.0` at `+0x18` of the manager and `137.9` at `+0x00`.
4. Each request is dispatched as an async job:
   `sub_26E4C80(…, 3, &qword_50A9600, sub_221B490, …)`, i.e. texture loads are
   queued to a job system, not performed inline.

**Meaning for the rewrite:** the engine never uploads the whole map's textures in one
go. It keeps a request queue, spends a bounded time slice per frame, and hard-stops
at a per-frame item budget. Our loader uploads everything up front — that, not the
GPU, is why the whole-map textured render died on 8 GB.

Replication target in Bevy: a residency queue with (a) a per-frame time slice and
(b) a per-frame item cap, uploading the highest-priority mips first, with the
remaining textures falling back to a lower mip until their turn arrives.

## Registry record layout (partial)

Records carrying type ids sit at `0x4e8c000`, `0x4eac7a8`, `0x4eac7e0`
(`0x85c817c3`), `0x4f559b4` and look like:

```
u32 0x02000000 | u32 <parent?> | u32 <type id> | u16 0 | u16 0x12 | u32 <n>
```

The `0x12` field is 18 and matches the cell reader's 18-byte guard, so it is very
likely the fixed size, but the meaning of the leading `0x02000000` and the
trailing dword is **not established**. No static xref exists to any entry; the
lookup is by hash at runtime.

## Next measurements, in order

1. Follow `Graphic::TextureStreamingManager`: find what writes `MEMORY[0xEBD97260]`,
   then decompile the virtual target at vtable `+192` for the residency/budget policy.
2. Decode the descriptor table stride at `0x4e9c800` and enumerate every type id with
   its size and alignment; that yields the complete serialised vocabulary.
3. Locate the `CompiledMeshInstanceStream` producer/consumer and compare it with
   `LoadedPiece`/`MapInstance`.
4. Only then change the Bevy side: per-instance LOD by distance, and a texture
   residency budget mirroring the manager's rule.
