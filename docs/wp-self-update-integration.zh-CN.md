# `wp-self-update` 接入文档

这份文档面向需要在 Rust 二进制程序中接入 `wp-self-update` 的维护者。

## 这个 crate 提供什么

主要公开入口：

- `wp_self_update::check`
- `wp_self_update::update`

主要请求和返回类型：

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

## 添加依赖

```toml
[dependencies]
tokio = { version = "1.48", features = ["macros", "rt-multi-thread"] }
wp-self-update = "0.1.7"
```

## 先确定发布来源

`wp-self-update` 支持两类发布来源。

### 1. Manifest 来源

适用于你自己维护 channel 目录树的场景，例如：

```text
updates/
  stable/manifest.json
  beta/manifest.json
  alpha/manifest.json
```

Rust 配置示例：

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

本地测试示例：

```rust
let source = SourceConfig {
    channel: UpdateChannel::Stable,
    kind: SourceKind::Manifest {
        updates_base_url: String::new(),
        updates_root: Some(std::path::PathBuf::from("./updates")),
    },
};
```

### 2. GitHub Release 来源

适用于你的程序通过 GitHub Releases 发布的场景。

使用最新版本：

```rust
use wp_self_update::{GithubRepo, SourceConfig, SourceKind, UpdateChannel};

let repo = GithubRepo::parse("https://github.com/wp-labs/wpl-check")?;
let source = SourceConfig {
    channel: UpdateChannel::Stable,
    kind: SourceKind::GithubLatest { repo },
};
```

使用指定 tag：

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

说明：

- GitHub 模式不使用 `stable`、`alpha` 这种语义 channel
- crate 在 latest 模式下会报告 `main`，在 tag 模式下会报告具体 tag
- GitHub 资产必须在 Releases API 中带有 digest 元数据

## 最小化检查流程

如果你只想判断是否有更新，而不做安装，可以调用 `check`。

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

## 最小化安装流程

如果你希望 crate 负责下载、校验、解包、安装和健康检查，可以调用 `update`。

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

## 如何选择 `UpdateTarget`

`UpdateTarget` 用来告诉安装器“应该安装哪些二进制文件”。

- `UpdateTarget::Bins(vec![...])`：适合自定义程序，尤其是 GitHub 单文件二进制发布
- `UpdateTarget::Product(UpdateProduct::...)`：适合内置 wp-labs 多二进制产品
- `UpdateTarget::Auto`：从解包后的目录自动发现；如果是裸二进制，则回退为当前可执行文件名

对于单二进制程序，优先建议：

```rust
UpdateTarget::Bins(vec!["my-binary".to_string()])
```

## 支持的制品格式

安装器接受两种制品格式：

- `.tar.gz`
- 裸二进制单文件

行为说明：

- `.tar.gz` 会先解包再安装
- 裸二进制制品要求目标二进制必须唯一

如果你发布的是裸 GitHub 资产，目标二进制映射必须明确。

## Manifest 要求

Manifest JSON 结构要求如下：

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

关键规则：

- `channel` 必须和请求里的 `UpdateChannel` 一致
- `version` 必须是合法 semver，可以带 `v` 前缀
- artifact URL 必须包含版本字符串
- `sha256` 必须是 64 位十六进制
- `assets` 中必须有当前平台对应的 target

## GitHub Release 要求

如果使用 GitHub 模式，需要满足：

- release 中包含当前 target triple 对应的资产
- asset 名称中包含 target triple
- Releases API 为该 asset 提供 `sha256:...` digest

匹配优先级：

1. 名称以 target triple 结尾的裸二进制
2. 其他包含 target triple 的裸二进制
3. 包含 target triple 的 `.tar.gz` 或 `.tgz`

## 安装语义

`update()` 执行步骤如下：

1. 解析发布元数据
2. 比较 `current_version` 和目标版本
3. 校验下载 URL 与 SHA-256
4. 下载制品并输出进度
5. 如有需要则解包
6. 安装到目标目录并保留备份
7. 执行健康检查
8. 健康检查失败时回滚

## 健康检查行为

安装完成后，crate 会按顺序尝试：

- `--version`
- `-V`
- `version`
- `--help`
- `help`

这对那些没有实现 `--version` 的工具很重要。

## 运行约束

- `install_dir` 必须已经存在
- 如果目录看起来像包管理安装目录，默认可能拒绝安装，除非 `force = true`
- 网络下载默认只接受 HTTPS；本地测试可以使用 loopback HTTP
- 备份目录位于 `.warp_parse-update/backups/`

## 推荐接入方式

对于大多数单二进制程序，推荐这样接入：

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

如果你的发布流水线已经在 GitHub Releases 上按平台发布单独的二进制文件，这通常是最简单的接入路径。
