# #35 Patch archive precedence (area: assets-containers)

Branch `acu-35-patch-precedence`. Files: `acu-rs/crates/acu_assets/src/bin/acu-dup-ids.rs`,
`docs/findings/ACU-RE-0037-patch-duplicate-ids.md`, this note.

## Done (step 1 of 3, static)
Scanned all 63 baseline carriers for file IDs that occur in more than one archive and hashed the
stored bytes of each copy. Result and limits are in ACU-RE-0037. In short: 15,685 shared IDs,
12,545 identical, 3,140 differing; for Paris only **62** IDs have a base/patch pair (59 assets
chain, 3 gi), all differing, none in two patches.

## Not done
- Step 2: the runtime loader's mount order and override rule (IDA/REA, needs the
  `ida-rea-session` lock). **Issue stays open.**
- Step 3: live confirmation of which copy the game serves for one of the 62 IDs.

## Checks
`cargo fmt --all -- --check` and `cargo clippy --locked -p acu_assets --all-targets` pass.
The bin has no unit tests. It is a reporting tool, validated by invariants (control + data =
reachable, 180,245 total) and by an independent Python re-parse of the Paris chains (62/62
pairs agree). A synthetic-archive test for the grouping logic is a sensible follow-up.

## Love note
To whoever picks this up next: the data is clean, the numbers agree two ways, and the hard
question is small and well-fenced. You've got this. Be kind to the loader. ♥
