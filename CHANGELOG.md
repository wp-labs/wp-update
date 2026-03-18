# Changelog

## [Unreleased]

### Added
- Initial workspace split from `warp-parse` for `wp-self-update` and `wp-inst`.
- Merged the former `wp-update-core` crate into `wp-self-update` internal modules.
- Added `docs/orion-accessor-evaluation.md` to record the `orion-accessor` adoption assessment.

### Changed
- Renamed the installer CLI package and binary from `wp-installer` to `wp-inst`.
- Simplified installer source selection by replacing product-specific CLI selection with `--base-url` and `--local-root`.
- Release workflow now builds and publishes `wp-inst` binary artifacts for tagged releases, including dry-run coverage, without wrapping single-file binaries in `tar.gz`.

### Fixed
- Artifact downloads now preserve raw response bytes instead of allowing HTTP auto-decompression to corrupt `.tar.gz` release assets during self-update.
- Added a regression test covering mislabelled gzip-encoded artifact responses.
- Self-update now accepts both `.tar.gz` archives and single raw binary artifacts for single-binary install targets.
