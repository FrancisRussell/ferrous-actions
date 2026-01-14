# Changelog

## Unreleased

* Remove use of `@actions/exec` and bind to NodeJS's child process
  functionality directly. Although the `exec` library was useful, the inability
  to preserve `stdout` and `stderr` as separate streams was unacceptable for
  processing compiler generated JSON while letting human-readable debug output
  through untouched. This fixes issue #271.
* Bump `actions/io` from 1.1.3 to 2.0.0.
* Switch to `regex-lite` from `regex`.
* Bump `simple-path-match` to remove dependency on `syn` version 1.
* Remove unused `derivative` dependency.

## v0.1.0-beta.3

* Use `getrandom` crate for random number generation and remove hacky
  `Math.random()` based random number generator.
* Expose and document the node.js and GitHub Actions Toolkit bindings.
* Improve path-to-glob conversion.
* Add changelog.
* Document `components` option for `install-rustup` in `README.md`.
* Use new `lookupOnly` option to simplify cache peeking code.
* Bump `@actions/cache` due to v3 deprecation.
* Bump various NPM and Rust dependencies.
* Add mistakenly omitted `override` option to `action.yml`.

## v0.1.0-beta.2

* Add work-around to enable consistent relative paths for cache keys and the
  paths used in the generated tar files.
* Convert paths to globs to enable cross-platform cache entry matching.
* Get cross-plafform sharing of cargo-home caches to work.
* Add tests for `PushLineSplitter`.

## v0.1.0-beta.1

* Ignore blank lines when reading annotations.
* Add foldable groups for cache load and save operations.
* Add `PushLineSplitter` work-around for line-splitting of process output on
  Windows (https://github.com/FrancisRussell/ferrous-actions-dev/issues/81).
* Add multi-platform (Ubuntu, Windows and Darwin) integration tests.
* Document naming of CI steps in `README.md`.
* Add origin platform of cross-platform cache entries to name.

## v0.1.0-alpha.3

* Migrate to new `base64` crate API.
* Reduce WASM size.
* Use `postcard` for state serialization.
* Enable `ncc` option to minify JavaScript.
* Monomorphize `dir_tree::apply_visitor_impl`.
* Add conversion from `&String` to `Path` and remove many calls to `as_str()`.
* Add unit tests for node.js bindings.
* Eliminate use of `noop-stream` NPM package.
* Use platform-agnostic paths for group and cache entry names.
* Add group contents hash to cache key name for easier debugging.
* Add the toolchain used to `cargo install` build artifact cache entry names.
* Avoid races when uploading `cargo install` build artifacts cache entries.

## v0.1.0-alpha.2

* Document how to use action.
* Avoid races when uploading new versions of cargo-home cache entries.
* Separate CI job dependency lists from actual cached content to allow CI jobs
  with different dependencies to share cached items but not fight over what
  should be cached.
* Add support for rustup ‘override’ functionality.
* Rename `targets` option to `target` to match `actions-rs`.

## v0.1.0-alpha.1

* First tagged version.
