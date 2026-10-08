# `wp-self-update` Platform Target Resolution: Linux One-to-Many (musl / glibc)

- Status: §3 (client candidates) and §4 (server-side aliases) implemented; release/rollout (§5) pending
- Last updated: 2026-10-08
- Related: `galaxio-labs/galaxy-ops` release matrix convergence (drop gnu, add aarch64-musl)
- Scope: `crates/wp-self-update/`, `galaxy-ops/.github/workflows/release.yml`

## 1. Background

WarpInsightCenter hosts each repo's release artifacts under a **fixed platform matrix** for gateway upgrade/pull, and requires a stable, complete set. The agreed three artifacts are:

| Platform | target triple | Expected asset name |
| --- | --- | --- |
| Linux x86_64 · musl (static) | `x86_64-unknown-linux-musl` | `<repo>-<version>-x86_64-unknown-linux-musl.tar.gz` |
| Linux ARM64 · musl (static) | `aarch64-unknown-linux-musl` | `<repo>-<version>-aarch64-unknown-linux-musl.tar.gz` |
| macOS · ARM | `aarch64-apple-darwin` | `<repo>-<version>-aarch64-apple-darwin.tar.gz` |

`galaxy-ops`' release workflow has been updated accordingly: **`x86_64-unknown-linux-gnu` removed, `aarch64-unknown-linux-musl` added**, all Linux artifacts musl-static (see `.github/workflows/release.yml`). No glibc artifact is published.

Side effect: **the self-update path breaks**, because the client still requests a `-gnu` key.

## 2. Problem and evidence

Self-update (`gops self` / `gx self` / `inst-x.sh`) resolves artifacts through the v2 manifest in `galaxio-labs/get`, which is **keyed by target triple**. Resolution looks up exactly **one** key:

- `crates/wp-self-update/src/platform.rs:3-12` — detection returns a **single** triple; Linux maps to `-gnu`:
  ```rust
  ("linux", "x86_64")  => "x86_64-unknown-linux-gnu",
  ("linux", "aarch64") => "aarch64-unknown-linux-gnu",
  ("macos", "aarch64") => "aarch64-apple-darwin",
  ```
- `crates/wp-self-update/src/manifest.rs:49-59` — `manifest.assets.get(target)`, **hard error when absent** (no fallback):
  ```
  manifest missing asset for target 'x86_64-unknown-linux-gnu'
    (available: aarch64-apple-darwin, x86_64-unknown-linux-musl, aarch64-unknown-linux-musl)
  ```
- `crates/wp-self-update/src/fetch.rs:169-183` — GitHub source (`--github` / `inst-x.sh`) is single-key too (`select_github_release_asset`, `:219-243`).

`gops` uses the manifest source: `galaxy-ops/src/self_update/service.rs:11-12,315-321`. `gx` shares the same crate.

Once the manifest drops gnu:

| Platform | Requested key | In new manifest? | Result |
| --- | --- | --- | --- |
| Linux x86_64 | `x86_64-unknown-linux-gnu` | No | **broken** (worked via gnu before) |
| Linux aarch64 | `aarch64-unknown-linux-gnu` | No | already broken (never published), still broken |
| macOS arm64 | `aarch64-apple-darwin` | Yes | fine |

> Note: Linux self-update currently pulls the **gnu** artifact; the musl asset has never been selected on the self-update path. So the "musl for portability" rationale is not realized end-to-end until the client is fixed.

## 3. Design: client-side one-to-many (ordered candidates)

Change platform detection from a **single value** to an **ordered candidate list**, and resolve by taking **the first hit**. `ResolvedRelease.target` stays a single value (the matched triple); downstream (`CheckReport.platform_key`, logs, `install_bins`) is unchanged.

### 3.1 Platform detection

```rust
// platform.rs
pub(crate) fn detect_target_candidates() -> UpdateResult<&'static [&'static str]> {
    Ok(match (std::env::consts::OS, std::env::consts::ARCH) {
        ("linux", "x86_64")  => &["x86_64-unknown-linux-musl", "x86_64-unknown-linux-gnu"],
        ("linux", "aarch64") => &["aarch64-unknown-linux-musl", "aarch64-unknown-linux-gnu"],
        ("macos", "aarch64") => &["aarch64-apple-darwin"],
        (os, arch) => return Err(invalid_request(format!("unsupported platform: {os}-{arch}"))),
    })
}
```

### 3.2 Manifest resolution

```rust
// manifest.rs
let candidates = detect_target_candidates()?;
let (target, asset) = candidates
    .iter()
    .find_map(|t| manifest.assets.get(*t).map(|a| (*t, a)))
    .ok_or_else(|| {
        // error lists both the candidates and the keys actually present
    })?;
```

### 3.3 GitHub source resolution

`select_github_release_asset` takes the candidate list; the caller iterates in order and takes the first `Some`. Its internal shape preference (raw > archive) is unchanged.

### 3.4 Ordering is policy

- **musl first**: a musl-static binary runs on both glibc and musl hosts; gnu only on glibc. musl-first is strictly better and matches the matrix convergence goal.
- **Cost**: for products publishing both (e.g. gops today), Linux clients silently switch from pulling gnu to pulling musl. That is the desired direction, but it is a behavior change and should be called out in release notes.
- gnu-first is not recommended (breaks Alpine hosts).

### 3.5 Compatibility

- Existing tests stay green more easily: e.g. `manifest.rs::parse_v2_release_ok` uses gnu keys with no musl — with candidates `[musl, gnu]` it falls through to the gnu hit. A hard switch to musl-only would instead break it.
- Substring matching cannot cross-match: `x86_64-unknown-linux-gnu` is not a substring of the musl name, or vice versa.
- The function is `pub(crate)` with only two call sites — no external fallout from renaming/resigning.

## 4. Server-side safety net: transitional manifest aliases

Client-side one-to-many **only helps future binaries**. Already-installed binaries (and `inst-x.sh`) still ask for the gnu key. So a **server-side** fallback is required: when generating the manifest, add aliases for the glibc-era keys that **point at the musl asset**.

Append after "Build assets map" in `galaxy-ops/.github/workflows/release.yml`:

```jq
| from_entries
| . + (
    (if .["x86_64-unknown-linux-musl"] != null
     then { "x86_64-unknown-linux-gnu": .["x86_64-unknown-linux-musl"] }
     else {} end)
    + (if .["aarch64-unknown-linux-musl"] != null
       then { "aarch64-unknown-linux-gnu": .["aarch64-unknown-linux-musl"] }
       else {} end)
  )
```

Notes:

- **Published artifacts remain the three musl/darwin ones, with no gnu** (satisfies the center); the alias exists only in the manifest key space.
- Both old and new clients resolve: new clients use the musl key, old clients use the gnu alias, pointing at the same musl asset.
- Aliases are a **transition**, not the end state.

## 5. Rollout ordering

`gops self` uses the logic embedded in the **already-installed binary**; the one-to-many fix only takes effect after users upgrade to a build containing it. Ordering is therefore the safety boundary:

- **Option A (clean, slow)**: release musl-aware `wp-self-update` → products bump the dependency and release → once the fleet is upgraded, drop aliases / remove gnu entirely.
- **Option B (fast, safe)**: ship the musl-only matrix together with §4 aliases immediately; old and new clients both keep working; drop aliases later.
- **Recommended**: **A + B combined**. B guarantees zero-downtime publishing; A guarantees final convergence to true musl-only (by then aliases have no consumers and can be deleted).

**Alias removal condition**: after one release cycle, confirm (or measure) that no client requests the gnu key.

**Ordering trap (avoid)**: shipping a gnu-less matrix with neither a client fix nor aliases — existing Linux users can neither self-update nor install via `inst-x.sh`, and must fetch the musl tarball manually.

## 6. Impact list

| File | Location | Change |
| --- | --- | --- |
| `crates/wp-self-update/src/platform.rs` | `:3-12` | single value → candidate list |
| `crates/wp-self-update/src/manifest.rs` | `:49-59` + tests | iterate candidates, take first hit; error lists candidates |
| `crates/wp-self-update/src/fetch.rs` | `:169-183, 219-243` + tests | `select_github_release_asset` accepts candidates |
| `crates/wp-self-update/Cargo.toml` | version | bump and publish |
| `galaxy-ops/Cargo.toml` / `Cargo.lock` | `wp-self-update` dep | bump to the new version |
| `galaxy-ops/.github/workflows/release.yml` | manifest job | add transitional alias keys |
| `crates/wp-installer` | shares `platform.rs` | benefits automatically, no separate change |

**Not affected** (verified):

- The packaging change `artifacts/gops` → `<repo>-<version>-<triple>/gops` does not affect installs: `install.rs:463-494 find_extracted_bins` (`Bins` target) and `:496-543 discover_extracted_bins` (`Auto` target) both walk the extracted tree **recursively**; the `artifacts` component is only a naming preference, not a hard requirement.
- `ResolvedRelease.target`, `CheckReport.platform_key`, `install_bins`, health check/rollback: all remain single-value semantics.

## 7. Verification

Unit tests:

- Candidate hit: manifest with musl only → hits musl; gnu only → falls back to gnu; both → hits musl.
- All absent → error message includes candidates and available keys.
- GitHub source: assets matched in candidate order, with the raw > archive preference intact.
- Existing `parse_v2_release_*` / `select_*` tests stay green.

End-to-end (post-release):

- Manifest carries the three musl/darwin keys; during transition, also the two gnu aliases.
- `file <bin>` → `statically linked`; `ldd <bin>` → `not a dynamic executable`.
- `<bin> --version` runs on both Alpine and Ubuntu.
- `gops self check/update` works on Linux x86_64 and macOS arm64; older `gops` still upgrades via aliases.

## 8. Acceptance

- `gops self update` and `inst-x.sh gops` work again on Linux (new clients use musl).
- During the transition, un-upgraded old clients are uninterrupted via aliases.
- The published matrix stays at three artifacts with no gnu, matching the center's platform set.
- After alias removal, behavior is unchanged (no consumers).

## 9. Open decisions

1. **Candidate order**: confirm musl-first (recommended).
2. **Alias lifetime**: decide removal by release cycle vs. client telemetry.
3. **Scope**: whether `gx` (galaxy-flow) also converges its matrix / bumps the `wp-self-update` dependency.
4. **Naming**: rename `detect_target_triple_v2` to `detect_target_candidates`.
