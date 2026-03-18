# Changelog

## [Unreleased]

### Added
- Initial workspace split from `warp-parse` for `wp-self-update` and `wp-installer`.
- Merged the former `wp-update-core` crate into `wp-self-update` internal modules.

### Changed
- Release workflow now builds and publishes `wp-installer` binary artifacts for tagged releases, including dry-run coverage.
