## How does `rustup` process package manifest files?
- https://github.com/rust-lang/rustup/blob/b3d53252ec06635da4b8bd434a82e2e8b6480485/src/dist/component/package.rs#L82

## How are crate files named in the cache?
- https://github.com/rust-lang/cargo/blob/f6e737b1e3386adb89333bf06a01f68a91ac5306/src/cargo/sources/registry/download.rs#L20

## How do registries get named in the cache?
- https://github.com/rust-lang/cargo/blob/f6e737b1e3386adb89333bf06a01f68a91ac5306/src/cargo/core/source/source_id.rs#L560
- https://github.com/rust-lang/cargo/blob/f6e737b1e3386adb89333bf06a01f68a91ac5306/src/cargo/sources/registry/mod.rs#L549

## Issues with `rustup` and `cargo` acting as package managers
- https://kornel.ski/rust-2019
