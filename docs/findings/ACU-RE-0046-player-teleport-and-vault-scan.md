# ACU-RE-0046 — Player teleport (entity matrix + Human reset) and a vault scan

Date: 2026-11-14. Related: [ACU-RE-0044](ACU-RE-0044-player-state-and-repeatable-start.md),
[ACU-RE-0045](ACU-RE-0045-decision-clips-and-window-sweep.md), [ACU-RE-0041](ACU-RE-0041-acufixes-layout-spot-checks.md).
State: observed. Teleport verified live; vault scan results below.

## Provenance

- `ACU.exe` SHA-256 `748abdc6456397fe7d8080f975f2a5ed3123bce548e7bf1c68b23f47e8b27e40`, image base
  `0x140000000`; mapped image SHA-256 `9a1124f1…0520` (ACU-RE-0030) for vtable reads.
- Corpus files (`ACU - Decompiled/`): `140EF73D0.c`, `140EF76D0.c`, `140EE11B0.c`, `140EBB080.c`,
  `140EA52A0.c` (lines 225–305), `140ED6770.c`, `141EC9D00.c`, `141EC9CA0.c`, `14131B5E0.c`,
  `141EC18C0.c`, `14131A530.c`, `1406428D0.c`.
- Lead: ACUFixes `ParkourDebuggingPatch.cpp` (`TeleportPlayer`: write player position, then
  `Human__Dispatch_HSTE_ResetBehavior` `0x14131A530`) and `Human.cpp` (`Human::GetForPlayer`:
  AtomAnimComponent `+0xC58`); clone reviewed in ACU-RE-0041. Facts re-derived below; no code copied.
- Live: run `local/native-probe/runs/teleport-1-launch` (pid 115404, pinned save `capture-start-1`).
- Tools: `tools/native_probe/focus.cpp` (teleport request), `teleport.py`, `vault_scan.py`.

## Observations

1. **Debug teleport.** Debug menu item "Teleport Character" / "Teleport (off)" (`0x140EF73D0`)
   calls `0x140EE11B0` when its toggle byte `+2060` is set (toggle `0x140EF76D0`, menu entry
   "Toggle Teleport Mode" in the debug manager constructor `0x140EA52A0`, vtable `0x142F57850`).
   `0x140EE11B0` builds a matrix with `0x140EBB080` (camera ray cast; entity matrix rows from
   `+0x20`, translation replaced), then `iface = H->vtbl[0xF0](H, 13)`,
   `iface->vtbl[0x28](iface, &m)` (accept check) and `iface->vtbl[0x30](iface, &m)`.
2. **Debug manager wiring.** `0x140ED6770` allocates it (0x860 bytes) and calls vtable `+192`
   with the player's component; `0x141EC9CA0` stores `a1[2]` = component, `a1[3]` =
   `[component + 0x10]` (entity), `a1[4]` = `[component + 0xC8]` (H). Live, a heap scan for the
   vtable found one object: component `0x15F034960` (vtable `0x142F08230`, entity component
   index 35), entity `0x15EEFC400` (= ACU-RE-0044 player), H `0x15F0381F0` (vtable `0x142FB2010`).
3. **H is the Human.** `[AtomAnimComponent (vtable 0x142E7F780) + 0xC58]` read live = the same
   `0x15F0381F0`. Its slot `0xF0` is `0x14131B5E0` (cases 0/5/6 return `this+2448/464/456`, else
   `0x141EC18C0` asks five sub-objects at `this + 40 + 80*i` via their slot `+72`).
4. **Interface 13 is null in retail** (live call through the focus shim, status −2). The debug
   teleport cannot be used as is.
5. **Caution:** the player's three `0x14309C7A0` components also have an ACU.exe object at `+0xC8`
   (vtable `0x142EC2C50`), but its slot `0xF0` is a scalar deleting destructor (`0x1406428D0`).
   Only the object proven by observations 2–3 is called.
6. **Working teleport.** On the game window's thread: copy a 4×4 matrix over the entity world
   matrix (`+0x20`, position row `+0x50`) and call `0x14131A530(H)`. Live: (56.065, −270.651,
   −0.549) → (57.243, −266.303, −0.378), error 0.000 m, still there 3.5 s later; Arno idle and
   upright in the screenshot. The requested yaw (90°) was **not** kept: heading stayed 75.0°,
   the pre-teleport heading. The reset restores orientation from state not overwritten here.

Inferences (not established): `0x14131A530` restarts the Human's behaviour state machine
(`[H+1808]` passed to `0x140BC6240`), which re-reads the entity transform; the per-frame position
overwrite noted in ACU-RE-0044 comes from that state, not from the entity.
Unknown: whether the window thread is the simulation thread; whether long-distance teleports
stream the destination in time (only short distances tested so far).

## Vault scan

The pinned save (`capture-start-1`, the user's only progress save, byte-identical to `1.save`)
is in the Sequence 1 **Versailles** world (`DataPC_ACU_Versailles.forge`; placement stems
`VRS_*`; district banner "Recollets" is the Versailles quarter). No Paris save exists on this
machine, so everything below is Versailles; the `GEN_ALL_*` prop kit is shared with Paris.

Vault clips (asset index, crc-name): `xx_Parkour_Sprint_SkipOver_Fd{010..300}cm-Up{050..200}cm`,
`SlideOver_Fd{100..300}cm-Up{050..200}cm`, `SkipOverFootl/Footr_VsLean…`, `SkipOverVshape…`,
`Walk_SkipOver…`; step-ups `Run_JumpOn{50..250}cm…`, `Idle_JumpOn100cmBlock`.

`tools/native_probe/vault_scan.py` (teleport to a ground origin, one calibration step, RT + A for
3 s in 8 directions; clip sampler and parkour-selection probe throughout):

| Run | Origins / runs | Clip starts | Vault-family starts | Notes |
| --- | --- | --- | --- | --- |
| vault-scan-1 | 5 / 40 | 1,141 | 12 | all `Run_JumpOn50/100cm` (decision type 9) |
| vault-scan-2 | 2 / 16 | 474 | 1 | narrow street; frontier did not grow |
| vault-scan-3 | 18 / 148 (stopped) | 4,267 | 40 | first true vault: `Sprint_SkipOver_Fd010cm-Up100cm` ×2 at (19.96, −260.52), decision type 14 |

vault-scan-3 stopped when enemies killed Arno (user saw it; world reloaded, new player entity
`0x15DE76F30`, respawn at (99.0, −307.7)). Proximity join (inference): step-ups land on market
stalls (`GEN_ALL_Table_02` with cups/dishes, `CratesWood_01`, `Wheelbarrow_01`); the SkipOver
lines up with `GEN_ALL_Rocks_Wallsnap_01`.

## Safety flags for unattended runs

7. **Invisible.** Debug item "Invisible :" (`0x140EF6DD0`, constructor `140EA52A0.c:1378–1384`)
   finds the player component typed by `0x140B9CD00` (descriptor `0x143DB1628`) via the entity's
   component lookup (vtable `+360`; cache at debug manager `+2008`, array `[E+0x78]`, u16 count
   `[E+0x82]`) and calls `0x141EC93F0(component, !byte[+208])`, which calls vtable `+648`
   (`0x140BA29D0`) and stores the byte. Live: the component is index 35, vtable `0x142F08230`,
   whose slot 5 is `0x140B9CD00` (exact type match) — the same object as observation 2.
   Other callers of the setter: `0x1401C2A60`, `0x141F228D0` (value from data records),
   `0x140E6E850`, `0x140E745E0`, `0x141AD7D70`, `0x140ED5AC0` (gameplay uses the same flag).
8. **God Mode.** Debug item `0x140EF7510`: handle = `[0x141CCFAC0(entity) + 184]`,
   `0x141212680(&handle, value)`; state read through `0x140CFBDA0(handle)`.
9. Both set live through the shim (modes 1 and 2), read back 1; re-asserted after every
   teleport. With them on, the Versailles hop scan and the prop probe saw no death and no
   danger clips.

## World entity inventory and hop scan

10. `tools/native_probe/world_entities.py`: heap scan for the World pointer at `+0x60`, accept
    rigid matrices at `+0x20`. Live: ~3.6 GB scanned in 13–16 s; 5,940–8,201 entities within
    200 m; ~300–363 animated (AtomAnimComponent).
11. Identity (hypothesis until joined with offline placements): `[E+0x130]` → handle, `+0x10` =
    the entity's own `Entity` (`0984415e`) file ID; e.g. `GEN_All_MetalFence_01_069` at
    (40.197, −250.353, 2.52). 2,917 of 5,940 entities resolved to a name.
12. `tools/native_probe/hop_scan.py` (frontier of NPC ground points, 75 m cover radius, 150 m
    inventory): Versailles covered in 4 hops, 11,384 unique entities, no danger events.
13. Identity confirmed: ACU-RE-0047 decodes the file IDs read live through `[E+0x130]+0x10`
    (`0000001a5fd4c457` `GEN_All_MetalFence_01_069`, `00000026fb963214`
    `GEN_ALL_CratesWine_01_008`) to the live positions within 0.7 mm.
14. Fall-through guard (user report: after a teleport the ground may not be streamed and Arno
    falls through): `teleport()` re-teleports with doubled settle time, up to 3 times, when Arno
    ends more than 1.5 m below the target; `hop_scan.py` then skips that cell. A 170 m
    Versailles teleport landed on the first attempt.

## Prop probe (Versailles, run `prop-probe-1`)

58 street-level `GEN_ALL_*` obstacle types (by name), two instances each, approached from four
sides (start 6.5 m out at the prop's height, RT + A for 2.5 s): 352 runs, 6,037 clip starts,
83 vault-family starts. `tools/prop_moves.py` groups clips into move families per type.

- Clean vaults: `Sprint_SkipOver_Fd010cm-Up200cm` at (158.8, −237.1) from two directions;
  `Sprint_SkipOver_Fd010cm-Up100cm` (vault-scan-3). Step-ups `Run_JumpOn50/100/140/150/250cm`
  and `LeapFeetPose_JumpOnBlock_Fwd100cm-Up100/140cm` at market stalls and crate stacks.
- **Per-type labels are not reliable yet.** `wall_run_climb` appears in most types because a
  2.5 s sprint also reaches the walls behind the prop; 2–8 runs per type. Time-window
  association cannot separate the prop from its surroundings.
- Next (implementation choice): key labels on the parkour decision's chosen candidate target
  (ACU-RE-0042 records candidate positions) joined to the nearest decoded placement
  (ACU-RE-0047), and stop each run at the first parkour clip.

## Paris

Blocked on data, not tools: the only progress save on this machine is the Versailles one, so
the live Paris hop scan needs a save in Paris. The offline Paris placement map (ACU-RE-0047)
does not need the game.
