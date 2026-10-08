# `wp-self-update` 平台目标解析：Linux 一对多（musl / glibc）

- 文档状态：§3（客户端候选）、§4（服务端过渡别名）已实现；发布/迁移（§5）待执行
- 最后更新：2026-10-08
- 关联：`galaxio-labs/galaxy-ops` 发布矩阵收敛（去 gnu、补 aarch64-musl）
- 涉及代码：`crates/wp-self-update/`、`galaxy-ops/.github/workflows/release.yml`

## 1. 背景

控制中心（WarpInsightCenter）以**固定平台矩阵**托管各仓库的 release 制品，用于网关升级/取件，要求「一次录入即齐备、命名稳定」。据此确定的三份制品为：

| 平台 | target triple | 期望资产名 |
| --- | --- | --- |
| Linux x86_64 · musl（静态） | `x86_64-unknown-linux-musl` | `<repo>-<version>-x86_64-unknown-linux-musl.tar.gz` |
| Linux ARM64 · musl（静态） | `aarch64-unknown-linux-musl` | `<repo>-<version>-aarch64-unknown-linux-musl.tar.gz` |
| macOS · ARM | `aarch64-apple-darwin` | `<repo>-<version>-aarch64-apple-darwin.tar.gz` |

`galaxy-ops` 的发布工作流已按此调整：**移除 `x86_64-unknown-linux-gnu`、新增 `aarch64-unknown-linux-musl`**，并统一用 musl 静态链接（见 `.github/workflows/release.yml`）。控制中心要求「不出现 glibc 制品」。

这带来一个副作用：**自升级链路会断**。原因是客户端请求的目标 key 仍是 `-gnu`。

## 2. 问题与证据

自升级（`gops self` / `gx self` / `inst-x.sh`）通过 `galaxio-labs/get` 的 v2 manifest 解析制品，manifest 是**按 target triple 作 key** 的映射。解析时只查**一个** key：

- `crates/wp-self-update/src/platform.rs:3-12` — 平台检测返回**单个** triple；Linux 映射到 `-gnu`：
  ```rust
  ("linux", "x86_64")  => "x86_64-unknown-linux-gnu",
  ("linux", "aarch64") => "aarch64-unknown-linux-gnu",
  ("macos", "aarch64") => "aarch64-apple-darwin",
  ```
- `crates/wp-self-update/src/manifest.rs:49-59` — `manifest.assets.get(target)`，**取不到即硬报错**（无回退）：
  ```
  manifest missing asset for target 'x86_64-unknown-linux-gnu'
    (available: aarch64-apple-darwin, x86_64-unknown-linux-musl, aarch64-unknown-linux-musl)
  ```
- `crates/wp-self-update/src/fetch.rs:169-183` — GitHub 来源（`--github`/`inst-x.sh`）也是单 key 匹配（`select_github_release_asset`，`:219-243`）。

`gops` 正是走 manifest 来源：`galaxy-ops/src/self_update/service.rs:11-12,315-321`（`SourceKind::Manifest` 指向 `get/updates/gops`）。`gx` 共用同一 crate。

一旦 manifest 去掉 gnu：

| 平台 | 客户端请求的 key | 新 manifest 内有？ | 结果 |
| --- | --- | --- | --- |
| Linux x86_64 | `x86_64-unknown-linux-gnu` | ❌ | **坏**（原先靠 gnu 可用） |
| Linux aarch64 | `aarch64-unknown-linux-gnu` | ❌ | 早已坏（从未发布），仍坏 |
| macOS arm64 | `aarch64-apple-darwin` | ✅ | 正常 |

> 补充事实：当前 Linux 自升级实际拉取的是 **gnu** 制品，musl 资产在自升级链路里从未被选中。「用 musl 换可移植性」在客户端修好之前并不成立。

## 3. 设计：客户端一对多（有序候选）

把「平台检测」从**单值**改为**有序候选列表**，解析时**按序取第一个命中**。`ResolvedRelease.target` 仍是单值（命中的那一个），下游（`CheckReport.platform_key`、日志、`install_bins`）不变。

### 3.1 平台检测

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

### 3.2 Manifest 解析

```rust
// manifest.rs
let candidates = detect_target_candidates()?;
let (target, asset) = candidates
    .iter()
    .find_map(|t| manifest.assets.get(*t).map(|a| (*t, a)))
    .ok_or_else(|| {
        // 报错里同时列出 candidates 与 manifest 实际拥有的 keys
    })?;
```

### 3.3 GitHub 来源解析

`select_github_release_asset` 改为接收候选列表，外层按候选顺序调用、取第一个非 `None`；其内部「raw > archive」的形状偏好保持不变。

### 3.4 优先级（顺序）是策略

- **musl 优先**：musl 静态二进制在 glibc 与 musl 主机上都能跑；gnu 只在 glibc 上能跑。故 musl-first 严格更优，且符合矩阵收敛目标。
- **代价**：对同时发布 gnu+musl 的产品（如当前的 gops），Linux 客户端会从「拉 gnu」静默切到「拉 musl」。这是期望方向，但属行为变更，应在 release note 中点明。
- 不建议 gnu 优先（Alpine 用户会受影响）。

### 3.5 兼容性

- 现有测试更容易保持绿：例如 `manifest.rs` 的 `parse_v2_release_ok` 用的是 gnu key、无 musl —— 候选为「musl→gnu」时会落到 gnu 命中。若硬切 musl-only，反而会打断它。
- 子串匹配不串味：`x86_64-unknown-linux-gnu` 不是 musl 名字的子串，反之亦然。
- 函数是 `pub(crate)`，仅两个调用点，改名/改签名零外溢。

## 4. 服务端兜底：manifest 过渡别名

客户端一对多**只让未来的二进制变聪明**。存量用户安装的旧二进制仍只会要 gnu key，`inst-x.sh` 同理。因此需要一个**服务端**兜底：在 manifest 生成时，为 glibc 时代的 key 补一个**指向 musl 资产**的别名。

在 `galaxy-ops/.github/workflows/release.yml` 的 "Build assets map" 之后追加：

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

要点：

- **发布资产层仍是三份、无 gnu**（满足控制中心要求）；别名只存在于 manifest 的 key 空间。
- 新旧客户端都能命中：新客户端走 musl key，旧客户端走 gnu 别名，指向同一个 musl 资产。
- 别名是**过渡手段**，不是长期形态。

## 5. 发布与迁移顺序

`gops self` 使用的是**已安装二进制内**的旧逻辑；一对多修复只有在用户升级到带新逻辑的版本后才生效。因此顺序即安全边界：

- **方案 A（干净，慢）**：先发布带候选逻辑的 `wp-self-update` → 各产品升级依赖并发版 → 待存量用户升到位后，再摘除别名/彻底去 gnu。
- **方案 B（快，稳）**：发布矩阵改为 musl 的同时，立刻启用 §4 的服务端别名；新旧客户端都不中断，之后再择机摘别名。
- **推荐**：**A + B 叠加**。B 保证发布期零中断，A 保证最终收敛到真正的 musl-only（届时别名已无消费者，可删）。

**摘除别名的条件**：一个发布周期后确认（或按统计）已无客户端请求 gnu key。

**顺序陷阱（务必避免）**：在没有任何客户端修复、也没有别名的情况下直接发布「无 gnu」矩阵 —— 存量 Linux 用户既无法 self-update，`inst-x.sh` 也取不到包，只能手动下载。

## 6. 影响面清单

| 文件 | 位置 | 改动 |
| --- | --- | --- |
| `crates/wp-self-update/src/platform.rs` | `:3-12` | 单值 → 候选列表 |
| `crates/wp-self-update/src/manifest.rs` | `:49-59` 及测试 | 候选遍历取首个命中；报错列候选 |
| `crates/wp-self-update/src/fetch.rs` | `:169-183, 219-243` 及测试 | `select_github_release_asset` 支持候选 |
| `crates/wp-self-update/Cargo.toml` | version | 版本递增，发布 |
| `galaxy-ops/Cargo.toml` / `Cargo.lock` | `wp-self-update` 依赖 | 升级到新版本 |
| `galaxy-ops/.github/workflows/release.yml` | manifest job | 追加过渡别名 key |
| `crates/wp-installer` | 复用同一 `platform.rs` | 自动受益，无需单独改动 |

**不受影响**（已核对）：

- 打包布局 `artifacts/gops` → `<repo>-<version>-<triple>/gops` 不影响安装：`install.rs:463-494 find_extracted_bins`（`Bins` 目标）与 `:496-543 discover_extracted_bins`（`Auto` 目标）都**递归**遍历解压树，`artifacts` 只是命名偏好而非硬约束。
- `ResolvedRelease.target`、`CheckReport.platform_key`、`install_bins`、健康检查/回滚：均仍是单值语义。

## 7. 验证

单元测试：

- 候选命中：manifest 仅含 musl → 命中 musl；仅含 gnu → 回退命中 gnu；两者都有 → 命中 musl。
- 全缺 → 报错信息含 candidates 与可用 keys。
- GitHub 来源：asset 命名按候选顺序匹配，且「raw > archive」偏好不回归。
- 既有 `parse_v2_release_*` / `select_*` 测试保持通过。

端到端（发布后）：

- manifest 含三个 musl/darwin key；过渡期另含两个 gnu 别名。
- `file <bin>` → `statically linked`；`ldd <bin>` → `not a dynamic executable`。
- Alpine 与 Ubuntu 上 `<bin> --version` 均可运行。
- `gops self check/update` 在 Linux x86_64、macOS arm64 上正常；旧版 `gops` 借助别名仍可升级。

## 8. 验收标准

- Linux 上 `gops self update` 与 `inst-x.sh gops` 恢复正常（新客户端走 musl）。
- 过渡期内，未升级的旧客户端依靠别名同样不中断。
- 发布矩阵仍为三份、无 gnu 制品，符合控制中心平台集。
- 摘除别名后行为不变（无消费者）。

## 9. 待定决策

1. **候选顺序**：确认 musl-first（推荐）。
2. **别名保留时长**：按发布周期还是按客户端统计决定摘除时机。
3. **范围**：`gx`（galaxy-flow）是否同步收敛矩阵 / 升级 `wp-self-update` 依赖。
4. **命名**：`detect_target_triple_v2` 是否重命名为 `detect_target_candidates`。
