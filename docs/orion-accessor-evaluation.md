# orion-accessor Evaluation

## Scope

This note evaluates whether `orion-accessor = "0.6.0"` should replace the current HTTP download implementation in `wp-self-update`.

The relevant current code paths are:

- `crates/wp-self-update/src/fetch.rs`
- `crates/wp-self-update/src/install.rs`

They currently cover two narrow cases:

- Download a manifest as text.
- Download an artifact as raw bytes.

## Current Requirements

The existing updater needs behavior that is very specific and easy to reason about:

- Manifest fetch must preserve HTTP status handling, especially `404`.
- Artifact fetch must return raw bytes and must not auto-decode `.tar.gz` payloads.
- Both paths need timeout and retry control.
- Artifact URLs are restricted by explicit host validation.
- The implementation should stay small and easy to test.

## What orion-accessor Provides

Based on the official repository and crate metadata, `orion-accessor` is a broader access layer focused on:

- Redirect and proxy rules
- HTTP / Git / local resource abstraction
- Downloading resources to local files
- Shared timeout / access-control configuration

Relevant upstream files reviewed:

- `README.md`
- `Cargo.toml`
- `src/types.rs`
- `src/addr/accessor/http.rs`
- `src/addr/accessor/client.rs`

## Fit Assessment

### Good Fit

`orion-accessor` is a reasonable candidate if `wp-update` needs:

- URL redirect rules
- proxy control
- mirror switching
- a shared resource abstraction across HTTP, Git, and local files

### Poor Fit

It is not a strong direct fit for the current updater download layer:

- Its primary download abstraction is `download_to_local`, not `fetch_text` or `fetch_bytes`.
- Its HTTP implementation still uses `reqwest` internally.
- The reviewed HTTP client builder does not show explicit disabling of auto gzip/brotli/deflate/zstd decoding.
- Replacing the current code would still require extra wrappers for manifest text loading and raw artifact bytes.

## Decision

Do not replace the current `reqwest`-based downloader with `orion-accessor` at this time.

The current implementation is narrower, easier to verify, and already covers the updater-specific edge cases we need.

## Download Error Fix

The previous updater failure:

`failed to read artifact response ...: error decoding response body`

was fixed in `crates/wp-self-update/src/install.rs` by:

- disabling automatic HTTP body decompression for artifact downloads
- increasing artifact request timeout to better tolerate large release assets
- keeping retry behavior intact

This preserves raw `.tar.gz` bytes for checksum verification and archive extraction.

## Recommended Future Use

If redirect or proxy policy becomes important, evaluate `orion-accessor` only for URL transformation / access-control concerns first.

Recommended migration order:

1. Keep current manifest and artifact download code.
2. Introduce a small URL rewrite abstraction if needed.
3. Integrate `orion-accessor` only for redirect / proxy handling.
4. Re-evaluate full downloader replacement only after proving it can preserve raw artifact byte behavior.
