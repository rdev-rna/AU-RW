# Validation

## Preparation

`python tools/acu.py check` validates required workflow files, configuration
shape and local Markdown links. `doctor` checks configured reference inputs and
reports tools without installing anything. `inventory` writes local provenance;
it does not validate a parser. Missing optional tools are reported separately.

## Rust and formats

Runtime decoder commands and library usage are in [acu-rs README](../acu-rs/README.md).
The full-install coverage command reads entries in memory and emits reports only.
It intentionally exits nonzero while any input or typed decoder remains unsupported.
This is a coverage failure, not a build/test failure. Preserve each run's report;
do not overwrite or compare a partial scan as though it were complete.

Once `acu-rs/` exists, run the four Cargo gates in `AGENTS.md`. Pin versions and
keep the lockfile. Install-independent tests should use small authored fixtures
and independently established expected results. Cover bounds, truncated input,
unsupported variants, integer overflow, cyclic tables, decompression limits,
duplicate IDs and the selected patch-resolution policy.

Separate actual install tests from synthetic tests. An install vector records
archive SHA-256 or byte-range identity/hash, file ID, source offset, framing,
decoder identity, expected decoded length and SHA-256. Compare decoded bytes
against an independently verified extraction or native routine, not just two
ports of the same guessed algorithm. Shared-port agreement is useful but weaker.
Never silently skip a requested install test and report a pass.

## Simulation and presentation

Replay fixed input sequences with explicit initial state, tick interval, RNG
seed, asset revisions and expected transitions. Check displacement, orientation,
contacts and state transitions with justified tolerances. Include a negative
control where useful: a deliberately wrong conversion should fail comparison.

For each visible change create a capture manifest using
`docs/templates/CAPTURE.json`. Record source revision (or source digest for
uncommitted work), executable hash, build command/profile, launch arguments,
scenario, controls, resolution, camera, seed/ticks, asset identity and outputs.
Capture freshly built code and inspect the result. Label synthetic content.
Keep original and rewrite captures separate and match scenario/camera where
possible; record differences in timing, mods, settings or data.

The install contains ACUFixes and loader DLLs. Record active configuration and
hashes when collecting original runtime evidence. The user explicitly authorizes
runtime execution and reversible hook/loader deployment and configuration changes
without further confirmation. Preserve replaced files and document restoration.
For live DLL-hook comparisons, record target RVA/module base, hook identity,
thread/tick context, inputs, native/Rust outputs and tolerances. Distinguish
ordinary gameplay from injected probes and record instrumentation overhead where
relevant. See `WORKFLOW.md` for the live comparison loop.

A completion report states what passed, what failed, what was unavailable and
what remains unverified. No build, screenshot or sample establishes full parity.
