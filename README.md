# wp-update

Shared update crates for wp-labs binaries.

## Crates

- `wp-self-update`
- `wp-inst`

## Docs

- `docs/orion-accessor-evaluation.md`

## Quick Start

Install the latest GitHub release for a single-binary tool directly:

```bash
wp-inst --github https://github.com/wp-labs/wpl-check
```

Install a specific GitHub release tag directly:

```bash
wp-inst --github https://github.com/wp-labs/wpl-check --tag v0.1.7
```

Install a skill into Codex or Claude skills directories:

```bash
wp-inst --skill --github wp-labs/wp-skills --tag v0.1.2 --path skills/warpparse-log-engineering
wp-inst --skill --github wp-labs/wp-skills --path skills/wpl-rule-check
```

Check a release without installing:

```bash
wp-inst check --github wp-labs/wpl-check
wp-inst check --skill --github wp-labs/wp-skills --path skills/wpl-rule-check
```
