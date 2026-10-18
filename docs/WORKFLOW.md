# Working loop

Use feature branches for ongoing development. Merge significant validated slices
into `main`, including the finding, implementation, tests and updated status.
On the original research machine, Git publication uses the independent checkout
at `C:/Users/PC/source/AU-RW`; the research directory remains in its parent repo.
Copy only explicit reviewed authored paths into that checkout. Preserve ignored
reference data and build caches in the research directory.

1. Read status, the task, relevant findings and any supplied handoff. Inspect
   current changes before editing. Run `python tools/acu.py doctor` when paths or
   tool availability may have changed. Reuse the inventory unless inputs changed.
2. Bound one question and its acceptance criteria using the task template.
   Record which binary, archives, export and tool sessions it concerns.
3. Gather the smallest useful evidence. Follow an address, call site, resource
   reference or file ID through to a result. Record negative searches with their
   exact boundaries. Inventory filenames are discovery hints, not decoded types.
4. Write the finding before encoding a recovered layout or behavior. Include
   observations, inference, unknowns and a way to falsify the interpretation.
5. Implement one slice. Keep parser and simulation logic testable without Bevy.
   Annotate recovered constants with finding IDs; use an ADR for design choices.
6. Validate against independent expectations. Save identities, commands and
   outputs. For visible behavior, run and inspect a current build with a capture
   manifest. A passing test covers its declared case only.
7. Update the task, status and indexes. State remaining gaps and the next ready
   task. Save a personal handoff outside shared repositories for continuation.

## Search recipes

The corpus has one file per exported address. Read exact files first:

```powershell
Get-Content -LiteralPath 'ACU - Decompiled/140001000.c' -TotalCount 90
rg -n --max-count 3 'sub_140001000' 'ACU - Decompiled' | Select-Object -First 25
rg -l -i 'parkour|free.?run|locomotion' 'ACU - Decompiled' | Select-Object -First 25
```

The last command is a bounded discovery search, not evidence of subsystem
coverage. Anonymous functions may contain no useful strings. Record call sites
and address relationships rather than repeatedly searching broad keywords.
Searches can stop early through the pipeline; do not describe those as exhaustive.

## Artifact identity

`python tools/acu.py inventory --output local/inventory.json` creates:

- Executable SHA-256 and PE metadata, including image base and entry RVA.
- Corpus file count, byte count and a filename/size metadata digest. This digest
  is NOT a full corpus-content hash or proof of matching the installed build.
- Top-level Forge names, sizes and first 32 bytes; archives are not fully hashed.
- Relevant DLL filenames/sizes/hashes and top-level extracted directory names.
- Reference parser source hashes and the parent Git root.

For each real parser fixture, hash the exact archive or identify its exact byte
range and range hash. A shared archive prefix does not identify its contents.
Check export-to-executable correspondence using several functions and original
instruction bytes in a correctly identified analysis database. Do not equate
the export VA with a live ASLR address; use RVA plus the live module base.

## Continuation

Shared status captures project facts. Personal handoffs live under
`C:/Users/PC/.codex/handoffs/Unity/` with unique timestamp/agent names. Include
Goal; Plan / Roadmap; Tried & Failed; Current State & Modified Files; Next Step.
Give the user an exact path and `/clear` instruction after saving a checkpoint.

## Live native comparison

The user authorizes discretionary game-code execution, debugger attachment,
native calls and DLL-hook development/deployment/injection without per-action
confirmation. Use live comparisons early when they resolve uncertainty more
directly than static analysis. A planning note is useful evidence, not an approval gate.

Keep hook source/build tooling in this project. Record the native executable and
hook hashes, module base, target RVA, calling convention, thread/tick context,
inputs and sampled outputs. Validate the target against the identified build
before applying a hook; preserve ABI, original call behavior and hook lifetime.
Keep logging bounded and measure instrumentation overhead when timing matters.
Compare equivalent native/Rust inputs and state; identify whether results come
from normal gameplay, injected inputs or isolated native calls.

Record loader/configuration changes and restoration steps. Preserve displaced
files before deployment. Existing ACUFixes/plugin-loader behavior is part of the
capture context. A loaded hook does not by itself demonstrate an observed call
or matching behavior; retain the actual trace and comparison result.
