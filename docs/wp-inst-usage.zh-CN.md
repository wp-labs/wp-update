# `wp-inst` 使用指南

`wp-inst` 是一个用于安装和检查 wp-labs 发布制品的引导安装器。

当前 CLI 收敛为两类动作：

- `wp-inst` 或 `wp-inst install`：安装，`wp-inst` 无子命令时默认等价于 `install`
- `wp-inst check`：只检查，不安装

制品类型有两类：

- 二进制制品：默认模式，也可以显式写 `--bin`
- Skill 制品：显式写 `--skill`

## 快速开始

安装 GitHub 上的最新二进制 release：

```bash
wp-inst --github wp-labs/wpl-check
```

安装指定 tag：

```bash
wp-inst --github wp-labs/wpl-check --tag v0.1.8
```

从远程 manifest 源安装：

```bash
wp-inst --source https://example.com/updates
```

从本地 manifest 根目录安装：

```bash
wp-inst --source ./updates --channel beta
```

安装 skill：

```bash
wp-inst --skill --github wp-labs/wp-skills --path skills/warpparse-log-engineering
```

## 命令模型

默认安装：

```bash
wp-inst ...
```

显式安装：

```bash
wp-inst install ...
```

只检查：

```bash
wp-inst check ...
```

兼容别名：

- `wp-inst update` 作为 `install` 的兼容别名保留

## 公开参数

主用法只保留这几个公开参数：

- `--github <repo>`：GitHub 仓库，支持 `https://github.com/<owner>/<repo>` 或 `<owner>/<repo>`
- `--source <url-or-dir>`：Manifest 来源；`https://...` 视为远程 base URL，其他值视为本地目录
- `--tag <tag>`：安装或检查指定 GitHub tag；不传时默认 latest
- `--channel <stable|beta|alpha>`：Manifest channel，默认 `stable`
- `--dir <path>`：二进制安装目录
- `--yes`：跳过确认
- `--json`：输出 JSON
- `--bin`：显式选择二进制模式；默认就是该模式
- `--skill`：选择 skill 模式
- `--path <repo-subdir>`：skill 在仓库归档中的相对路径

约束：

- `--bin` 和 `--skill` 互斥
- 不写时默认 `--bin`
- `--github` 和 `--source` 互斥
- skill 模式要求 `--github` 和 `--path`
- 二进制模式使用 `--github` 或 `--source` 二选一

## 二进制模式

### 从 GitHub 安装

安装 latest release：

```bash
wp-inst --github wp-labs/wpl-check
```

安装指定 tag：

```bash
wp-inst --github wp-labs/wpl-check --tag v0.1.8
```

安装到指定目录：

```bash
wp-inst --github wp-labs/wpl-check --dir ~/bin
```

规则：

- 不传 `--tag` 时默认 latest
- GitHub 模式不使用 `--channel`
- 安装目标二进制名默认取仓库名

### 从 Manifest 安装

远程来源：

```bash
wp-inst --source https://example.com/updates
wp-inst --source https://example.com/updates --channel alpha
```

本地来源：

```bash
wp-inst --source ./updates
wp-inst --source ./updates --channel beta
```

Manifest 解析路径：

```text
{source}/{channel}/manifest.json
```

如果未传 `--source`，当前实现仍支持通过环境变量回退：

- `WP_INSTALLER_DEFAULT_BASE_URL`
- `WP_INSTALLER_DEFAULT_ROOT`

### 只检查

检查 GitHub 二进制：

```bash
wp-inst check --github wp-labs/wpl-check
wp-inst check --github wp-labs/wpl-check --tag v0.1.8
```

检查 Manifest：

```bash
wp-inst check --source https://example.com/updates
wp-inst check --source ./updates --channel beta
```

JSON 输出：

```bash
wp-inst check --github wp-labs/wpl-check --json
```

## Skill 模式

skill 模式用于从 GitHub release 归档中提取仓库子目录，并安装到默认的 skills 目录。

安装 skill：

```bash
wp-inst --skill --github wp-labs/wp-skills --path skills/warpparse-log-engineering
wp-inst --skill --github wp-labs/wp-skills --path skills/warpparse-log-engineering --tag v0.1.2
```

检查 skill：

```bash
wp-inst check --skill --github wp-labs/wp-skills --path skills/warpparse-log-engineering
```

规则：

- `--skill` 必须和 `--github` 一起使用
- `--skill` 必须和 `--path` 一起使用
- `--path` 必须是相对路径
- `--path` 不能包含 `..`
- 不传 `--tag` 时默认 latest release
- `--yes` 仅适用于二进制安装，skill 模式下不支持

安装目标目录：

- 若 `~/.codex/skills` 存在，会安装到该目录
- 若 `~/.claude/skills` 存在，也会安装到该目录
- 若两者都不存在，默认创建并安装到 `~/.claude/skills`

可以用环境变量 `WP_SKILLS_PLATFORM` 调整目标：

- `codex`
- `claude-code`
- `auto`

skill release 要求：

- `wp-inst` 会读取 GitHub Releases API
- release 中必须存在名为 `{repo-name}-{tag}.tar.gz` 的归档 asset
- 解压后必须能找到 `--path` 指向的子目录

## 安装行为

二进制模式下：

- 支持 `.tar.gz` / `.tgz` 与裸二进制单文件
- 安装前校验 SHA-256
- 安装前在目标目录创建 `.warp_parse-update/backups/<uuid>/` 备份
- 安装后执行健康检查
- 健康检查失败会回滚

skill 模式下：

- 下载 release 归档并解压
- 将 `--path` 对应的目录复制到目标 skills 目录
- 若目标位置已有同名 skill，则先整体替换

## 输出

文本输出用于人读，`--json` 用于脚本。

二进制 `check --json` 会输出：

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

二进制安装成功时，文本输出类似：

```text
wpl-check install
  Channel  : main
  Current  : 0.0.0
  Latest   : v0.1.8
  Install  : /Users/you/bin
  Artifact : https://github.com/wp-labs/wpl-check/releases/download/v0.1.8/wpl-check-v0.1.8-aarch64-apple-darwin
  Status   : installed (backup: /Users/you/bin/.warp_parse-update/backups/...)
```

skill 安装成功时，文本输出类似：

```text
Installed: warpparse-log-engineering
Source   : https://github.com/wp-labs/wp-skills
Path     : skills/warpparse-log-engineering
Platform : claude-code
Location : /Users/you/.claude/skills/warpparse-log-engineering
```

## 常见失败原因

- `--github` 与 `--source` 同时出现
- 二进制模式下未提供 `--github` 或 `--source`
- skill 模式下缺少 `--github` 或 `--path`
- skill release 缺少 `{repo-name}-{tag}.tar.gz`
- 当前平台缺少对应二进制 asset
- GitHub asset 缺少 digest 元数据
- artifact URL 与版本号不一致
- SHA-256 校验失败
- 安装目录不存在或不是目录
