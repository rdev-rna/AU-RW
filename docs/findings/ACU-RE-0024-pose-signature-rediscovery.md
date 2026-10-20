# ACU-RE-0024 - Rediscovering current pose buffers from captured topology bytes

Date / investigator / related task: 2026-10-20 / Codex / ACU-008
State: partially established

## Provenance

Binary path / SHA-256 / image base / VA / RVA:
- PID 16136, `D:/Games/Assassin's Creed Unity/ACU.exe`, SHA-256
  `748abdc6456397fe7d8080f975f2a5ed3123bce548e7bf1c68b23f47e8b27e40`,
  image base `0x140000000`.
- A positive scan control found the 16 captured instruction bytes
  `0f2970e80f2978d8440fb7d9440f2940` at VA `0x1400988e3`, RVA `0x988e3`.
  This was a memory read; no breakpoint or hook was installed.
- The exact prior 300-byte topology capture was found at `0x1068e75d0`.
  The exact prior 200-byte joint hash/index capture was found at
  `0x10685c3b0` and `0x1068e78b0`.

Corpus path and lines / export identity status:
- `ACU - Decompiled/1400988D0.c:47-52` suggests a metadata pointer at
  context `+0x170`, a pose-array pointer at `+0x178`, a topology pointer at
  metadata `+0x68`, and a 32-byte record stride. This export's correspondence
  remains unverified. The earlier guarded captures and independent record
  checks are described in [ACU-RE-0022](ACU-RE-0022-live-bone-pose-buffer.md).

Archive identity / byte range / file ID: not applicable to this live-memory slice.

Stored and decoded lengths / SHA-256:
- Reference topology: 300 bytes, SHA-256
  `052eac64ab8e0c38aa01bba22dedad998b0c869f839f10286814ce99491dafd5`.
- Reference files remain under `local/native-probe/bone-trace-20261019/`;
  each exact signature and pointer signature has a length and hash in the
  scan manifests. No original game or reference file was modified.

Tool/provider/version / evidence IDs or saved output:
- Python 3.12 / Windows x64, external `VirtualQueryEx` and `ReadProcessMemory`
  through [scan_memory_signature.py](../../tools/native_probe/scan_memory_signature.py).
  The PID's executable path and hash are checked before each scan.
- Evidence directory: `local/native-probe/signature-discovery-20261020/`.
- `topology-scan-low.json` SHA-256:
  `0eb166f353940adf42a8807f9d63cadd16726aecd6a561436084f67c8110bf2e`.
- `metadata-references-full-range.json` SHA-256:
  `654cf3830f33633436a34d29de4ef3c60479f676513d06c8dae29a131cb854db`.
- `metadata-references-upper.json` SHA-256:
  `752e717c3729c1e606f3c5d6c5165b7f5c9d3e8a23351a4b373f0f34cab50e8a`.
- `pose-validation-full.json` / `pose-validation-upper.json` SHA-256:
  `53929fd0912a8b5341bd782e5be897e94a34c75381302b16a4b60e7efb9d37ee` /
  `84c896d00713e30cbcd1f4c34e9597805b86e783f58ec302d3008dd88ea5fffb`.
- `pose-classification-all.json` SHA-256:
  `883a31ed529c672cec7f385130e2750e3d918c951eb36f20980f4f8ee302f83f`.

Exact commands and search boundary:
- Initial exact-table search: address interval `0x10000..0x400000000`,
  4 GiB successful-read budget, 40 s time budget; stopped at the byte budget,
  next address `0x1755a0000`. Other table copies outside that prefix are unknown.
- Pointer to topology at `0x1088a1c58` gives candidate metadata base
  `0x1088a1bf0` using the existing `+0x68` rule. A live 192-byte read confirms
  the topology pointer at `+0x68`, joint hash/index pointer at `+0x38`, and
  paired 25-valued fields at `+0x58` and `+0x70`.
- Exact metadata-pointer search: `0x100000000..0x180000000`, 4 GiB / 40 s /
  4,096-hit limits; 2,100,989,952 readable bytes, 1,589 hits, address bound
  reached with no read failures. The preliminary 256-hit scan was incomplete
  and was replaced by this wider hit budget.
- Continuation search: `0x180000000..0x7fffffffffff`, same budgets;
  1,446,277,120 readable bytes, 559 hits, no read failures. `VirtualQueryEx`
  stopped at `0x7fffffff0000`; the tail beyond that point is unqueried.
  Metadata references below `0x100000000` were not searched.

## Result

Observations:
- For each metadata-pointer hit, read the following qword as a candidate array
  pointer and define candidate context as `hit-0x170`. Validate 25 contiguous
  32-byte records: all finite; translation W within `1e-5` of zero;
  quaternion squared norm within `1e-3` of one; translation magnitudes below
  10,000 stored units. These are analysis filters, not native limits.
- 1,286 candidates passed in the lower interval and 238 in the upper interval.
  Passive three-second polls observed changes in 255 and 25 respectively.
  These polls are separate observations, not a coherent city-wide frame.
- Compare each candidate's 24 nonroot translations with the first local-pose
  record in `bone-buffer-poll2.jsonl` from ACU-RE-0022. 621 contexts, referencing
  620 distinct initial arrays, match with maximum translation error below
  `1e-5`. Forty-three of those contexts changed during passive polling.
- Local candidate `0x136007b00` has array `0x136007ca0`; its 24 nonroot
  translations match exactly and up to 24 records changed during polling.
  This establishes current changing skeletal data without the failed hook.
- A separate saved raw-byte poll of the 43 moving local candidates contains
  57 frames over three seconds; 36 candidates changed in this later interval.
  The full arrays allow independent recomputation of changing-record counts.
  `current-local-poll/samples.jsonl` SHA-256:
  `b614b6c283f7f7645517e88ed516e82f8e4eaab6eaaac790596d1bdd598573a7`.
  Its initial probe status reports every hook clear. No input was replayed.

Inferences and alternatives:
- The exact topology bytes, joint identities, record invariants, and matching
  local translations strongly support shared skeletal pose buffers. Several
  contexts can alias one buffer or retain stale data. Many belong to crowd
  actors; sharing the topology does not identify Arno.
- Neighborhood checks around three early moving contexts found owner/component
  class candidates, but proximity does not prove a field relationship. Their
  addresses are discovery hints only. The 43 moving local contexts replace
  the six unvalidated candidates used in runs 40-43 for future investigation.

Unknowns / incomplete coverage:
- Player/Arno identity, ownership, active clip/time, simulation root, facing,
  and native/Rust phase agreement remain unresolved. No input replay ran
  during these polls. Exact signatures miss other rig variants and modified
  table copies. Reads race animation writers and object lifetimes.
- Matching boundaries across separate memory regions and failed chunks is
  unsupported; manifests retain coverage and stop reasons. Neither query
  completeness nor a pose hash establishes locomotion parity.

Independent comparison / falsification test:
- Positive control: the known instruction signature occurs exactly once in
  `0x140098000..0x140099000`, at the expected VA.
- One altered signature byte gives zero hits in the same page. An eight-byte
  scan budget starting at the known 16-byte match gives zero hits and stops
  exactly at `0x1400988eb` with `byte_bound`.
- These controls validate the exercised search/budget cases. They do not
  validate actor identity or every memory-boundary case.

Reproduction steps:
- Run the scanner with the recorded exact reference file, executable hash,
  bounds and budgets. Rediscover pointers per process; addresses are transient.
- Local `validate_contexts.py` records first arrays and changing-record counts;
  `classify_poses.py` records template errors. Use the full/upper manifests and
  preserve distinct output files. Next compare controlled movement/stop phases
  across fresh pose and transform candidates, then confirm a direct owner link.

Implementation implications and finding-linked constants:
- No locomotion constants or Rust animation selection changed. This provides
  a hook-free way to reacquire current pose candidates for ACU-008. Full parity
  still requires verified player/root ownership and equivalent native/Rust states.
