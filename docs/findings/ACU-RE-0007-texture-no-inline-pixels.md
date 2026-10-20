# ACU-RE-0007 — Texture resources without inline compiled pixels

2026-10-18 / Codex / ACU-005. State: partially established.

## Provenance

Original archive `D:/Games/Assassin's Creed Unity/DataPC.forge`, entry 1478,
file ID `00000001a36b095f`, archive offset 293443308, stored length 144.
Stored SHA-256:
`26a4a401fa74f3fa69712d300779f2063eacd907379c63f42c508b59d4e6b69a`.
Decoded resource stream length 97, SHA-256:
`71a2fad1bf9aafcf3333bef5988eb43dd746277d6de5377472059e95728d153a`.
Resource payload is stream range 32..97 (65 bytes), SHA-256:
`5a9408727f2ccdb5c8cef15ee5b180247509fded358b39021a6f4ad0e9bd4fc8`.
Resource type `a2b7e917`, name `DBG_FontTexture_Map`.

No native executable VA/RVA, game execution or pseudocode correspondence is
claimed. Archive bytes were checked independently by inline Python/ctypes using
native `lzo1x_decompress_safe`, same oracle DLL and provider identity as
ACU-RE-0005. Equal stored/decoded blocks are copied directly; other blocks use
native LZO. The Rust install test checks the independent stream hash above.

Third-party reader identity is the AnvilToolkit 1.3.7.0 DLL recorded in
ACU-RE-0003/0005, SHA-256
`940374775553172b06d55537c4db78048bc3206b9b2f72bcc61bacfdcdb57636`,
REA identity `ev_01f1308aac08023bb993cc29cf977fa03e4a7076b1985e3d9d02ee25a0c7db1a`.
Retained `local/reference-atk/TextureMap.cs` lines 342–399 read a byte before the
compiled block: zero reads the block, any other byte reports no data. This does
not establish a reference ID after a nonzero marker. A focused
`ilspycmd -t AnvilToolkit.FileTypes.AnvilNext.Materials.Texture.CompiledMip`
against the same explicit DLL showed its supported-game list excludes Unity;
that layout was therefore not imported as Unity evidence. Tool: ilspycmd
8.2.0.7535. Original game and ATK files remained read-only.

## Result

Observed complete 65-byte payload: normal wrapped TextureMap identity; dimensions
2048x2048x1, engine format 9 (A8), kind 1 (2D), gamma 0, one mip, map type 11,
category 5, dynamic byte 1, ignore-skip byte 0, user category 8, top-mip tag 0
and null u64 top-mip reference, final compiled marker 3. There are no following
bytes, compiled class identity or pixels. The previous error text
`unsupported referenced compiled texture` incorrectly suggested a reference:
the observation establishes absence of inline data, without a reference target.
The name and dynamic flag suggest a runtime font atlas placeholder; native
allocation/update behavior remains unknown.

`texture::parse_resource` now returns either `TextureResource::Resident` with
validated native pixel spans or `TextureResource::NoInlinePixels` with a fully
consumed typed descriptor. The latter has no pixel buffer and must not count as
an uploadable texture. Only the observed nonzero marker 3 with dynamic=true and
a null top-mip reference is accepted; other marker/flag combinations fail.
`texture::parse` retains its strict CPU-usable-pixels contract and rejects this
resource. External nonzero mip IDs still fail explicitly; no external mip or
standalone compiled texture was resolved in this slice.

## Diagnostic scope and validation

Streaming the existing completed JSONL report, without decoding archives again,
counted the old TextureMap errors: 26158 dimensions/kind/mips, four compiled
dimension mismatches and two nonzero compiled markers; no external top-mip error
had been reached. There were 26110 distinct failing payload hashes. Searching
the same report found no resource envelopes with types `13237fe9`, `85b04e20`
or `1d4beba3`; the latter two were lookup candidates only, not established Unity
resource identities. This absence does not prove those formats never occur.
Sampling four Paris failures found one 32-cubed LUT and three 128-square BC1
cubes, consistent with the previously implemented cube/volume slice. These
diagnostics do not provide an updated all-install success count.

From `acu-rs/`, with `ACU_GAME_DIR` set:
`cargo test --locked -p acu_assets --test texture -- --include-ignored` passed
seven tests: five authored and two explicitly install-dependent tests covering
seven original archive vectors in total. The placeholder tests reject every
truncated prefix, trailing bytes, unknown markers, invalid booleans and unresolved
external references, and verify both resident and pixel-less API variants.
`cargo clippy --locked -p acu_assets --all-targets` passed. Tests without the game
environment leave install checks explicitly ignored/unavailable.

Reproduction: set `ACU_GAME_DIR`, run the command above; use `acu-probe`
`DataPC.forge 1478` for the exact envelope identity. Parent package/scanner
integration must preserve the distinction between a typed descriptor and usable
pixels. No game execution, GPU upload, native rendering or all-asset parity claim.
