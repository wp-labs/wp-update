# `wp-inst` Usage

`wp-inst` is a bootstrap installer for wp-labs release artifacts.

The CLI has two actions:

- `wp-inst` or `wp-inst install`: install artifacts; no subcommand defaults to `install`
- `wp-inst check`: inspect an artifact without installing it

The CLI has two artifact modes:

- binary artifacts: default mode; you can also spell it as `--bin`
- skill artifacts: explicit `--skill`

## Quick Start

Install the latest GitHub binary release:

```bash
wp-inst --github wp-labs/wpl-check
```

Install a specific tag:

```bash
wp-inst --github wp-labs/wpl-check --tag v0.1.8
```

Install from a remote manifest source:

```bash
wp-inst --source https://example.com/updates
```

Install a skill:

```bash
wp-inst --skill --github wp-labs/wp-skills --path skills/warpparse-log-engineering
```

## Public Flags

The primary interface is intentionally small:

- `--github <repo>`: GitHub repository URL or `<owner>/<repo>`
- `--source <url-or-dir>`: manifest source; `https://...` is treated as a remote base URL, anything else as a local root directory
- `--tag <tag>`: select a specific GitHub tag; if omitted, `wp-inst` resolves the latest release
- `--channel <stable|beta|alpha>`: manifest channel, default `stable`
- `--dir <path>`: binary install directory
- `--yes`: skip confirmation
- `--json`: machine-readable output
- `--bin`: explicit binary mode
- `--skill`: skill mode
- `--path <repo-subdir>`: skill path inside the repository archive

Rules:

- `--bin` and `--skill` are mutually exclusive
- binary mode is the default
- `--github` and `--source` are mutually exclusive
- skill mode requires `--github` and `--path`
- binary mode uses `--github` or `--source`

## Binary Mode

Install from GitHub:

```bash
wp-inst --github wp-labs/wpl-check
wp-inst --github wp-labs/wpl-check --tag v0.1.8
wp-inst --github wp-labs/wpl-check --dir ~/bin
```

Install from manifests:

```bash
wp-inst --source https://example.com/updates
wp-inst --source ./updates --channel beta
```

Check without installing:

```bash
wp-inst check --github wp-labs/wpl-check
wp-inst check --source ./updates --channel alpha
wp-inst check --github wp-labs/wpl-check --json
```

Manifest paths resolve as:

```text
{source}/{channel}/manifest.json
```

If `--source` is omitted, the current implementation still supports environment fallback:

- `WP_INSTALLER_DEFAULT_BASE_URL`
- `WP_INSTALLER_DEFAULT_ROOT`

## Skill Mode

Skill mode installs a repository subdirectory from a GitHub release archive into the default skills directories.

Install:

```bash
wp-inst --skill --github wp-labs/wp-skills --path skills/warpparse-log-engineering
wp-inst --skill --github wp-labs/wp-skills --path skills/warpparse-log-engineering --tag v0.1.2
```

Check:

```bash
wp-inst check --skill --github wp-labs/wp-skills --path skills/warpparse-log-engineering
```

Rules:

- `--skill` requires `--github`
- `--skill` requires `--path`
- `--path` must be relative
- `--path` must not contain `..`
- if `--tag` is omitted, `wp-inst` resolves the latest release
- `--yes` is only for binary installs and is rejected in skill mode

Default targets:

- install into `~/.codex/skills` if it exists
- also install into `~/.claude/skills` if it exists
- if neither exists, create and use `~/.claude/skills`

`WP_SKILLS_PLATFORM` can be used to constrain targets:

- `codex`
- `claude-code`
- `auto`

Skill release requirements:

- `wp-inst` reads the GitHub Releases API
- the release must contain an asset named `{repo-name}-{tag}.tar.gz`
- the extracted archive must contain the requested `--path`

## Behavior

Binary mode:

- supports `.tar.gz` / `.tgz` archives and raw single-file binaries
- verifies SHA-256 before install
- creates backups under `.warp_parse-update/backups/<uuid>/`
- runs a post-install health check
- rolls back on health-check failure

Skill mode:

- downloads the release archive and extracts it
- copies the requested `--path` subtree into the target skills directory
- replaces an existing skill directory atomically at the destination level

## Output

Text output is for humans. `--json` is intended for scripts.

Binary `check --json` emits:

- `product`
- `channel`
- `branch`
- `source`
- `manifest_format`
- `current_version`
- `latest_version`
- `update_available`
- `platform_key`
- `artifact`
- `sha256`

Binary install output looks like:

```text
wpl-check install
  Channel  : main
  Current  : 0.0.0
  Latest   : v0.1.8
  Install  : /Users/you/bin
  Artifact : https://github.com/wp-labs/wpl-check/releases/download/v0.1.8/wpl-check-v0.1.8-aarch64-apple-darwin
  Status   : installed (backup: /Users/you/bin/.warp_parse-update/backups/...)
```

Skill install output looks like:

```text
Installed: warpparse-log-engineering
Source   : https://github.com/wp-labs/wp-skills
Path     : skills/warpparse-log-engineering
Platform : claude-code
Location : /Users/you/.claude/skills/warpparse-log-engineering
```
