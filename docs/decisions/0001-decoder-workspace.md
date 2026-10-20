# ADR-0001 — Runtime decoder workspace

2026-10-18. Accepted for the runtime decoder work.

Create only acu_assets initially: the user prioritizes runtime asset support.
Do not add empty simulation/application crates. Use Rust 1.99.0, edition 2024,
on the already-installed x86_64-pc-windows-msvc toolchain. Reserve exact Bevy
0.19.1 in workspace dependencies; no Bevy build is needed for CPU-only parsing.
Its official tagged Cargo.toml states rust-version 1.95.0, below the selected
compiler. The official setup guide currently illustrates Bevy 0.19 and stable
Rust. Rendering/platform dependencies remain to be verified at app creation.

Sources checked on this date:
- https://raw.githubusercontent.com/bevyengine/bevy/v0.19.1/Cargo.toml
- https://bevy.org/learn/quick-start/getting-started/setup/

Use lzokay 2.0.1's bounded pure-Rust decompressor to test the LZO1X hypothesis;
the upstream API accepts an output slice and returns the written size. Enforce
the expected output length in our framing code. The dependency is MIT licensed;
retain Cargo.lock and upstream notices through normal dependency distribution.
No game bytes or third-party source are vendored by this decision.
https://docs.rs/lzokay/2.0.1/lzokay/decompress/fn.decompress.html

Use exact direct-dependency versions and a lockfile. Do not stage the parent
repository to commit it: this project remains an untracked subtree there.
