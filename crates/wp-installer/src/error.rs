use derive_more::From;
use orion_error::conversion::ToStructError;
use orion_error::prelude::SourceErr;
use orion_error::{OrionError, StructError, UnifiedReason};
use wp_self_update::UpdateError;

#[derive(Debug, Clone, PartialEq, From, OrionError)]
pub(crate) enum InstallerReason {
    #[orion_error(identity = "conf.installer_invalid_request")]
    InvalidRequest,
    #[orion_error(identity = "sys.installer_output_failed")]
    OutputFailed,
    #[orion_error(identity = "sys.installer_check_failed")]
    CheckFailed,
    #[orion_error(identity = "sys.installer_install_failed")]
    InstallFailed,
    #[orion_error(identity = "sys.installer_skill_install_failed")]
    SkillInstallFailed,
    #[orion_error(transparent)]
    Uvs(UnifiedReason),
}

pub(crate) type InstallerError = StructError<InstallerReason>;
pub(crate) type InstallerResult<T> = Result<T, InstallerError>;

pub(crate) fn invalid_request(detail: impl Into<String>) -> InstallerError {
    InstallerReason::InvalidRequest.to_err().with_detail(detail)
}

pub(crate) fn skill_install_failed(detail: impl Into<String>) -> InstallerError {
    InstallerReason::SkillInstallFailed
        .to_err()
        .with_detail(detail)
}

/// 把底层 `wp_self_update` 错误的 reason + detail 合成一行，便于 `render()` 直接看到根因。
pub(crate) fn describe_update_error(err: &UpdateError) -> String {
    match err.detail() {
        Some(detail) => format!("{}: {}", err.reason(), detail),
        None => format!("{}", err.reason()),
    }
}

/// 用给定 reason 包裹底层 update 错误：把根因写进 `detail`，同时保留 source 链。
///
/// 背景：`render()` 只打印 reason/detail，不含 source 链；若 detail 只写「失败动作」，
/// 调用方就看不到真正的失败原因（此前 `wp-inst install` 只会报
/// 「installer self update failed」，即使真实原因是安装目录不存在）。
pub(crate) fn update_failed(
    reason: InstallerReason,
    what: &str,
    err: UpdateError,
) -> InstallerError {
    let detail = format!("{what}: {}", describe_update_error(&err));
    let wrapped: Result<(), UpdateError> = Err(err);
    wrapped
        .source_err(reason, detail)
        .expect_err("constructed Err always yields Err")
}

#[cfg(test)]
mod tests {
    use super::*;
    use orion_error::protocol::DefaultExposurePolicy;

    #[test]
    fn installer_error_projects_to_cli_json() {
        let json = invalid_request("bad cli args")
            .exposure(&DefaultExposurePolicy)
            .to_cli_error_json()
            .expect("cli json");

        assert_eq!(
            json["code"],
            serde_json::json!("conf.installer_invalid_request")
        );
        assert_eq!(json["category"], serde_json::json!("conf"));
        assert!(json["summary"]
            .as_str()
            .unwrap_or_default()
            .contains("bad cli args"));
    }

    #[test]
    fn update_failed_folds_cause_into_detail_and_keeps_source_chain() {
        use wp_self_update::UpdateReason;

        let source = UpdateReason::InstallFailed
            .to_err()
            .with_detail("failed to access install dir /root/bin: No such file or directory");
        let wrapped = update_failed(
            InstallerReason::InstallFailed,
            "failed to install binary update",
            source,
        );

        assert_eq!(wrapped.reason(), &InstallerReason::InstallFailed);
        let rendered = wrapped.render();
        // 根因进入 detail，render() 即可见
        assert!(rendered.contains("No such file or directory"), "{rendered}");
        assert!(
            rendered.contains("failed to install binary update"),
            "{rendered}"
        );
        // source 链仍保留（JSON / expose 路径可用）
        assert!(!wrapped.source_frames().is_empty());
    }
}
