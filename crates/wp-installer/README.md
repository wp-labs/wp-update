# wp-inst

Bootstrap installer CLI for wp-labs binary releases, built on top of `wp-self-update`.

Examples:

```bash
wp-inst check --base-url https://raw.githubusercontent.com/wp-labs/warp-parse/refs/heads/main/updates
wp-inst --github https://github.com/wp-labs/wpl-check
wp-inst --github https://github.com/wp-labs/wpl-check --tag v0.1.7
wp-inst --github wp-labs/wp-skills --tag v0.1.2 --path skills/warpparse-log-engineering --skill
wp-inst --github wp-labs/wp-skills --path skills/wpl-rule-check --skill
```
