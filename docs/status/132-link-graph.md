# #132 Asset link graph and distributed index (area: tooling)

Branch `acu-link-graph` (stacked on #122 and #130). Index release: `asset-index-2026-10-22.1`.

## Done
- `acu-links` (Rust): scans every resource payload for 8-byte values equal to a known file ID,
  with a same-size fake-ID control. Calibration: from 25 bits up, fakes matched ~0.002% as often
  as real IDs; below that they match as often (`kind='weak'`).
- `asset_graph.py` merges/walks candidate links; `annotate_asset_db.py` adds `type_label`
  (observed-decode / crc-name / unknown), `loose_file` and the baseline flag.
- `audit_asset_db.py` checks for holes against fresh parses; `package_asset_index.py` audits,
  scrubs paths and packages; `index_status.py` downloads the release with a progress bar.
- Paris was rescanned: the first scan read a different `DataPC_ACU_Paris.forge` (3,850,403,840 B;
  now 3,770,744,832 B, identical to Backups). Older Paris counts describe the old file.

## Index 2026-10-22.1
63 archives, 180,245 entries, 3,289,299 resources, 12,575,691 candidate links, 484 types
(3,012,843 resources under a confident label), 422 loose files. Audit: 22 checks, no holes.
Archive 559,120,359 B, SHA-256 254364665c08...; unpacked 1,910,329,344 B.

## Not done
- Typed links (Entity/LODSelector/BuildTable refs) replace candidates only per decoder (#135, #39).
- 442 of 484 types are unlabelled; no animation-graph or shader type identified.
- Arno pilot walk vs ACU-RE-0013/0016 hand-traced chains not yet run.
- DeepSeek/opencode following the AGENTS.md instruction is untested.
