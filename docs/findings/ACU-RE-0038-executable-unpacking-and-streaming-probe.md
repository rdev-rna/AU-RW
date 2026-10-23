# ACU-RE-0038 — Unpacking the executable, the cell reader, and the streaming probe

Date: 2026-10-23
Game: Assassin's Creed Unity (PC, installed build)
Status: partial — static anchors found, streaming scheduler not yet identified

## Why

Issue #137 needs the map to be complete ("1:1"). Our streaming order, radius and
throttle were invented; the engine's behaviour had never been reverse engineered. This
note records what the engine binary actually contains and how far the trace got.

## ACU.exe is packed

Static open of `ACU.exe` (120 MB image, sha256 `748abdc6…`):

    total_functions: 68        total_strings: 88
    segments: .UBX0 (rwx, 5.1 MB), .UBX1 (rwx, 8.7 MB + 21.3 MB)

Ubisoft's UBX protection: the real code is decrypted in memory. Static analysis of the
file on disk is useless.

## Runtime dump

`tools/dump-image.ps1` reads the live process with `VirtualQueryEx` + `ReadProcessMemory`
and writes a flat image at the module base (`0x140000000`, size `0x7825000`).

    local\dump\ACU_dump.bin      120 MB   (taken ~45 s after launch)
    local\dump\ACU_dump2.bin     120 MB   (taken later, game running)

Loaded into IDA as a raw binary (base 0, i.e. offset = VA − 0x140000000):

    total_functions: 215,241     total_strings: 90,641
    saved IDB: local\dump\ACU_dump_raw.i64

Auto-analysis cost for 120 MB: ~3–4 minutes. Reopen is instant.

**Address arithmetic trap:** IDA offsets are relative to the dump, so a function at
offset `0x1DEF490` is at VA `0x141DEF490` (base + offset), not `0x1401DEF490`. The first
breakpoint attempt failed to read memory purely because of this.

## Anchors found

### Cell block reader

Searching for the resource type id we already know (`ac2bbf68`) as a 32-bit immediate:

    0xAC2BBF68 at 0x1DEEB(9)/0x1DEF4(1), plus copies in .data

    sub_1DEF490:  if (cursor + 9  > end) bounds_fail();  read type 0xAC2BBF68
    sub_1DEEBB0:  if (cursor + 18 > end) bounds_fail();  read 0x4180_0004, then 0xAC2BBF68

The type descriptor at `0x4E9D308` is `ac2bbf68 00120000` — size `0x12` = 18, matching
the bounds check in `sub_1DEEBB0`. Two independent artifacts agree, so this is the
engine's `GridCellDataBlock` reader, not a lookalike.

Both readers report **0 callers** in the IDB, and scanning the module image and the whole
live address space for pointers to `0x1401DEF490` / `0x1401DEEBB0` returns **0 hits**.
The dispatch table is built at runtime; there is no static call edge.

### Content loading thread

    "ContentLoadingThread" (0x3020570)
      xref -> sub_1D29090  (thread start; object +136, spin flag +604, ready +609)
      caller -> sub_1488B70 (singleton check on qword_3F5F460)
      caller -> sub_9058C0

## Dynamic probes

`tools/trace-call.ps1` — attaches with `DebugActiveProcess`, patches `INT3`, on hit dumps
the stack (return addresses inside `0x140001000..0x14779B000`), restores the byte, single-
steps, re-arms, then detaches. `tools/scan-memory.ps1` scans all committed regions of a
live process for a byte pattern.

**Bug found and fixed:** an initial breakpoint is raised in ntdll at attach. Forwarding it
with `DBG_EXCEPTION_NOT_HANDLED` killed the game. All non-target breakpoints must be
answered with `DBG_CONTINUE`.

Results:

| probe | outcome |
|---|---|
| `0x141DEF490` (cell reader), 3 min while walking | **no hits** — not on the live streaming path |
| `CreateFileW` (game kernel32), 2.5 min while walking, filtered to `.forge` | **no hits** — forges are opened once at load |

So content is streamed through already-open forge handles; the scheduler is a caller of
`ReadFile`, not of file opening. That is the next probe (map handle → path from
`CreateFileW` at load, then break `ReadFile` and filter by handle).

## What this settles, and what it does not

- Settled: the binary is packed; a live dump gives a complete, analysable image; the
  engine's cell-block reader is identified by type id and bounds-check size.
- Not settled: the streaming scheduler, its prefetch radius, its LOD policy and its
  cadence. Our 150 m ring, 40 m re-stream and 0.3 s throttle remain our own policy, **not**
  the game's behaviour.
- The engine was **not** observed writing a decoded cache. Whether it has one is unknown.

## Cache work in this branch (independent of the RE above)

`crates/acu_bevy/src/cache.rs`, format v2:

    --cache-write PATH   dump the resolved scene after decoding
    --cache-read  PATH   rebuild it without opening a forge

Stores pieces (vertices, triangles), instances, materials and textures, plus the trailing
rig-piece count (recomputing it from the command line gave a different split and indexed
past the mesh table: `len 161, index 161`).

Verified on cell 9566: 24.8 MB, identical native bounds on read
(`799.5..838.1 / -100.2..-60.6 / -0.45..13.6`), **1.6 s read vs 5 s decode**.

Whole-map dump was started but not completed: per-cell cost is wildly non-uniform (the
first cell alone took 86 s and produced 11,214 pieces), so a linear ETA from the start of
the list is meaningless. Progress reporting was added (`map: N/M cells, rate, ETA`).

## Tools added

- `tools/dump-image.ps1` — flat dump of a live module (repeatable)
- `tools/scan-memory.ps1` — pattern scan across a live process's committed memory
- `tools/trace-call.ps1` — debugger breakpoint + stack capture (+ UTF-16 filename for
  `CreateFileW`), with the initial-breakpoint fix

## Next

1. `ReadFile` probe with a handle → path map, filtered to `.forge`, capturing stack,
   byte count and offset. That yields the scheduler, its read sizes and cadence.
2. Feed the captured return addresses back into `ACU_dump_raw.i64` and decompile the
   manager.
3. Only then decide whether to mirror the engine's policy or keep our own (cache +
   streaming).
