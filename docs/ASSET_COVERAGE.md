# Runtime asset decoder acceptance

The current user objective is successful parsing of every installed ACU asset,
including models, maps, animations and textures, for direct runtime use in the
Rust/Bevy rewrite. Extraction is not the product: index original archives and
decode requested entries in memory. Extracted folders are optional comparison
evidence only and must never become a runtime dependency. Validation scans emit
reports and identities, not bulk asset copies.

Success requires an enumerated input manifest, validated archive boundaries,
successful entry decompression, resource record parsing, and typed decoding of
every supported asset's content and references. A wrapper around opaque bytes,
an unimplemented type, a skipped variant, or missing data is not a parsed asset.
Report these stages separately, including errors with archive, offset, file ID,
type and reproducible hashes. Do not claim 100% from a sampled scan.

Start with all original top-level and DLC Forge archives under the configured
install. Exclude extracted folders, backups and copies from that denominator,
and report the exclusions. Other runtime asset carriers (audio, video, etc.)
need their own explicit inventory and parser gates before all-assets completion.
Modified/corrupt install bytes stay visible as failures or separately evidenced
variants; do not reinterpret them as success merely to raise the coverage rate.

Implementation sequence: bounded archive/index and in-memory entry decode;
resource-envelope/type inventory; texture, mesh/skeleton, animation and world
decoders guided by actual variants; complete-install regression with zero
unsupported types, skips or parse failures. Unknown fields remain documented;
opaque data is not full typed coverage. Rendering and animation inspection are
additional integration checks once CPU decoding is established.
