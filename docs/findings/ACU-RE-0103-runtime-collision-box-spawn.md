# ACU-RE-0103 — Spawning collision boxes and moving parkour props at runtime

Date: 2026-11-16. Related: [ACU-RE-0046](ACU-RE-0046-player-teleport-and-vault-scan.md) (teleport
shim), [ACU-RE-0074](ACU-RE-0074-transition-sync-source-and-graph-conditions.md) obs. 12 (the flat
sprint this floor made possible), [ACU-RE-0057](ACU-RE-0057-body-velocity-from-root-motion.md) (root delta).

State: **observed** for the spawn sequence, and live for collision with the player.
- **Observed:**
  - a box built this way holds Arno up, and outside it he falls;
  - several boxes spawn per launch, and Arno braces against them;
  - none gives a vault, climb or ledge (obs. 9–11);
  - world props moved into the sky keep their parkour (obs. 12–13), but not their rendering
    (obs. 14);
  - the entity clone faults on the window thread without an engine memory frame (obs. 15);
  - with the frame opened, as the game does, cloned crates and beams render and keep their
    parkour (obs. 16).
- **Not established:**
  - what the RigidBody motion field values mean;
  - the collision filter layer;
  - what other systems (AI, navigation, cameras, rendering) make of the entity;
  - removal.

## Provenance

- `ACU.exe` SHA-256 `748abdc6456397fe7d8080f975f2a5ed3123bce548e7bf1c68b23f47e8b27e40`, image base
  `0x140000000`.
- Corpus files: `ACU - Decompiled/<VA>.c`. The export-to-binary correspondence is unverified, but
  every RVA below was called live and behaved as read.
- Class layouts come from the reflection symbol index (`tools/symbol_query.py`, read-only):
  - `fields BoxShape`, `fields RigidBody`, `fields InertComponent`;
  - `methods InertComponent`;
  - `callers 0x141FAFDE0 / 0x1425C3600 / 0x1425DDAF0 / 0x1425EA110`.
- Shim: `tools/native_probe/focus.cpp` mode 8, `runSpawnBox`. The DLL SHA-256 is
  `c33fcf4f4e56d8843fc6db47c2e42dbe36dbe4117543816bb4512b6930314b07` (local
  `acu_focus.dll` = `acu_focus_floor.dll`).
- Live runs, local and ignored:
  - `local/native-probe/runs/floor-spawn-1` (launch);
  - `local/native-probe/runs/floor-sprint-1` (capture).
  - PID 125172, fps cap 60, save pin `capture-start-1`. Closed with `--close` (2.8 s); saves put
    back.

## Result

### Observations (pseudocode)

1. **Generic spawn helper** `sub_141FAFDE0(component, world)`:
   - allocates 368 B (align 16);
   - `sub_141CB9AA0` constructs the entity and `sub_141CE9100` initialises it;
   - `sub_141CC00A0(entity, component)` attaches the component;
   - `sub_141D40250(world, entity)` adds it to the World (`sub_141CBEC30`, layer from entity
     +228);
   - then calls entity vtbl+248 with `(entity, 0)`.
   - Its caller `sub_141FAFE80` gets the World from `sub_141CD0D30` (`[[0x145217368]+0x98]`), then
     sets the matrix with `sub_1420F7510(entity, matrix)`. That calls `sub_1420F6F70` when the
     entity has a parent (+0x18), else vtbl+128.
2. **Collision entity built by `PhysicsWorldComponent::vf16` (`0x142613F80`).**
   - An InertComponent is allocated (304 B, align 16) and constructed by `sub_1425EA110`.
   - Its RigidBody is embedded at +0x50 (`sub_1425C42E0`).
   - Its ObjectShape is set by `sub_1425DEEC0(component + 0x50, &handle)`, which exchanges the
     handle at RigidBody +0x50 and takes a reference at handle +12.
   - The InertComponent constructor ends with `sub_1425DD9F0(rigidBody, 0)`, which sets RigidBody
     +0x58 to 0. The RigidBody constructor default is 2.
3. **BoxShape** (128 B), constructed by `sub_1425C3600`, vtable `0x14361ECF0`.
   - Reflected fields:
     - +0x20: vec4, hash `A4041396`, the half extents;
     - +0x30: `Matrix`;
     - `Center`: an accessor-only property;
     - +0x70: `Material`.
   - The constructor ends with vf21 `sub_1425E1910`, which builds the engine shape with
     `sub_142B53CE0(&extents, …, matrix)` and stores it at +0x18.
   - The extents setter `sub_1425DDAF0` clamps each axis to at least 0.01, stores the value at
     +0x20 and calls vtbl+168 to rebuild.
   - `sub_141DA0D30` makes a runtime box the same way: construct, set extents, then
     `sub_1426EB640(0x14525BB70, 0, box)` creates the handle.
4. **Allocation:** `sub_1426D9A10(size, align, sub_1426F3760(size, align, [0x1452585D0]))`, as all
   the functions above do.

### Observations (live, 2026-11-16)

5. **Spawn.**
   - Mode 8 ran the sequence below inside SEH on the game window thread:
     1. box → extents → handle;
     2. InertComponent → set shape;
     3. entity → write matrix (+0x20) → attach → World add → activate → `sub_1420F7510`.
   - It used half extents (100, 100, 0.5) at centre (605.7, −28.9, 60), next to Arno at z −0.28.
   - Status 1, no exception. The vtable checks passed: BoxShape `0x14361ECF0`, InertComponent
     `0x1436238F0`.
   - RigidBody +0x98 / +0xA0 read 0 after activation, so those fields are not body pointers.
6. **On the box.** The ACU-RE-0046 teleport put Arno at z 61.5. Within 0.5 s he was at z 60.496,
   0.004 m below the top face at 60.5, and he stayed there for 6 s.
7. **Sprint on the box.** A 5 s sprint (`root_delta_capture.py --free-run`) covered 27 m. Every
   live sample had z = 60.496.
8. **Control off the box.** A teleport to (905.7, −28.9, 61.5), 200 m outside the box, held z 61.497
   for 1.5 s while the behaviour reset. By +3 s Arno was at z 18.1, having fallen to something
   below.

### Observations (live, parkour on spawned boxes, 2026-11-16)

9. **Several spawns per launch.** Run `parkour-box-1` (PID 110412, same launch path and pin, closed
   with `--close`) spawned four boxes, all with status 1:
   - the floor, as in obs. 5;
   - a 1 m wall: half (2, 0.3, 0.5) at (605.7, −20.9, 61.0);
   - a 2 m block: half (2, 1.5, 1.0) at (617.7, −20.9, 61.5);
   - a 5 m wall: half (2, 1.0, 2.5) at (629.7, −20.9, 63.0).
10. **Free run with parkour up into each obstacle.** Arno was teleported 8 m in front of each
    obstacle, facing it. `controller_autoplay.py` then held the stick forward with RT + A for 5 s
    (moves file `local/native-probe/parkour-box-approach-moves.json`), with `live_state.py` at
    60 Hz. Clip names come from the asset index (`crc-name`, DataPC.forge entry 928):

    | Obstacle | Clips after the run (`0000000f9f512072` `Run_3-Loop`) | Outcome |
    | --- | --- | --- |
    | 1 m wall | `xx_Nav_SprintFwd_tr_WallBraceLowObject` (`0000002d6c52de55`), then `xx_Nav_WallBraceIdle_NoHands` | stops 0.35 m from the face; z 60.496 throughout |
    | 2 m block | `xx_Nav_SprintFwd_tr_WallBrace_NoHand` (`0000002e3c6ef4fa`), then wall-brace idles | stops 0.40 m from the face; z 60.496 throughout |
    | 5 m wall | `xx_Parkour_Run_PreWallEntryFront_Fd050cm/Fd100cm` (`0000000fda0491dd/e8`), then `xx_Parkour_WallEntry_Fail_Fd0Deg` (`0000001081724057`), then `xx_Parkour_LeapingToVoidFwd_HitWall_Hurt`, then landing; repeated once | rises to z 62.22 and falls back |

11. **No vault, climb or mantle on any box.** Arno never stood on the 1 m wall or the 2 m block,
    and never got higher than 1.7 m up the 5 m wall.

### Observations (live, moving and cloning world props, 2026-11-16)

Runs `clone-course-1` (PID 64800) and `clone-course-2` (PID 67828). Both used the test shim
`acu_focus_floor.dll` (modes 9 and 10, SHA-256 of the final build in the README), the same pin
and fps cap 60, and were closed with `--close`. Props were picked by `world_entities.py` within
150 m of the start. Names are `crc-name` labels from the asset index.

12. **Moving an entity (mode 9).** `0x1420F7510(entity, matrix)` reaches `Entity::set_world_matrix`
    `0x141CE1FF0`. That stores the matrix at +0x20 and calls each component's vtbl+560;
    InertComponent's is `0x14262CBF0` → `0x1425DE420`, which acts only when RigidBody +168 is not
    0x7FFFFFFF (a registered body). Every move returned status 1, and the entity matrix read back
    as requested.
13. **Parkour follows a moved prop.** Each prop was moved onto the sky floor 8 m in front of Arno,
    then he free-ran with parkour up into it:

    | Prop (moved from) | Result |
    | --- | --- |
    | `GEN_ALL_CratesWine_01_002` (601.0, −35.5, 0.0) | `xx_Parkour_Run_JumpOn100cm_tr_Sprint_WithHands` onto the crate (z 61.65), `Leaping_Fd650cm-Dn200cm` off it, `LandingFwd_LandMid` |
    | `GEN_ALL_Beam_2m_01_232` (618.5, 1.1, 4.7), 1.0 m above the floor | `xx_Parkour_Run_JumpOn100cm_tr_Sprint`, `StepLevel_300cm` (z 61.50), `SprintingStepDown_Fd150cm-Dn100cm`, `LeapLevel_tr_Sprint` |
    | `GEN_ALL_PlankStack_01_031` | run over as a low slope (`Run_SlopeDn30Deg`, z ≤ 60.68) |
    | `GEN_ALL_WindowLedge_2m_01_100`, 1.8 m up on the face of a spawned 2 × 1 × 5 m wall, facing Arno or facing the wall | `SprintFwd_tr_WallBrace_NoHand` at the wall; no grab |
    | `GEN_ALL_AnchorClimbing_01_147`, 1.5 m up on a spawned 5 m wall | the wall entry fails, as on a bare box (obs. 10) |

    The stick moves Arno relative to the camera, and the camera keeps its last heading across
    teleports. Lanes whose approach did not match the camera's heading were discarded:
    `clone-course-2` lanes A, B, D, E and F.
14. **Rendering does not follow a move.** A screenshot (`clone-course-1/shot-crate`) taken 5 m
    behind the moved crate shows Arno on the invisible floor with no crate in front of him,
    although he had just jumped onto it.
15. **Cloning (mode 10) faults.** `Entity::vf3` (`0x141CC65D0`, the object clone EntityBuilder uses
    in `0x141CD0470`) was called on a live crate, a plank stack, an anchor and a barrel, with and
    without the builder flags. Every call raised `0xC0000005`, caught by the shim. Fault record:
    - at RVA `0x26F4AED`, in `sub_1426F4A80`;
    - accessed address `0x16865D23C08`;
    - in-image stack `0x1CC663B` (`Entity::vf3` + 0x6B), then `0x1CDC150`, `0x1CE6A42`,
      `0x1C49F1A`, `0x276AE60` (the game's window-message dispatch).

    `sub_1426F4A80` reads the thread's engine memory context: TLS slot + 104, frames of 360 B
    indexed by +2880, and a stack at +2896 with its count at +2906. The game stayed responsive
    after each fault.
16. **Cloning with an engine memory frame (`clone-course-3`, PID 11660).**
    - Cause of the fault. Disassembly of `0x1426F4AED` (mapped image, Capstone) is
      `cmp byte ptr [rcx+0x150], 0`, with `rcx = ctx + 0x168 * [ctx+0xB40]`. The context
      constructor `sub_1426E0560` sets that frame index (+2880) to −1 (line 26). On the window
      thread no frame was open, so the address ran past the context.
    - The engine's frame pair:
      - `sub_1426E6F20(a1, a2)` increments +2880 and resets that frame (flags at +336 = a1, +337 =
        a2);
      - `sub_1426EEC40()` flushes (`sub_1426ED300`, `sub_1426ECF90`), resets the frame and
        decrements +2880.
      - `BreakableCompoundComponent::helper_140DB4490` wraps its runtime clones in
        `sub_1426E6F20(1, 0)` / `(*obj + 24)(obj, 0, 0)` / `sub_1426EEC40()` (lines 456–461 and
        579–581).
    - Mode 10 now opens the frame with (1, 0) around `Entity::vf3`, closing it in `__finally`. The
      shim's SHA-256 is `af54d7f0d9f4ce3bcb536bdd358ff76bd2833853acde574f11335c02215d5478`.
    - Results:

      | Clone of | Placed | Rendered (screenshot from 8 m) | Parkour on the copy |
      | --- | --- | --- | --- |
      | `GEN_ALL_CratesWine_01_002` | sky floor, 8 m along the camera heading | yes: the crate stack floats ahead of Arno | `Run_JumpOn100cm_tr_Sprint_WithHands` (z 61.65), `Leaping_Fd650cm-Dn200cm`, `LandingFwd_LandMid` |
      | `GEN_ALL_Beam_2m_01_232` | 1.0 m above the floor | yes: the beam (and the crate copy, behind) | `Run_JumpOn100cm_tr_Sprint`, `StepLevel_300cm` (z 61.50), `SprintingStepDown`, `LeapLevel_tr_Sprint` |

      Both used the builder flags (+136 |= 0x800014, `0x141CE94F0`, +212 bit 0x40000). The
      originals stayed in place.
17. **A sky course of cloned prop types (`prop-course-1`, PID 130616, default shim `af54d7f0…`).**
    - First pass, floor at 60.5 m: clone sources found at the start returned −1 once Arno was
      lanes away. Streaming had unloaded them: the entity no longer pointed at the current World.
      Lanes near Notre-Dame also ran inside its towers.
    - Second pass:
      - floor at 120.5 m;
      - Arno back at the ground start (4 s for streaming), then a fresh scan;
      - all 15 clones made at once, laid out along the start camera heading; each lane then
        started 8 m before its first prop along the live camera heading.
      - All 15 clones returned status 1.
    - Results:

      | Lane | Source (nearest of its stem) | Parkour |
      | --- | --- | --- |
      | three-beam chain, 3.5 m apart, 1.0 m up | `GEN_ALL_Beam_2m_01_232` ×3 | `JumpOn100cm`, `StepLevel_300cm`, `Leaping_Fd400cm-Level`, `StepLevel`, `Leaping_Fd400cm-Level`, `StepLevel`, `SprintingStepDown`: a chain of bound jumps across all three |
      | same, beams turned 90° | same | `SprintFwd_tr_WallBraceLowObject` (the beam's length faced him) |
      | gameplay tree | `GEN_ALL_GamePlayPlataneTree_01_016` | `PreWallEntryFront`, `WallEntryFwd30Deg_tr_Climb_Rt100cm-Up275cm`, `ClimbCycle_tr_VsLeapFeet_Balanced20Deg` (perched at z 124.5), `Vshape_VsLeapFeet_Balanced_tr_Leaping`, `Leaping_Fd900cm-Dn400cm` |
      | corner awning, 1.5 m up | `GEN_ALL_AwningCorner_90_01_019` | `JumpOn200cmBlock`, `LeapFeetLand_tr_Sprint_FastRampUp` (ran up the awning), `Leaping_Fd650cm-Dn200cm` |
      | roof piece | `GEN_ALL_Roof_CH_03_027` | `WallEntryFwd_tr_Climb_Up325cm`, `ClimbCycle_tr_MantleLeapFeet_AnticipateWall`, `LandOnBeamFwdLow_tr_Leaping` (ridge at z 125.2), `Leaping_Fd900cm-Dn400cm` |
      | stairs | `GEN_ALL_Stairs_Row_A_3x3_01_030` | `StepLevel_100cm_RP`, `Run_JumpOn50cm` |
      | pole, 3.6 m up (two rotations) | `GEN_ALL_Pole_2m_01_105` | no contact: renders as a horizontal bar above him (screenshot) |
      | "rope" (two rotations) | `GEN_ALL_Rope_01_120` | no contact: the asset is a coiled rope prop, not a tightrope |
      | façade bay (two rotations) | `MDA_House_B_2F_WD_04A_Open_041` | no contact: renders as one tall window bay well off the lane; its origin is not at its centre |
      | balcony | `GEN_ALL_B_Balcony_3m_02_020` | no samples recorded |

    - Every clone checked by screenshot rendered: crate, beams, pole, rope coil, façade bay.
    - In the first pass, a pole at 2.3 m was used as a block (`Run_JumpOn250cmBlock`, z 62.81).
    - The game's window clips are `xx_WindowEntry_*`, `xx_WindowExit_*` and
      `xx_WindowGratingEntry/Exit_*` (asset index), entered from climbing. An open window needs a
      climbable façade around it.

### Inferences

- The engine has a reusable runtime collision primitive: an InertComponent RigidBody with any
  ObjectShape subclass (the BoxShape children include Capsule, Cylinder, Sphere, ConvexVertices,
  Mesh, HeightField and List).
- RigidBody +0x58 = 0, as Inert sets it, behaves as static: the box did not move under Arno.
- The default CollisionFilterInfo (+0x60) collides with the player.
- The entity has no visual component, so the floor is invisible.
- **Two layers of traversal.** The navigation layer reacts to the spawned boxes:
  - it plays wall braces in front of an obstacle;
  - it starts a wall-run entry at a tall wall.

  So that layer queries live collision. The parkour layer produces no hold or ledge on the boxes:
  the wall entry fails, and the 1 m and 2 m tops are never mounted.

  This fits ACU-RE-0055: candidates sit on edges of MeshShape triangles. Those edges come from
  authored mesh shapes or from precomputed analysis data (`ClimbAnalysisData`,
  `ParkourAnalysisData`). Neither exists for a BoxShape made at run time.
  - Alternative 1: the parkour layer reads only MeshShape triangles at run time.
  - Alternative 2: a GameplaySurfaceNavType flag (InertComponent +0x120, a 1-byte bitfield with 5
    unnamed reflected bits) gates it. It was left at its default.
- **Parkour is analysed per object, at its current transform** (obs. 13). A crate and a beam moved
  60 m up into empty sky gave the same jump-on, step and leap actions as in the street. So the
  parkour layer does not rely on data baked for the original location. It reads the object's own
  collision wherever that is.
- A ledge alone does not make a wall climbable. Wall climbing probably needs the wall's own
  climbable collision, for example the invisible `FRV_Parkour_Row_*` / `FRV_Parkour_Corner_*`
  groups (ACU-RE-0047), and not just a hold on a box.
- The world's static meshes are drawn from a render structure that a move does not update
  (obs. 14). A copy made by a real clone or a fresh instantiation would carry its own visual
  component; not verified.
- The clone fault (obs. 15) was a missing memory frame, not a bad clone call. Opening the frame,
  as the game's own runtime clones do, makes cloning work from the window thread (obs. 16).
- A clone is a complete instance: its own visual, collision and parkour, placed and activated
  like a streamed object. Moving instead leaves the visual behind.
- **For a custom parkour course mod:**
  - Floors, walls and blockers work today (spawned boxes). They stop Arno, he walks and sprints on
    them, and he braces against them.
  - Parkour props can be copied anywhere, and they render and work there: `clone-entity` on any
    nearby crate or beam (obs. 16).
  - Working prop types so far (obs. 17): crates, beam chains (bound jumps), gameplay trees
    (climb, perch, leap), corner awnings (ramp run), roof pieces (climb, ridge) and stairs.
  - Next steps:
    - spawn by file ID through the loader, so any asset can be placed without the original loaded
      nearby; streaming unloads originals by distance (obs. 17);
    - a container world for courses, for example a small rift World (`ACU_LGS_Medieval`,
      `ACU_LGS_WorldWar2`) or `ACU_RebirthRitual`, loaded with the existing world-load mode;
    - open windows on climbable façades, swing poles placed for a swing, real tightropes;
    - save and load a course layout.

### Unknowns

- The meaning of the +0x58 values (0 = static is an inference) and the filter layer bits.
- Whether AI, navigation or the camera treat the entity as ground. NPC reactions were not checked.
- Lifetime: one extra reference is taken on the entity handle (+0x130 / +12). The entity is never
  removed and is expected to go when the World unloads; this was not checked.
- Whether a runtime MeshShape, or a set GameplaySurfaceNavType bit, gives parkour candidates.
  The parkour selection trace (`parkour_selection.profile.json`) was not run on the boxes.
- Whether a clone is cleaned up when its World unloads (one extra reference is taken), and how
  many clones the engine tolerates.
- Clones of entities with AI, physics props that move, or interactive components. Only static
  props were cloned.
- What makes a surface climbable (wall entry succeeds): the invisible parkour groups, a
  MeshShape, or a GameplaySurfaceNavType bit.
- Whether the moved prop's collision also stays at its old place. Only the new place was tested.

### Reproduction

1. Start the game:

   ```bash
   python tools/native_probe/capture_run.py local/native-probe/runs/<name> --until world --keep-game --save-pin capture-start-1 --fps-cap 60
   ```

2. Spawn the box and stand on it:

   ```bash
   python tools/native_probe/teleport.py <pid> godmode on
   python tools/native_probe/teleport.py <pid> spawn-box X Y 60 --half 100 100 0.5
   python tools/native_probe/teleport.py <pid> to X Y 61.5
   ```

3. Close the game with `capture_run.py --close <pid>`, then run `save_pin.py release <run dir>`.

### Implementation implications

None for Rust. This is a capture tool. It gives:
- flat, open ground for clean native baselines (ACU-RE-0074 obs. 12);
- a starting point for modding: runtime platforms and blocking volumes.
