# Project board and delegation protocol

GitHub issues and the "AU-RW Reconstruction" project are the claim and status
surface. `docs/` stays the evidence record. The repository is public: issues and
PRs carry addresses, hashes, finding IDs and doc paths only, never game bytes,
extracted assets or decompiled code.

## Finding work
1. Query ready, unclaimed work:
   `gh issue list -R rdev-rna/AU-RW --label status:ready --label agent-ok --search "no:assignee"`
2. Search open issues and PRs for your topic before creating anything. One issue
   per ACU-NNN / ACU-RE-NNNN; the ID is in the title.
3. Areas and their anchors, owning crates and reuse sources: `docs/AREAS.md`.

## Claiming
1. Self-assign; replace `status:ready` with `status:claimed`.
2. Comment: `CLAIM <name/agent> branch=acu-<issue#>-<slug> files=<owned paths>`.
3. Work on that branch. Open a draft PR early with `Closes #N`.
4. No commit or comment for 72 h: anyone may comment `STALE-RELEASE` and take it.
5. Agents never take `human-only` issues. Do not claim a slice with open blockers.

## Dependencies
Epics hold slices as sub-issues. Write `Depends on #N` in the body. A slice is
`status:ready` only when every dependency is closed. When you finish a slice,
promote the slices it unblocked.

## Locks (single-instance resources)
Pinned issues `lock:game-process`, `lock:ida-rea-session`, `lock:capture-run-dir`.
Comment `ACQUIRE <name> pid=<PID> until=<time>` before launching/injecting/sending
pad input or binding an IDA/REA session; comment `RELEASE` when done. Do not
touch the game, the probe pipe (`\.\pipe\acu-probe-<PID>`), the virtual pad or the
debug-register hooks without holding the lock. Prefix run directories under
`local/` with `<issue#>-<name>-`. Never reuse a For Honor IDA/REA session.

## Shared files (do not edit directly in slice PRs)
- `docs/STATUS.md`, `docs/TASKS.md`, `docs/FINDINGS.md`: maintainers summarise.
  Slice notes go in `docs/status/<issue#>-slug.md`. TASKS.md keeps ACU-NNN -> #N.
- Finding numbers: opening a `type:re` issue reserves the next ACU-RE number
  (state it in the issue); the PR adds only `docs/findings/ACU-RE-NNNN-slug.md`.
- `Cargo.toml`, `Cargo.lock`, toolchain pins: only via an `area:infra` issue.
- `acu_motion/src/lib.rs` (flat `pub mod` list): add your module line only;
  new areas get their own crate when their first slice lands.

## Delegating and handing off
- Found an unknown outside your slice? Open a `type:re` or `type:slice` issue with
  the exact question, VA/corpus/archive pointers and `agent-ok`; do not widen
  your slice.
- Stopping mid-slice: comment Goal / Done / Tried & Failed / Modified files /
  Next step, set `status:ready` (or `blocked`), unassign. Personal handoff files
  still go to `C:/Users/PC/.codex/handoffs/Unity/` and may be linked, not required.

## Labels
`status:` needs-triage, ready, claimed, in-review, blocked, needs-evidence.
`type:` epic, slice, re, bug, infra. `area:*` per `docs/AREAS.md`.
`agent-ok` / `human-only`. `size:S|M|L`. `lock`.
Evidence rules in AGENTS.md apply to every issue: separate observation,
inference, implementation choice and unknown.
