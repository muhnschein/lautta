# cpp_common 0.5.11 (patched)

Copy of the crates.io release `cpp_common 0.5.11` (MIT OR Apache-2.0, © Nika
Layzell, Olivier Goffart and the rust-cpp contributors,
https://github.com/mystor/rust-cpp).

One change: `file_hash()` hashes only the crate's build folder name
(`<crate>-<hash>` above `OUT_DIR`), not the whole `OUT_DIR` path. The hash
names the `rust_cpp_callbacks<hash>` symbol; from the full path, a build in
another folder named it differently, the linker laid out the GOT in another
order, and the binary was no longer byte for byte the same. Still unique per
crate in a link: cargo gives each crate its own build folder.
