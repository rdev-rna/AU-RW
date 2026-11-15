# LAN co-op testbed (ACU-RE-0090)

Scripts that put two ACU games on one PC into a working LAN co-op session, without Ubisoft services. Evidence and
the reasoning behind each step are in `docs/findings/ACU-RE-0090-network-mode-flag.md`, runs 1–15. Everything
writes process memory only; no game files or code bytes change.

Rules (same as `tools/native_probe/README.md`):
- the custom loader is active (`python tools/loader_swap.py use custom`);
- always launch with an explicit `--fps-cap` of 300 or less;
- close only with `capture_run.py --close <pid>`;
- never remove or hot-swap the shim on a running game;
- at the end, `python tools/loader_swap.py use original`.

`ACU_MAPPED_IMAGE` overrides the mapped-image path the hook scripts use for expected bytes.

## Steps

1. **Game A, the joiner, on the original ports.** Run `lanflag.py <out.json> --timeout 120` in the background.
   It writes network mode 1 (LAN) into `dword_144E52380` as soon as the game unpacks. Then start the game:
   `capture_run.py <dir> --direct --until world --fps-cap 60 --cpu-cores 0-3 --allow-user-input`.
2. **Game B, the host, ports moved by 10.** Run `lanoffset2.py <out.json> --port-offset 10` in the background.
   It writes LAN mode, then rewrites the port offset byte `byte_145218C33` until Storm's config shows 3084.
   Start the game with `capture_run.py ... --direct --until world --fps-cap 30 --cpu-cores 4-5`.
3. **Something to join.** `privhost.py <pidB> <out.jsonl> [--mission <id>]` makes the map's own Play-in-Private call
   (`0x14250CD00`) with the probe's `CALLAT`, without the UI. The default mission is `000000104ccb592a` (`COOP_RB050`,
   DEFEND Danton). The host enters `PRIVATE_MATCH_HOST` (ACU-RE-0092). In runs 10–13 the host chose Private from the
   map by hand instead; only that lobby was seen answering discovery.
4. **Discovery in game A.** `lancall.py <pidA> <out.jsonl>` calls the LAN matchmaker's slot 4 once with the
   probe's `CALLAT`. `lanrec.py <pidA> <pidB>` then lists the stored records.
5. **Join.** In ACU-RE-0092 discovery alone made the joiner follow the host into the mission about 45 s later.
   Otherwise, `lanjoin2.py <pidA> <pidB> <out.jsonl>` calls ACU's `JoinSession` with the stored record.
6. **Watch.** `coop_state.py <pid> [pid]` takes a read-only snapshot. `dropwatch.py ... --watch-only` hooks the
   leave, kick and disconnect slots and logs session changes.

Known pitfalls:
- leaving and then rejoining one second later crashed the joiner (run 14);
- a relaunched joiner at the same IP and port is re-adopted by the host as its old peer (run 15).

## Ownership census

`rolecensus.py <out.json> <pid>...` lists every entity within 120 m of the player whose role bits (`entity+212`) are replica or whose kind is set, with its asset name. Run it before and after moving the players, then diff by `file_id` to see ownership migrate (ACU-RE-0092 observations 18–19).
