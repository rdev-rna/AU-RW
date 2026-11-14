# ACU-RE-0042 — Native parkour candidate selection, traced live

Date: 2026-11-14. Related: [ACU-RE-0041](ACU-RE-0041-acufixes-layout-spot-checks.md) (layouts),
ACU-RE-0030 (scanner), movement plan Stage 4.
State: observed. The original game's candidate lists, ranking and chosen index were recorded
for one scripted traversal run. What each action type means remains open.

## Provenance

- Installed `ACU.exe` SHA-256 `748abdc6456397fe7d8080f975f2a5ed3123bce548e7bf1c68b23f47e8b27e40`,
  image base `0x140000000` (live base equal), launched through Ubisoft Connect, save loaded.
- Probe: `acu_probe.dll` built from this branch's `tools/native_probe/probe.cpp`, SHA-256
  `b56d3efd13cfeecca918fd77bef6627e96ab2d3cb9ef66de9032dae3ce1ca3fb`; hardware execute
  breakpoints with the new bounded `GATHER` (pointer array, 16 elements × 672 bytes).
- Profile `tools/native_probe/parkour_selection.profile.json`:
  - entry `0x140133B00`, guard `488bc448895818488968204889501056`; the candidate array
    (9th argument) is `[RSP+0x48]`, consistent with `mov rsi,[rsp+0xe0]` at `0x140133B1B`
    after 5 pushes and `sub rsp,0x70`;
  - shared epilogue `0x140133D4B`, guard `4c8d5c2470498b5b40498b6b48498be3415f415d415c5f5ec3`.
    Every return reaches it: `0x140133CE9` (empty array) and `0x140133D46` fall through
    `or eax,-1` at `0x140133D48`; success sets `eax=r15d` at `0x140133D78` and jumps back.
    EAX is the selected index; RSI still holds the array. Both callers (`0x14013C200`,
    `0x140141420`) tail-jump into the function, so no post-call site exists.
  - Guard bytes and the disassembly above are identical in both mapped captures named in
    ACU-RE-0041 (`9a1124f1…0520`, `07afa161…2137`).
- Input: in-process scripted controller (focus shim `acu_focus.dll`, build SHA-256
  `981a47f236d0178610e800cb436bb54006655222ae5df536f0a69210dcb50c34`), game unfocused,
  profile `tools/native_probe/parkour_moves.example.json` (walk, free run, RT+A, RT+B,
  camera turns; about 48 s). The shim only serves XInput user index 0 and window focus; it
  does not touch game code.
- Trace: `local/native-probe/runs/parkour-1/native-session.jsonl` (machine-local, not
  distributed), SHA-256 `c8705d61918aac4be98a5525028e87c6dee84330fb377dfe2f9222dc9a7d6c5a`.
  Decoded with `tools/parkour_trace.py … --image <9a1124f1… mapped image>`.
- Probe counters after the run (cumulative since attach, including earlier short API
  heartbeat tests): 2,516 hits, 122 dropped or overwritten, 0 thread-slot conflicts.

## Result

Observed in this run:

| Quantity | Value |
| --- | --- |
| Selection calls (entry/exit pairs) | 944, all paired; 1 entry without exit (run end) |
| Calls that chose an action | 17 |
| Calls with candidates but none passing the final filters | 927 |
| Action types seen (fancy enum getter) | 3, 8, 9, 35, 38, 40, 44, 47, 48, 53, 65, 73, 75 |
| Action types chosen | 3, 8, 9, 40, 48, 53 (×3), 65 (×6), 73, 75 (×2) |
| Largest candidate list | 108 (16 gathered) |

Behaviour that matches the corpus (`140133B00.c`) on live data:

- Pass 1 drops near-zero fitness: one call considered 47 and ranked 24; another 11 → 7.
- The exit order is not fitness order. At 9.74 s a fitness-3.58 type-8 candidate ranks third
  behind 0.994 and 0.985 type-3 candidates, as expected if the sort key is fitness times the
  tester's per-action weight (fancy `0x28`, not captured).
- The final filters reject ranked candidates: at 16.40 s the selected index was 4, with the
  four higher-ranked type-8 candidates rejected.
- A run of type-65 selections (30.3–33.8 s, fitness 2.25–2.28, distance 0.95–1.50) coincides
  with the RT+A forward step after the first camera turn. The game-window capture taken after
  the run shows Arno on a roof edge. The meaning of type 65 is not established.

Inferences and limits:

- Type 53 is considered about twice per call almost continuously (1,874 of the gathered
  candidates); it is the most common background candidate. Not interpreted further.
- Only 16 candidates per call are copied. Selected indices above 15 would be reported as
  `selected_outside_gather`; none occurred.
- Field names for `+0xC0`/`+0xC4` are ACUFixes' (unverified) and are reported separately.
- Hardware-breakpoint sampling serialises hits; 122 events were dropped or overwritten over
  the session, so per-frame completeness is not claimed.
- One run, one save location. No claim about which input caused which action type.

## Second run: capture preset, 60 fps cap

Same profile and moves, after a relaunch with `tools/capture_preset.py` applied and the shim's
60 fps limiter; Arno started at the save's spawn in a crowded street, not where run 1 began.
Trace `local/native-probe/runs/parkour-2-preset/native-session.jsonl`, SHA-256
`b2d4693db9fb5bbc759c6900933d1ced47fbc9585a140aaff8a32ca0033b53fd`. Probe drops: 0.

| Quantity | Run 1 | Run 2 |
| --- | --- | --- |
| Selection calls | 944 | 32 |
| Calls that chose an action | 17 | 31 |
| Action types chosen | 9 | 9 (adds 15, 32, 77) |

Run 2 has none of run 1's near-continuous non-selecting calls (two type-53 candidates almost
every frame). The start position, the route actually taken and the settings all differ, so the
difference is **not** attributed to the preset. Game-window captures show street level, a
roof, a facade climb and a balcony. Type 8 was chosen five times in a row (4.9–9.2 s, fitness
4.04–4.31, distance 5.3–6.6) during the first RT+A step.

Run 3, `capture_run.py` (one command, 2026-11-14): trace
`local/native-probe/runs/parkour-5-runner/scenario/native-session.jsonl`, SHA-256
`143ce084a5504af6acd48264208b2f55c5ee1da775b0f2a730ab77cb9323e6fa`; preset confirmed by a
1280x720 client area; 37 selection calls, 35 chosen, 16 chosen types, 0 probe drops,
no decode problems. Different start again (the autosave moves it), so not compared.

## Reproduce

```powershell
python tools/native_probe/capture_run.py local/native-probe/runs/<name> --image <mapped image>
```

or step by step with a running game:

```powershell
python tools/native_probe/focus.py install <pid>
python tools/native_probe/controller_autoplay.py <pid> tools/native_probe/parkour_moves.example.json local/native-probe/runs/<name> --expected-sha256 748abdc6456397fe7d8080f975f2a5ed3123bce548e7bf1c68b23f47e8b27e40 --background --in-process-pad --probe-profile tools/native_probe/parkour_selection.profile.json
python tools/parkour_trace.py local/native-probe/runs/<name>/native-session.jsonl local/native-probe/runs/<name>/decoded --image <mapped image>
```

Next: map the action-type numbers to classes (fancy table → constructor → factory switch
`0x1401CC990`), capture the weight (fancy `0x28`) at the exit, and pair selections with
animation clip starts to tie decisions to the clips the Rust side already decodes.

## Development handoff
- **Confidence:** observed — candidate lists, ranking and chosen index recorded live in three
  runs; what each action type means is open.
- **Established:**
  - Selection routine entry `0x140133B00`, shared epilogue `0x140133D4B`; candidate array is the
    9th argument (`[RSP+0x48]` at entry); EAX at the epilogue is the chosen index or -1
    (`or eax,-1` at `0x140133D48`). Callers `0x14013C200`, `0x140141420` tail-jump in.
  - Live behaviour matches `140133B00.c`: pass 1 drops near-zero fitness; exit order is not
    fitness order; final filters can reject higher-ranked candidates (run 1, 16.40 s, index 4).
  - Action-type numbers seen and chosen per run (tables above; runs `parkour-1`,
    `parkour-2-preset`, `parkour-5-runner`).
- **Not established:** meaning of any action type; the sort key (fitness × per-action weight at
  fancy `0x28` is inferred, weight not captured); which input caused which type; field names
  `+0xC0`/`+0xC4` (ACUFixes, unverified); per-frame completeness (run 1 dropped 122 events; only
  16 candidates per call gathered); run-to-run comparability (start, route and settings differ).
- **Interface:** unknown for product code. Observed shape: an array of pointers to 672-byte
  candidates, each with fitness, distance and action type; output = selected index or -1. A
  future selector belongs in `acu_sim`; the probe and trace decoder are live tooling only.
- **Oracle:** `python tools/native_probe/capture_run.py local/native-probe/runs/<name> --image
  <mapped image>`, decoded by `tools/parkour_trace.py`; compare per call the candidate set, rank
  order and selected index/type with a Rust selector fed the same candidates.
- **Spawned / follow-up:**
  - Map action-type numbers to classes (done in ACU-RE-0043; names still open).
  - Capture the per-action weight (fancy `0x28`) at the exit.
  - Pair selections with animation clip starts (started in ACU-RE-0045).
