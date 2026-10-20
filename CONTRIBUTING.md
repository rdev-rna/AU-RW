# Contributing to AU-RW

For a guided first session, paste the
[collaborator onboarding prompt](docs/COLLABORATOR_PROMPT.md) into your coding
agent after filling in its task and local paths.

This is an early Rust/Bevy reconstruction, currently a static asset inspection
scene rather than a playable game. Start with [status](docs/STATUS.md), the
[task queue](docs/TASKS.md) and [movement plan](docs/MOVEMENT_PLAN.md).
Agree on one bounded task to avoid overlapping decoder or renderer edits.

## Local setup

Use Python 3.12+ for the tooling. The Rust workspace pins its toolchain in
`acu-rs/rust-toolchain.toml` and dependencies in `acu-rs/Cargo.lock`.
On Windows the current build uses the MSVC target and C/C++ linker tools.
Other platforms have not been validated. No sibling project is required.

Keep your own original game installation and decompiled pseudocode outside
the checkout, or under ignored `local/`. Neither is distributed here.
From the checkout root, configure their actual locations:

```powershell
python tools/acu.py configure --game-dir 'D:/MyGames/ACUnity' --decompiled-dir 'E:/MyResearch/ACU-pseudocode'
python tools/acu.py paths
python tools/acu.py check
python tools/acu.py doctor
python tools/acu.py inventory --output local/inventory.json
./tools/run_arno.ps1
```

`configure` creates ignored `local/project.json` and refuses to overwrite it.
Edit that file to change paths later. Paths can be absolute or relative to the
checkout. Overrides are `ACU_GAME_DIR` and `ACU_DECOMPILED_DIR`; each takes
precedence over the local configuration, then the example configuration.
The example uses deliberately unpopulated locations under ignored `local/`.
`for_honor_project` is optional (`null` by default); only historical reader
provenance investigations need that reference. `ACU_FOR_HONOR_PROJECT` can
override it. Doctor reports its absence without failing.

The scene needs the game install but does not read the decompiled corpus.
Research inventory/doctor require both. `check` and authored tests need neither.
The launcher accepts `-GameDir` or `ACU_GAME_DIR` before falling back to the
configuration. Direct Cargo tools use their documented arguments or
`ACU_GAME_DIR`; they do not read `local/project.json`. To set the environment
from configuration for Cargo commands:

```powershell
$env:ACU_GAME_DIR = python tools/acu.py paths --key game_dir
```

## Validation and evidence

```powershell
python -m unittest discover -s tools -p 'test_*.py'
Set-Location acu-rs
cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo test --locked --workspace
cargo build --locked --workspace
```

Install-dependent tests are ignored by default. Run relevant ones explicitly
with your own `ACU_GAME_DIR` and record the actual archive/build identities;
missing data is unavailable, never parity success. Freshly inspect visible
changes. Follow [validation](docs/VALIDATION.md) and the
[evidence workflow](docs/WORKFLOW.md). Record recovered behavior using the
[finding template](docs/templates/FINDING.md), including uncertainties.

Contribute authored code, synthetic fixtures, documentation and small hashes/
identifiers. Keep original executables, archives, extracted assets, decompiled
files, database exports, captures and machine reports out of commits and uploads.
Findings refer to local corpus paths as provenance; those files are not bundled.
Review diffs and the staged file list before each commit. Do not commit local
configuration or credentials. A `.gitignore` does not remove already tracked data.

## Repository and source snapshots

The public collaboration repository is
[rdev-rna/AU-RW](https://github.com/rdev-rna/AU-RW), maintained by Rdev.
Clone it with `git clone https://github.com/rdev-rna/AU-RW.git`.
Use branches and pull requests for bounded contributions; include validation
and evidence in the description. Coordinate task ownership before editing.

The original research directory is inside a parent repository; the public repo
was prepared in a separate source-only checkout with fresh history. Do not
stage or publish the research parent tree. Prepare a
fresh source snapshot with:

```powershell
python tools/source_bundle.py --output local/unity-source-review.zip
```

This creates a local ZIP and per-file SHA-256 manifest using explicit source
locations and extensions. It excludes game data, decompiled C files, local
settings, build output and Git history; linked source paths and oversized files
are rejected. It refuses to replace an existing bundle. Review the actual files
and provenance before publication; the allowlist is not a content/license audit.

No project license has been assigned. Third-party reader
provenance is documented in [FORGE_REUSE.md](docs/FORGE_REUSE.md) and findings;
do not assign blanket licensing to third-party material without review.
