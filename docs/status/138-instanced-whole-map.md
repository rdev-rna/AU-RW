# 138 - Instanced whole map: one entity per part

Issue: #138 (`type:slice`, `area:world-data`). Branch: `acu-137-whole-map`.
Follows `docs/status/137-whole-map.md` (#137).

## The measured blocker

The per-placement renderer created one entity per placement. The whole map is
486,454 placements at the piece limit that still fit, and the full 8,330-cell
map is 1,064,087 placements -> 1,064,087 entities. Halving the placements cut
triangles by only 6.3%, so the cost was per-entity draw/submission overhead, not
geometry.

The per-placement path ran out of memory at capture on every combination tried:
3840x2160, 2560x1440 and a 250,000-piece limit, each dying at frame 60 with
~20 GB RSS and 8 GB VRAM in use.

## The instanced result

`--map-instanced` draws one entity per (piece, part) with an instance buffer
(`acu-rs/crates/acu_bevy/src/instanced.rs`, shader
`acu-rs/crates/acu_bevy/src/shaders/map_instanced.wgsl`). The whole map is
1,064,087 placements in 55,644 entities (19.1x fewer entities), and a 4K
capture of the whole map completed in 17.5 minutes with no OOM, where the
per-placement path had died at every resolution and piece limit tried.

Streaming keeps the same model: `Streaming::cell_instances` stores each cell's
placements and `rebuild_instanced_parts` recomputes only the pieces a changed
cell touched, so a streamed cell adds or removes exact instance lists.

## Single-cell oracle

Cell `--map-cell 4884` decoded through the cache is byte-identical to the
direct decode. Rendered through the instanced path its silhouette intersects
the per-placement path at IoU 1.00000, and the frame holds 17,659 distinct
colours with the per-part diffuse texture and alpha mask bound (the
per-placement path of the same cell is the reference).

## Window result

`--map-stream 150 --map-player --map-instanced` in a window holds 31-55 fps
while 95 cells are streamed in and out. Screenshot: `local/play-window.png`
(not committed, `local/` is ignored).

## Known defects

These are open, not fixed:

- hollow facades - building shells render without their back faces/interiors;
- roofs and whole chunks float detached from their walls;
- ground is fragmented, with gaps between streamed cells;
- the non-instanced streaming path renders a near-flat surface: 2,286 distinct
  colours against 31,289 for the instanced path, so the per-placement streaming
  path loses most of its geometry;
- 6-7 meshes are still undecoded, reported as
  `unsupported vertex format 2 stride 40`.

Culling is NOT the cause: the instanced pipeline uses `cull_mode = None`
(`acu-rs/crates/acu_bevy/src/instanced.rs:364`), so no face is dropped by
rasteriser state. The remaining geometry defect is in the streaming/group
placement logic - which cell's placements land in which piece bucket, and with
what root matrix - and that is the next thing to investigate. It is not fixed
by this slice.

## Verification commands

```
# cell oracle
acu_bevy --map-cell 4884 --map-instanced --capture local/cell-inst.png
acu_bevy --map-cell 4884 --capture local/cell-ref.png
# whole map
acu_bevy --map-cells local/paris-cells.json --map-instanced --capture local/map-4k.png
# window
acu_bevy --map-stream 150 --map-player --map-instanced
```
