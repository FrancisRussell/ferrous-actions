Running `npm run build` followed by `du -b dist/lib_bg.wasm`. All builds are
release and have `wasm-opt` applied to them.

Commit: `6ac38a64a969d92a5187bc1754b4c52c700eed31`
No link-time optimization: 1200380
`lto = true`: 1170489

All following builds have `lto = true` in `Cargo.toml`.

Commit: `6ac38a64a969d92a5187bc1754b4c52c700eed31`
`opt-level = "s"`: 946514
`opt-level = "z"`: 925461

Setting `opt-level = "s"` due to concerns about potential speed costs.

Commit: `d0b4d8a8423f2cd1c934a9acbb63874e2433db39`
Monomorphizing `dir_tree::apply_visitor_impl`: 940065

Switch to postcard for dependency file (cache group list) format: 938404
