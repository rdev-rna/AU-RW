# ACU-RE-0052 — Player camera: objects, fields and camera-relative forward

Date: 2026-11-14. Related: [ACU-RE-0044](ACU-RE-0044-player-state-and-repeatable-start.md)
(stick is camera-relative, row 1 = forward), [ACU-RE-0051](ACU-RE-0051-tick-and-time.md) (clock
ticks), REWRITE_READINESS backlog item 3.
State: **live-verified (runs camera-1, camera-2).** Static part from the pseudocode corpus and the
mapped image; live part from probe sessions and memory polls on the running game (section
"Live result"). Oracle: `data/oracles/camera.json`.

## Provenance

- `ACU.exe` SHA-256 `748abdc6456397fe7d8080f975f2a5ed3123bce548e7bf1c68b23f47e8b27e40`, base
  `0x140000000`. Mapped image (Unity project)
  `local/native-probe/motion-link-20261020/runtime-view/capture/main-image.mapped.bin`, SHA-256
  `9a1124f1c4c368939ad3d08315b3dc968e6e48cc22f7b096f34a94a5e7380520`, VA = base + file offset.
- Corpus `E:/ReverseEngineering/Projects/Unity/ACU - Decompiled/`; first 16 hex digits of file
  SHA-256: `141EA39C0.c` `38db6642bab52415`, `141E91F10.c` `396266d28b04caa4`, `141EF7D20.c`
  `bcfecbf5992a99ab`, `14054F7E0.c` `a4ac0ca20d31345e`, `141EA42A0.c` `0546833f31a055db`,
  `141F44750.c` `888d52e04ff5f70b`, `141ED6BB0.c` `fb3c79832630959c`, `141ED39C0.c`
  `df947df1239bc914`, `141E7BE20.c` `6b31df809ab546fc`, `141E7B7F0.c` `79a3bda58a685985`,
  `141F11340.c` `5130385d002b70fb`, `141E4EF00.c` `c569dcddbcf39f82`. Export identity unverified
  (ACU-RE-0031).
- Class names come from the engine's type descriptors in the mapped image: descriptor `+0x18`
  name pointer, `+0x20` hash, `+0x28` size field, `+0x38` parent descriptor; a vtable's slot 5
  (`+0x28`) returns the class-info that points to the descriptor. Names are therefore
  **observed strings**, not guesses.
- Tools: as ACU-RE-0051 (bounded `rg`, rel32 scanner over `.text`, capstone).

## Observations (static)

1. **Class chain.** `ACUPlayerCameraComponent` (descriptor `0x144EF0CF0`, hash `36647abf`, size
   field `0x12D0`, vtable `0x143050B00`) → `PlayerCameraComponent` (`0x144EEA210`, `f7aef970`,
   `0x120`, vtable `0x14304D9E0`) → `Component` → `BaseComponent` → `ManagedObject` → `Object`.
   It is a Component, so `+0x10` is its owning entity (ACU-RE-0044 obs. 4).
2. **PlayerCameraComponent ctor** `0x141ED6BB0` (lines 15–23): `+0x50` 16 B = 0, `+0x60` 16 B =
   `xmmword_142E75BE0` = (0, 0, 0, 1) (identity quaternion), `f32 +0x70` = `0x3F490FDB` = π/4.
   **ACUPlayerCameraComponent ctor** `0x141F44750` → `0x141F479D0` (lines 17–20): `+0x120` =
   dispatch table `aU_3181` (most ACU-specific virtuals jump through `[this+0x120]`, e.g.
   `0x141F55C30`), and a second set `+0x8B0` = 0, `+0x8C0` = identity quaternion, `f32 +0x8D0` =
   π/4; sub-objects at `+0x940` (816 B), `+0xA30`, `+0xA38` (96 B), `+0xA40` (184 B).
3. **CameraManager** singleton `qword_14521AAD0` (ctor `0x141ED39C0` line 19, vtable
   `0x14304D800`, descriptor hash `d92dcaa7`, size field `0x98`):
   - camera component array `+0x40`, u16 count `+0x4A` (`141EF7D20.c` lines 11–18,
     `0x141EE92A0`);
   - active render view `+0x70` (`0x14204CE70`; set by `0x141E823A0` line 9 from the viewport
     picker `0x142209770`). Consumers read the view's `+0x30` as a position (`0x140FAFA20`).
4. **Player camera binding.** `0x141E4EF00` (lines 9–14) takes `World+0xD8` (the player entity E,
   via `0x1420B2240` = `[World+216]`) and activates (vtable `+0x280`) the CameraManager component
   whose `+0x10 == E`. The player camera component is therefore **owned by the player entity**.
5. **Camera forward from a component.** `0x141EF7D20(mgr, out, entity)` (lines 18–23) finds the
   component with `+0x10 == entity` and returns `q ⊗ (0, 1, 0)` with `q = component+0x60`
   (`xmmword_1435F22E0` = (0, 1, 0, 0), factor `xmmword_142E78010` = (2, 2, 2, 0)): camera
   forward is the camera's **local +Y**, matching ACU-RE-0044 (row 1 = forward). Default when
   not found: (0, 1, 0).
6. **CameraProxy** singleton `qword_14521A768` (ctor `0x141E7BE20` line 32, vtable `0x143040D10`).
   `GetForward` `0x141E91F10(proxy, out, framesBack)` (45 rel32 callers) by mode `u32 +0x64`:
   mode 2 → quaternion `+0xB0`; a running blend (byte `+0x58`, timers `+0x40`/`+0x48`/`+0x50`)
   → quaternion `+0xE0`; mode 1 → CameraManager component of the player (obs. 5); otherwise →
   CameraHolder rotation ring (obs. 7) `framesBack` frames ago (`0x141E91E60`).
7. **CameraHolder** singleton `qword_1451B1D50` (ctor `0x141E7B7F0` line 19, vtable
   `0x143041338`). Per-frame update `0x141EA39C0` (lines 40–47): updates the components in its
   list (`+0x08`, u16 `+0x12`, `0x141F16F80`), publishes the camera (`0x141F11340`), then pushes
   into 5-entry rings: index `u32 +0x120 = (idx + 1) % 5`, position ring `+0x80 + 16·idx`,
   normalised rotation ring `+0xD0 + 16·idx`.
8. **Published camera globals** (`0x141F11340` lines 18–22, `0x141F15570`, `0x141F06870`), under
   lock `off_14521AB60`, from the camera object behind the handle at `CameraHolder+0x28`:
   `xmmword_14521AB40` = object `+0x10` (position), `xmmword_14521AB50` = object `+0x20`
   (rotation quaternion), `f32 dword_144EE0304` = object `+0x58`. Readers `0x141EF7260`,
   `0x141EF72C0`, `0x141EF7220` (9 callers).
9. **Frame order.** Job `UpdateCamera` ("Ai::UpdateCamera", `141CE4650.c` lines 320–325) runs
   `0x141EA42A0(CameraProxy)`, which calls `0x141EA39C0(CameraHolder)` (line 233) and then
   `0x141F12800(CameraManager, player)` (line 240). Blend durations are in clock ticks (30000 =
   1 s, ACU-RE-0051 obs. 4), e.g. `30000 × 0.5` (line 121).
10. **Flattened forward.** `0x14054F7E0(out, &up)` gets the proxy forward, then two normalised
    cross products give the forward projected perpendicular to `up` (ground-plane forward).
    Callers: `0x140940900` (twice), `0x141921140`, `0x141A19040`, `0x141A1F540`.

## Inferences

- (Superseded by the live result: the published globals are **not** the live view.) Live camera
  pose = `xmmword_14521AB40` (position), `xmmword_14521AB50` (quaternion x, y, z, w);
  forward = q ⊗ (0, 1, 0); **yaw = atan2 of forward (x, y)** and **pitch = asin(forward.z)** with
  Z up (axis use per ACU-RE-0044). `dword_144EE0304` is probably the vertical FOV in radians
  (π/4 defaults elsewhere); not proven.
- PlayerCameraComponent `+0x50` is probably its position (it sits beside the `+0x60` quaternion
  and `+0x70` FOV); only `+0x60` is proven by a reader.
- **Distance** = |camera position − `E+0x50`|; no stored orbit distance, yaw or pitch field was
  identified (candidates: the ACU sub-objects at `+0x940…`).
- The stick → camera-relative routine is most likely one of the callers of `0x141E91F10` /
  `0x14054F7E0` that only run (or change result) while the stick is deflected. `0x140940900`
  uses both and is the first static candidate; it is **not** confirmed.

## Live result (runs camera-1, camera-2)

Setup: pid 86540, Paris, pinned save `capture-start-1` (world travelled from Versailles), focus shim
t8 at a 30 fps cap, Invisible + God. Input through the shim's in-process pad (no real devices).
Runs live under `local/native-probe/runs/camera-1` (probe session with the four hooks of the plan
above, plus a 250 ms memory timeline) and `camera-2` (`tools/native_probe/camera_sweep.py`, 50 ms
memory poll). Tools: `tools/native_probe/camera_state.py`, `camera_sweep.py`, native probe
`client.py`.

11. **The published globals are not the live view.** Over 12 s, while Arno walked 10.6 m,
    `xmmword_14521AB40/AB50` stayed at (576.10, 94.86, 1.60), 221 m from the player. The CameraHolder
    rings (`camera_holder_update`, 206 hits, all from `0x141EA464F`) held that same position in all
    5 slots, and the screenshot showed a normal follow camera behind Arno. That holder camera is
    stale or belongs to another view; its role is unknown.
12. **Live camera = the player's `ACUPlayerCameraComponent`.** `camera_manager_forward`
    `0x141EF7D20` hit 1,412 times in about 7 s, all from `0x141E91F8B` (inside the proxy's
    `GetForward`, so the proxy is in mode 1 in normal play). Its gathered CameraManager array had
    count 1. The element had vtable `0x143050B00` and `+0x10` == r8 == the current World's player E
    (all hits). Its `+0x50` held a position about 3.9 m behind and 1.6 m above E (e.g. camera
    (685.24, −96.90, 1.20) against player (688.10, −94.76, −0.38)), tracking E as it walked.
    `+0x60` is a unit quaternion (x, y, z, w). This confirms obs. 4–6, and that `+0x50` is the
    position. Read path without hooks: `[0x14521AAD0]` → `[+0x40]` array, u16 `+0x4A` count, the
    element with that vtable whose owner is E (`camera_state.py`).
13. **Forward stick follows the camera yaw.** With the left stick straight forward, the early move
    heading equalled the camera yaw within 0.1° (camera-1: camera 36.6° vs displacement 36.9°,
    math convention; camera-2: 0.0° median). Straight back gave 180.0°. Right (+X) started at about
    +89° and settled at +91…+93° while the camera turned. Left gave −49…−29°, but Arno faced −90°
    relative to the camera throughout, so that run was probably deflected by the corridor wall; it
    needs a re-run in the open. Compass yaw = atan2(fx, fy) of forward = q ⊗ (0, 1, 0); Z up.
14. **Right stick rate and response (30 fps cap).**

    | Input | Yaw rate |
    | --- | --- |
    | +X full (32000) | +218 °/s |
    | −X full | −220 °/s |
    | +X half (16000) | +25 °/s |

    The response is strongly non-linear (dead zone and/or curve: half deflection gives about
    1/9 of the rate). +X turns the view clockwise seen from above (looking right). The camera
    eases out after release: about 11–15° more in the following second.
15. **Pitch.** Right stick +Y raised the pitch (not inverted). The rest pitch was −3.3°, and the
    observed range was +64° up to −78° down. Full up took 1.5 s to reach +64°; full down went
    from +63° to −78° in 1.4 s. When Arno starts moving, the pitch recentres to about −3.3° within
    2.4 s.
16. **Distance is not fixed.** Camera–player distance was 3.9 m at rest and level. It fell to
    about 2.4 m while yawing in the corridor (collision with the arches is the likely cause, not
    proven), to 1.2 m when pitched up (camera near the ground), and grew to 3.6–3.8 m pitched
    down. Camera height above the player's feet: +1.6 m at rest.
17. **Auto-follow.** Strafing right with a neutral right stick turned the camera +134° in 2.4 s
    (about 55 °/s). Forward with half right stick turned it 77.5° in 3 s (26 °/s, close to the
    25 °/s of half right stick alone).

Hook callers for the plan's other probes, recorded for the dossier:
- `camera_proxy_forward` `0x141E91F10`: 1,436 hits from `0x142655D32`, `0x142655DB8`,
  `0x14037E2B6`, `0x140FEC47B`, `0x142656E6A`, `0x14054F843` (= `flatten_forward`),
  `0x14037EBD9`, `0x140487FA9`.
- `flatten_forward` `0x14054F7E0`: 196 hits, all from `0x141A194D1` (the `0x141A19040` caller of
  obs. 10).

The routine that turns the stick into a heading is not yet named: no caller set was compared
between neutral and deflected stick. It is not needed for the port, because the oracle pins the
behaviour (heading = camera yaw + stick angle).

## Unknowns

- ~~Which proxy mode is used in normal play~~: mode 1 (player component), obs. 12.
- Whether the player component (vtable `0x143050B00`) is in E's own `+0x78` component array.
- The ACU orbit parameters (yaw/pitch/distance targets, lag, collision) inside
  ACUPlayerCameraComponent; the input → camera rotation routine (right stick).
- The type of the camera object behind `CameraHolder+0x28` and of the render view at
  `CameraManager+0x70`.

## Live verification plan

Probe register index: 0 = rcx, 1 = rdx, 2 = r8, 3 = r9; every hit also records return address,
64 stack bytes and xmm0–5. Profile: `tools/native_probe/camera.profile.json`.

| # | Label | VA (RVA) | First 16 bytes | Snapshot | Confirms if | Falsified if |
| --- | --- | --- | --- | --- | --- | --- |
| 1 | `camera_holder_update` | `0x141EA39C0` (`1ea39c0`) | `40534883ec30488d99f0010000488974` | rcx + 0x80, 176 B (both rings + index) | rcx == `[0x1451B1D50]`; one hit per frame; ring[idx] position equals `xmmword_14521AB40` of the previous frame and lies a few metres from `E+0x50` | no per-frame hit, or ring position far from the player |
| 2 | `camera_proxy_forward` | `0x141E91F10` (`1e91f10`) | `48896c2418574883ec20488bfa8b5164` | rcx + 0x40, 176 B (mode `+0x64`, quaternions `+0xB0`/`+0xE0`) | rcx == `[0x14521A768]`; the set of return addresses that hit only with the left stick deflected names the camera-relative movement routine | no caller differs between neutral and deflected stick |
| 3 | `camera_manager_forward` | `0x141EF7D20` (`1ef7d20`) | `4d85c0742d4c8b49400fb7494ac1e103` | rcx + 0x40, 64 B; gather: array `[rcx+0x40]`, count u16 `rcx+0x4A`, 16 × 128 B | r8 == player E; a gathered component with vtable `0x143050B00` and `+0x10 == E`; q ⊗ (0,1,0) from its `+0x60` matches the forward from `xmmword_14521AB50` | never hit (proxy not in mode 1) — then rely on the holder ring path |
| 4 | `flatten_forward` | `0x14054F7E0` (`54f7e0`) | `48895c2408574881ec80000000488bd9` | rdx, 16 B (the up vector) | up = (0, 0, 1) for the player; return address in `0x140940900` or another caller that correlates with stick input | up not Z or no hits during locomotion |

Memory reads without hooks (per frame or 50 ms):

- `xmmword_14521AB40` (camera position), `xmmword_14521AB50` (quaternion), `f32 0x144EE0304`.
  Derive forward, yaw, pitch and distance to `E+0x50`; compare forward with the player's motion
  direction under a known left-stick vector (ACU-RE-0044 closed-loop tooling).
- `u32 [0x14521A768] + 0x64` (proxy mode); `[0x14521AAD0]`: `+0x40` array, u16 `+0x4A` count,
  `+0x70` view; for each component check vtable and `+0x10 == E`, and read `+0x50/+0x60/+0x70`.
- Search E's `+0x78` components (u16 count `E+0x82`) for vtable `0x143050B00`.
- Right-stick sweep: yaw should change monotonically with right-stick X; record yaw rate vs input
  (that is the oracle for camera rotation speed).

Oracle target: per frame input (both sticks) → camera position, quaternion, yaw/pitch/distance and
player heading, exported like the travel oracle.

## Implementation implications

- `acu_sim` can model camera-relative input as: forward = camera quaternion applied to local +Y,
  flattened against world up (Z), right = forward × up (sign to confirm); the stick vector is
  then combined in that basis. Until the live check, treat the basis convention as a candidate.
- Keep the camera pose as position + quaternion (as the original publishes it); derive yaw/pitch
  only for presentation and tests.

## Development handoff

- **Consumer:** `acu_sim` (camera state, camera-relative input), `acu_bevy` (view).
- **Model:** a follow camera with position + quaternion, forward = q ⊗ (0, 1, 0). The move heading
  equals the camera's compass yaw plus the stick angle (obs. 13). Right stick X sets the yaw rate
  (non-linear: 218 °/s full, 25 °/s half at 30 fps); Y sets the pitch, clamped to about −78°…+64°.
  The pitch recentres to about −3.3° on movement, and the yaw follows a strafing player at about
  55 °/s. Distance is about 3.9 m behind and 1.6 m above at rest, shortened by collision and pitch.
- **Oracle:** `data/oracles/camera.json` (`acu-oracle/camera/1`), regenerated by
  `python tools/native_probe/camera_sweep.py PID local/native-probe/runs/camera-2/sweep.jsonl`
  then `python tools/oracle_export.py camera`. Diff: phase summaries (rates, pitch range, early
  move heading relative to the camera).
- **Implementation choices still open:** the stick response curve between the two measured
  points, the collision probe, the easing constants, and frame-rate dependence (30 fps only). Record
  each as a labelled choice or measure it first; ACUPlayerCameraComponent `+0x940…` sub-objects
  are the static lead for orbit parameters.
