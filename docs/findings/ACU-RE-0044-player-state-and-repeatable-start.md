# ACU-RE-0044 — Player entity, transform and a repeatable capture start

Date: 2026-11-14. Related: ACU-RE-0025 (player rig), [ACU-RE-0042](ACU-RE-0042-parkour-candidate-trace.md),
[ACUFIXES_REFERENCE.md](../ACUFIXES_REFERENCE.md) (leads).
State: observed live for one build and one save. The player can be found and read every frame
without hooks; a pinned save then starts every capture at the same position.

## Provenance

- `ACU.exe` SHA-256 `748abdc6456397fe7d8080f975f2a5ed3123bce548e7bf1c68b23f47e8b27e40`, base
  `0x140000000`; Ubisoft Connect launch; capture preset applied.
- Corpus `141CD0D30.c` (SHA-256 `1aeb528c…baba`): `sub_141CD0D30` returns
  `[qword_145217368 + 0x98]`; 12 corpus call sites read `+0xD8` from its result
  (`rg 'sub_141CD0D30\(\) \+ 216\)'`, e.g. `1405568D0.c:10`).
- ACUFixes leads (cite-only, unverified before this): World player at `+0xD8`; entity `World*` at
  `+0x60`, world matrix `+0x20`, position `+0x50`; component array `+0x78`, `Component::entity` `+0x10`.
- Reader: `tools/native_probe/player_state.py` (probe `READ` only).
- Live sessions: `local/native-probe/runs/live-re-1/` (PID 114608), pinned runs `pinned-3a`,
  `pinned-3b`, `pinned-5` (session SHA-256 `1477df25…2665`, `7aac2f45…c09`, `ebac42e2…e83`).
- Pinned save `capture-start-1`: `1.save` SHA-256 `c2c07ad5fbbe1073a9e1894e13a050193bdccb8b77e3541c77c34df506284d7e`.

## Observations

1. Chain `[0x145217368] → +0x98 → World W → +0xD8 → entity E` with `[E+0x60] == W` holds live.
   During loading E is a placeholder at the origin with an identity matrix; when the world loads
   E changes (here `0x1193982F0` → `0x160DA85F0`) and `+0x50` becomes a city position.
2. `E+0x50` follows input: about 8.4 m during a 2.5 s free-run nudge; standing still it is constant.
3. Matrix rows: row 2 = (0, 0, 1) every sample → **Z up**. After a camera-relative stick input,
   Arno's travel direction matched row 1 (dot 0.98) → **row 1 = forward**, row 0 = right. One
   sample; not a full convention proof.
4. All 46 pointers in the `+0x78` array point back to E at `+0x10` (component owner lead holds).
5. Writing `E+0x50` has no effect: the next frame restores it. Writing single position copies in
   four components (vtables `0x143051E60` `+0x110`/`+0x210`, `0x143028ED0` `+0x160`,
   `0x143624FD0` `+0x790`, `0x142E76630` `+0x130`) also had no effect. The authoritative
   transform is elsewhere; teleporting needs the game's own routine (open).
6. The stick is camera-relative: a "forward" push mostly turned Arno in place (0.25 m moved).
7. With the pinned save and the movement-free world check below, three runs started at the same
   position `(56.065, -270.651, -0.549)` and forward `(0.2588, 0.9659, 0)`. Load to world:
   29.2 s, 28.2 s, 28.2 s.
8. The same open-loop move script from that start still diverged: pinned-3a 233 calls / 8 chosen,
   pinned-5 34 / 23, with different chosen types. Timed scripts are not repeatable enough for
   targeted actions; closed-loop control is needed.

## Implementation consequences

- `capture_run.py` world check: tap A until `E+0x50` leaves the origin, then wait for it to hold
  still. No free-run nudges (they moved the start by load-time-dependent amounts).
- `tools/save_pin.py` pins and restores the save around each run; Ubisoft Connect writes
  `<slot>.z`/`<slot>.upload` while uploading after exit, which the tool waits out.
- `tools/native_probe/navigate.py` (closed-loop walk/face from this readout) is written and unit
  tested against a simulated camera offset; not yet run live.

Unknowns: units are presumably metres (not established); which object owns the authoritative
transform; float determinism of the readout.
