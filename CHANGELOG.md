# Changelog

## [Unreleased]

## [0.2.0] - 2026-04-29

### Added
- Added structured installer error semantics tests covering stable identities, self-update wrap boundaries, and preserved source frames.
- Added machine-consumable CLI error JSON output for `wp-inst --json` failure paths via `orion-error` CLI projections.

### Changed
- Replaced `wp-self-update`'s dependency on `wp-error` with a dedicated `UpdateReason` / `UpdateError` model built on `orion-error 0.7`.
- Replaced `wp-inst` internal `Box<dyn std::error::Error>` flows with a dedicated `InstallerReason` / `InstallerError` model built on `orion-error 0.7`.
- Wrapped `wp-self-update` failures at the installer boundary so `wp-inst` reports installer-level semantics while preserving lower-layer structured sources.

## [0.1.9] - 2026-03-25

### Added
- Added `wp-inst check --skill --github <repo> --path <repo-subdir>` to validate skill release archives and requested skill paths before installation.
- Added dedicated JSON/text reports for skill checks and installs.

### Changed
- Simplified `wp-inst` CLI around `install` and `check`, with bare `wp-inst` defaulting to `install` and `update` retained as an alias of `install`.
- Simplified top-level flags to prefer `--github`, `--source`, `--tag`, `--dir`, `--bin`, and `--skill`, while keeping legacy manifest flags hidden for compatibility.
- Unified artifact selection so binary mode defaults to `--bin` and GitHub installs default to the latest release when `--tag` is omitted.

### Fixed
- Skill installs now stage into a temporary directory before swapping into place, avoiding destructive replacement of an existing skill on partial failure.
- Skill checks now reuse a single resolved release/archive instead of reloading GitHub release metadata twice.
- Skill mode now rejects binary-only flags such as `--yes`, `--dir`, and explicit `--channel`, and binary mode rejects stray `--path` values unless `--skill` is selected.
- Skill activation errors now report backup-restore failures explicitly when rollback cannot complete.

## [0.1.8] - 2026-03-24

### Added
- Added direct skill mode via `wp-inst --github <repo> --latest|--tag <tag> --path <repo-subdir> --skill`, installing the selected skill from a release archive into default skills directories.
- Skill installs follow the `wp-skills` shell workflow and honor `WP_SKILLS_PLATFORM` for default target selection.

### Changed
- Direct GitHub binary installation now defaults to the latest release when `--tag` and `--latest` are both omitted.
- Switched skill installation from GitHub tree snapshots to versioned GitHub release archives selected by `--latest` or `--tag`.
- Skill installation now defaults to the latest release when `--tag` and `--latest` are both omitted.
- Refactored `wp-inst` source layout to split CLI parsing, source resolution, reporting, and skill installation responsibilities into smaller modules.
- Reduced installer-local archive handling by reusing `wp-self-update` release metadata loading and archive download/extract helpers.

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
