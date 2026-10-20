# ACU-RE-0003 — First runtime TextureMap variant

2026-10-18, Codex. State: partially established for the inline 2D variant.

Primary game-byte fixture: DataPC.forge, entry 1, ID 0000000000000008,
offset 672191, stored length 164, stored SHA-256
9d647e486d3f2287821d5173d8da2190929d6bec7908c447b51a7ccd8679ac5b.
The decoded resource-stream hash is
9fe2ca94dd611b45eb33a25d6c29ef104dfd3aeb29546f2649eb9a790e73b9a0,
independently checked with native liblzo (see ACU-RE-0002).
No native game-code addresses or pseudocode identity claims are made.

Format facts checked against the identified AnvilToolkit 1.3.7.0 DLL, SHA-256
940374775553172b06d55537c4db78048bc3206b9b2f72bcc61bacfdcdb57636,
REA managed-static identity ev_01f1308aac08023bb993cc29cf977fa03e4a7076b1985e3d9d02ee25a0c7db1a.
Local ilspycmd 8.2.0 decompilation of TextureMap.Read,
CompiledTextureMap.ReadDefault, ScimitarClass.Deserialize,
Extensions.ReadClassID and PixelFormats.PostUnity/GetFormat is retained in
ignored local/reference-atk/. These are third-party reader observations,
corroborated against ACU bytes, not proof of native game execution. No toolkit
implementation source is imported into the rewrite.

Observed first fixture: resource type a2b7e917, name Default Diffuse Texture,
root payload begins byte 1, u64 object ID 8 and u32 matching type hash.
The Unity TextureMap field sequence is dimensions, engine format, texture kind,
gamma, mip count, map type, category (all u32), dynamic/ignore-skip bytes,
u32 user category, byte/u64 top-mip reference, byte inline-compiled marker,
compiled u64 ID/u32 class hash 13237fe9, then compiled platform/SDK,
dimensions, mip count, format, kind, gamma, top-mip size, total size and alignment
(u32), and a length-prefixed pixel block.

The first fixture has 4x4x1 dimensions and 64 BGRA8 pixel bytes, all pixels
50 50 50 ff. Both stored mip counts are zero; the reference reader treats zero
as one. This interpretation is checked by the exact pixel-byte count, rather
than assigning zero levels and ignoring bytes.

Bounded initial implementation: only this inline 2D layout with no external
top mip; validate root/class IDs, dimension/format consistency, supported format,
entire payload consumption and every mip's independently computed byte range.
Preserve gamma/category/platform fields. Expose borrowed native pixel slices for
runtime renderer upload, not an extracted DDS conversion step. External mips,
arrays, cubemaps, other wrappers and new formats must fail explicitly until
separately supported; descriptor-only parsing is not full texture success.

Implemented in acu_assets/src/texture.rs. The completed 63-archive audit validates
39,302 of 65,466 TextureMap records as inline 2D textures, including all pixel/mip
spans (1,447 in DataPC.forge). Other variants remain unsupported. The install-dependent
test checks the default diffuse texture's dimensions, format and all 16 pixel
values, while authored fixtures reject every truncated prefix, wrong identity,
format/size corruption and unresolved external mip references. No GPU upload,
rendered appearance or full texture-family coverage is claimed.
