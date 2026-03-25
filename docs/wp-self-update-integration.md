# `wp-self-update` Integration

This document is for maintainers who want to add self-update support to a Rust binary using `wp-self-update`.

## What The Crate Provides

Public entry points:

- `wp_self_update::check`
- `wp_self_update::update`

Core request and response types:

- `CheckRequest`
- `UpdateRequest`
- `CheckReport`
- `UpdateReport`
- `SourceConfig`
- `SourceKind`
- `UpdateChannel`
- `UpdateTarget`
- `UpdateProduct`
- `GithubRepo`

## Add The Dependency

```toml
[dependencies]
tokio = { version = "1.48", features = ["macros", "rt-multi-thread"] }
wp-self-update = "0.1.7"
```

## Choose A Source Strategy

`wp-self-update` supports two release source families.

### 1. Manifest Source

Use this when you control a channel-based manifest tree such as:

```text
updates/
  stable/manifest.json
  beta/manifest.json
  alpha/manifest.json
```

Rust setup:

```rust
use wp_self_update::{SourceConfig, SourceKind, UpdateChannel};

let source = SourceConfig {
    channel: UpdateChannel::Stable,
    kind: SourceKind::Manifest {
        updates_base_url: "https://example.com/updates".to_string(),
        updates_root: None,
    },
};
```

For local testing:

```rust
let source = SourceConfig {
    channel: UpdateChannel::Stable,
    kind: SourceKind::Manifest {
        updates_base_url: String::new(),
        updates_root: Some(std::path::PathBuf::from("./updates")),
    },
};
```

### 2. GitHub Release Source

Use this when your binary is published on GitHub Releases.

Latest release:

```rust
use wp_self_update::{GithubRepo, SourceConfig, SourceKind, UpdateChannel};

let repo = GithubRepo::parse("https://github.com/wp-labs/wpl-check")?;
let source = SourceConfig {
    channel: UpdateChannel::Stable,
    kind: SourceKind::GithubLatest { repo },
};
```

Specific tag:

```rust
let repo = GithubRepo::parse("wp-labs/wpl-check")?;
let source = SourceConfig {
    channel: UpdateChannel::Stable,
    kind: SourceKind::GithubTag {
        repo,
        tag: "v0.1.7".to_string(),
    },
};
```

Notes:

- GitHub mode ignores semantic channels like `stable` or `alpha`
- The crate reports `main` for latest-release mode and the exact tag for tag mode
- GitHub assets must include digest metadata from the GitHub Releases API

## Minimal Check Flow

Use `check` when you want to show update status without changing anything.

```rust
use wp_self_update::{check, CheckRequest};

let report = check(CheckRequest {
    product: "wpl-check".to_string(),
    source,
    current_version: env!("CARGO_PKG_VERSION").to_string(),
    branch: "main".to_string(),
})
.await?;

println!("latest = {}", report.latest_version);
println!("artifact = {}", report.artifact);
println!("update_available = {}", report.update_available);
```

## Minimal Update Flow

Use `update` when you want the crate to download, validate, unpack if needed, install, and health-check the binary.

```rust
use wp_self_update::{update, UpdateRequest, UpdateTarget};

let report = update(UpdateRequest {
    product: "wpl-check".to_string(),
    target: UpdateTarget::Bins(vec!["wpl-check".to_string()]),
    source,
    current_version: env!("CARGO_PKG_VERSION").to_string(),
    install_dir: Some(std::path::PathBuf::from("/Users/you/bin")),
    yes: false,
    dry_run: false,
    force: false,
})
.await?;

println!("status = {}", report.status);
```

## Pick The Right `UpdateTarget`

`UpdateTarget` controls what binaries the installer expects to install.

- `UpdateTarget::Bins(vec![...])`: best for custom binaries or GitHub single-binary releases
- `UpdateTarget::Product(UpdateProduct::...)`: best for built-in wp-labs multi-binary products
- `UpdateTarget::Auto`: discover binaries from an extracted archive, or use the current executable name for raw single-binary updates

For a single binary, prefer:

```rust
UpdateTarget::Bins(vec!["my-binary".to_string()])
```

## Artifact Formats

The installer accepts two artifact formats:

- `.tar.gz`
- raw single binary

Behavior:

- `.tar.gz` is extracted before installation
- raw binaries require exactly one target binary

If you publish a raw GitHub asset, keep the target binary mapping unambiguous.

## Manifest Requirements

Manifest JSON must be shaped like:

```json
{
  "version": "v0.1.7",
  "channel": "stable",
  "assets": {
    "aarch64-apple-darwin": {
      "url": "https://example.com/releases/download/v0.1.7/my-binary-v0.1.7-aarch64-apple-darwin",
      "sha256": "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
    }
  }
}
```

Important rules:

- `channel` must match the requested `UpdateChannel`
- `version` must be valid semver with optional `v` prefix
- artifact URL must contain the version string
- `sha256` must be 64 lowercase or uppercase hex characters
- the current platform target must exist in `assets`

## GitHub Release Requirements

If you use GitHub mode, make sure:

- the release contains an asset for the current target triple
- the asset name contains the target triple
- the release API exposes a `sha256:...` digest for that asset

Selection preference for matching assets:

1. raw binary whose name ends with the target triple
2. other raw binary containing the target triple
3. `.tar.gz` or `.tgz` asset containing the target triple

## Installation Semantics

`update()` performs these steps:

1. resolve the release metadata
2. compare `current_version` with the release version
3. validate the download URL and SHA-256
4. download the artifact with progress reporting
5. extract if needed
6. install binaries into the target dir with backup
7. run a health check
8. roll back on failed health check

## Health Check Behavior

After install, the crate probes installed binaries using:

- `--version`
- `-V`
- `version`
- `--help`
- `help`

This matters for tools that do not implement `--version`.

## Operational Constraints

- `install_dir` must exist
- package-managed locations may be rejected unless `force = true`
- network downloads are HTTPS-only except loopback HTTP for local testing
- backups are stored under `.warp_parse-update/backups/`

## Recommended Integration Pattern

For most single-binary tools:

```rust
use wp_self_update::{
    update, GithubRepo, SourceConfig, SourceKind, UpdateChannel, UpdateRequest, UpdateTarget,
};

let repo = GithubRepo::parse("my-org/my-binary")?;
let report = update(UpdateRequest {
    product: "my-binary".to_string(),
    target: UpdateTarget::Bins(vec!["my-binary".to_string()]),
    source: SourceConfig {
        channel: UpdateChannel::Stable,
        kind: SourceKind::GithubLatest { repo },
    },
    current_version: env!("CARGO_PKG_VERSION").to_string(),
    install_dir: Some(std::path::PathBuf::from("/Users/you/bin")),
    yes: true,
    dry_run: false,
    force: false,
})
.await?;
```

That is the simplest path if your release pipeline already publishes one binary per target on GitHub Releases.
