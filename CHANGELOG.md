# Changelog

## [Unreleased]

## [0.1.6] - 2026-03-19

### Added
- Initial workspace split from `warp-parse` for `wp-self-update` and `wp-inst`.
- Merged the former `wp-update-core` crate into `wp-self-update` internal modules.
- Added `docs/orion-accessor-evaluation.md` to record the `orion-accessor` adoption assessment.

### Changed
- Renamed the installer CLI package and binary from `wp-installer` to `wp-inst`.
- Simplified installer source selection by replacing product-specific CLI selection with `--base-url` and `--local-root`.
- Added GitHub latest-release install mode via `wp-inst --github <repo> --latest` for single-binary tools.
- Added GitHub tag install mode via `wp-inst --github <repo> --tag <tag>` for selecting a specific release, mutually exclusive with `--latest`.
- GitHub latest-release installs now resolve platform-matching assets directly from repository releases and prefer raw single-binary artifacts when available.
- Release workflow now builds and publishes `wp-inst` binary artifacts for tagged releases, including dry-run coverage, without wrapping single-file binaries in `tar.gz`.

### Fixed
- Artifact downloads now preserve raw response bytes instead of allowing HTTP auto-decompression to corrupt `.tar.gz` release assets during self-update.
- Added a regression test covering mislabelled gzip-encoded artifact responses.
- Self-update now accepts both `.tar.gz` archives and single raw binary artifacts for single-binary install targets.
- Artifact downloads now show visible progress in both interactive terminals and non-TTY log output, and the `curl` fallback no longer hides its progress stream.
- Installed binaries that do not implement `--version` now pass post-install health checks via fallback probes such as `-V`, `version`, and `--help`.
- GitHub release installs now label output with the target binary name, and report `Channel` as `main` for `--latest` or the selected tag for `--tag`.
