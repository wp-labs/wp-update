use crate::error::{invalid_request, UpdateResult};

/// 当前平台的目标 triple 候选，按优先级排列；调用方按序取第一个命中者。
///
/// Linux 采用「musl 优先、glibc 兜底」：musl 静态二进制在 glibc 与 musl 主机上都能运行，
/// 而 gnu 版仅能在 glibc 主机上运行。gnu 候选仅为兼容仍只发布 gnu 制品的旧发布保留，
/// 待其退场后可移除（见 `docs/wp-self-update-platform-target.zh-CN.md`）。
pub(crate) fn detect_target_candidates() -> UpdateResult<&'static [&'static str]> {
    Ok(match (std::env::consts::OS, std::env::consts::ARCH) {
        ("linux", "x86_64") => &["x86_64-unknown-linux-musl", "x86_64-unknown-linux-gnu"],
        ("linux", "aarch64") => &["aarch64-unknown-linux-musl", "aarch64-unknown-linux-gnu"],
        ("macos", "aarch64") => &["aarch64-apple-darwin"],
        (os, arch) => {
            return Err(invalid_request(format!(
                "unsupported platform: {os}-{arch}"
            )));
        }
    })
}
