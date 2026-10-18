# ACU runtime asset readers

Read original Forge files directly; requested data is decoded in memory. The
library never extracts assets and needs neither AnvilToolkit nor an extracted
asset directory. Configure the install through the caller or `ACU_GAME_DIR`.

```rust,no_run
use acu_assets::{Limits, Result, forge::Forge};

fn example() -> Result<()> {
    let root = std::env::var("ACU_GAME_DIR")?;
    let mut archive = Forge::open(std::path::Path::new(&root).join("DataPC.forge"), Limits::default())?;
    let package = archive.read_package(1)?;
    let texture = package.texture(0)?;
    // Native pixels and mip ranges are borrowed from the in-memory package.
    assert_eq!((texture.width, texture.height), (4, 4));
    assert_eq!(texture.pixels.len(), 64);
    Ok(())
}
```

Entry indices above identify a validated fixture only. Real callers should use
stable file IDs plus explicit archive provenance, retain the package while its
borrowed data is used, and resolve patch/reference ambiguities before loading.
No filename-based patch precedence is assumed. The TextureMap reader supports
inline 2D/cube/volume variants and typed descriptors without inline pixels;
external mip resolution and other asset readers remain incomplete. Skeleton
coverage includes the male/base humanoid variant, four modifier schemas and
bounded IK/COM/Reflex inputs (ACU-RE-0014). No solver or animation playback yet.

Run from this directory (PowerShell):

```powershell
$archive = Join-Path $env:ACU_GAME_DIR 'DataPC.forge'
cargo run --locked --bin acu-probe -- $archive --list 4
cargo run --locked --bin acu-probe -- $archive --fid 0000000000000008
cargo test --locked --test install -- --ignored
cargo run --locked --release --bin acu-scan -- ../local/coverage
```

`acu-probe` outputs bounded identities/descriptors and hashes, never an asset
copy. It accepts an entry index or an unambiguous ID within a selected archive.
`acu-scan` explicitly audits all top-level and DLC Forge archives in memory and
writes only JSON/JSONL coverage reports. It exits with an error when support is
incomplete. Unsupported metadata extensions and opaque resource types remain
failures for full coverage; envelope/decompression success is reported separately.
Backups/extracted copies are excluded, other carrier types are not yet covered.
See ../docs/ASSET_COVERAGE.md for the all-assets acceptance gate.

`acu-mesh ARCHIVE ENTRY RESOURCE... --skin --out=REPORT` emits mesh bone
matrices, per-part hardware palettes and decoded joint/weight bytes for focused
diagnosis. Reports include stored/decoded package hashes and payload ranges;
keep original-derived reports under ignored `local/`. The independent
`../tools/skin_audit.py` compares a report to the identified skeleton diagnostic.
See finding ACU-RE-0015 for commands, matrix alternatives and unresolved bones.
This audit does not activate skinning or select additive rigs.

`acu-mat ARCHIVE ENTRY RESOURCE --hex --out=REPORT` and
`python ../tools/build_table_probe.py REPORT --output=PARSED` diagnose selected
reference/handle BuildTables (finding ACU-RE-0016). Nonempty PropertyPaths and
unsupported property types fail explicitly. The head-rig install test
`cargo test --locked -p acu_assets --test skeleton original_arno_head_rig -- --ignored`
requires `ACU_GAME_DIR`; it verifies 134 bones and 106 typed wrinkle inputs.
It also verifies 147 poses and 2,526 packed bone records. The standard test suite
includes malformed-input vectors for both wrinkle and pose-group boundaries.
These are serialized inputs and references, not native selection or skinning.
