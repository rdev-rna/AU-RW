# Animation database

`tools/animation_catalogue.py` builds a searchable SQLite metadata snapshot from
the runtime asset coverage JSONL. The database lives in ignored `local/`; it
contains original asset names and identities and is not part of source bundles.
Python's standard library is sufficient. It does not extract animation payloads
or add an extracted-file dependency to the viewer.

```powershell
python tools/animation_catalogue.py build local/coverage-typed-full2-20261018/entries.jsonl local/animation-matcher-20261019/animations.sqlite
python tools/animation_catalogue.py stats local/animation-matcher-20261019/animations.sqlite
python tools/animation_catalogue.py query local/animation-matcher-20261019/animations.sqlite --role walking --limit 10
python tools/animation_catalogue.py query local/animation-matcher-20261019/animations.sqlite --name Sprint_3-Loop
python tools/animation_catalogue.py query local/animation-matcher-20261019/animations.sqlite --name Ladder_ClimbCycle
python tools/animation_catalogue.py query local/animation-matcher-20261019/animations.sqlite --file-id 000000016737c2c4
python tools/animation_catalogue.py query local/animation-matcher-20261019/animations.sqlite --actor Arno
```

The initial snapshot covers every matching resource envelope in the supplied
scan, without the 20,000-name cap used by the earlier local discovery probe.
This is not proof that every animation in the install has been discovered: scan
failures, opaque packages and animations stored under other resource classes
remain outside this index. `metadata` records the exact input SHA-256, line and
resource counts. The source scan must be regenerated when install inputs change.

Initial local snapshot (2026-10-19): **110,447 Animation occurrences, 37,803
distinct file IDs**, across 180,245 input records containing 3,289,051 resource
envelopes. The other three indexed classes have no standalone envelopes in this
scan; their embedded objects still require decoding. Source JSONL SHA-256:
`7e8a2c0a11fbf316ae5fd9acd0b58571bb3847c1090af7c3e7f2871ca3de3646`.
Role hints include 2,302 walking, 453 sprinting and 991 climbing occurrences;
these include archive duplicates and do not establish usable player clips.
The initial snapshot had zero verified bindings and zero decoded track rows.
The locomotion audit now imports 261,388 tracks from 3,317 decoded animations
across DataPC.forge entries 110/928/1518, with 6,365 explicit decode failures.
Verified actor bindings remain empty. See [finding0020](findings/ACU-RE-0020-idle-leg-helper-audit.md).

```powershell
cargo run --locked -p acu_assets --bin acu-animation-audit -- <archive> 928 <audit.jsonl>
python tools/locomotion_audit.py <database> <audit.jsonl> <candidates.csv>
```

Run Cargo from `acu-rs/` and Python from the project root. Repeat for other
packages. The importer checks archive/entry/resource identity plus stored,
decoded and payload hashes and payload ranges against the metadata snapshot;
a mismatch rolls the import back. Supplemental `decode_results` and
`track_metrics` tables retain failures, duration, source audit identity and
per-track change metrics. Existing annotations are preserved. A repeated audit
replaces only the matching occurrences' previous decoded-track/audit records.
The exported broad locomotion/traversal/idle name inventory currently contains
17,897 occurrences / 5,662 IDs: 1,340 decoded, 2,751 unsupported and 13,806
not audited. It includes NPC/shared/cinematic candidates, with ownership unknown.

| Table | Purpose |
| --- | --- |
| `occurrences` | One archive/entry/resource occurrence; file ID, name, class, original entry offset and length, stored/decoded/payload hashes, decoded payload range and source line. |
| `role_hints` | Searchable roles inferred from names: walking, sprinting, climbing, idle, traversal, jumping, combat, facial and cinematic. Every row explicitly says `name_hint`. |
| `bindings` | Actor, role, rig, track-data, bone, event or other reference associations, with evidence kind, provenance and notes. |
| `tracks` | Decoded track index, bone hash, channel, codec, key count, duration and byte range, with decoder identity and provenance. Track offsets are relative to the Animation payload; lengths include the serialized track header/time/value bytes. |
| `decode_results` | Supplemental audit status/error and duration, with source audit hash and exact line provenance. |
| `track_metrics` | Supplemental maximum decoded component change from the first key; useful to detect a static pose, not proof of correct interpolation or actor ownership. |
| `metadata` | Input identity, schema scope and snapshot counts. |

Resource classes indexed are Animation (`0fa3067f`), AnimTrackData (`0181efe8`),
AnimTrackDataMapping (`653caa76`) and AnimTrackEvent (`41682213`). These reflect
the existing resource-envelope classification and animation schema discovery;
indexing an envelope does not certify its payload is decoded. Embedded objects
inside a clip are not separate envelopes and will require track decoding.

File IDs remain hexadecimal TEXT, preserving IDs above signed/JavaScript integer
ranges. Duplicate IDs are retained as separate occurrences; no patch winner is
invented. An occurrence number is local to a snapshot: use archive, entry,
resource index and payload hash when recording durable evidence. Payload ranges
are offsets in the decoded entry, not physical compressed-archive offsets.

Actor ownership starts **unknown**. Even a clip name containing `Arno` does not
establish that it is exclusive to him or selected by his animation graph. Shared
player, NPC, cinematic and equipment associations must follow decoded references
or runtime observations. The `--actor` filter excludes manual hypotheses.
An empty result means no evidenced binding has been recorded, not that the actor
has no animations. Bindings can be added after investigating a specific occurrence:

```text
python tools/animation_catalogue.py annotate <database> <occurrence-id> actor <actor-name> --evidence-kind decoded_reference --provenance "<finding and exact reference location>" --note "<sharing or selection limitations>"
```

Use `manual_hypothesis` for unverified ideas, `decoded_reference` for an actual
reference chain and `runtime_observation` for a recorded runtime association.
Always include reproducible provenance; these labels are researcher assertions,
not an automatic verifier. Never promote a filename hint to a decoded reference.
Rebuilding refuses an existing database so accumulated annotations are preserved;
build a new snapshot and explicitly reconcile associations by stable identities.

Validation: `python -m unittest discover -s tools -p test_animation_catalogue.py -v`
covers duplicate IDs, integer preservation, atomic failure, overwrite protection,
unknown ownership, literal search filters, invalid ranges and foreign keys.
