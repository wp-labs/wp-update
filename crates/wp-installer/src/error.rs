use derive_more::From;
use orion_error::conversion::ToStructError;
use orion_error::{OrionError, StructError, UnifiedReason};

#[derive(Debug, Clone, PartialEq, From, OrionError)]
pub(crate) enum InstallerReason {
    #[orion_error(identity = "conf.installer_invalid_request")]
    InvalidRequest,
    #[orion_error(identity = "sys.installer_output_failed")]
    OutputFailed,
    #[orion_error(identity = "sys.installer_skill_install_failed")]
    SkillInstallFailed,
    #[orion_error(identity = "sys.installer_self_update_failed")]
    SelfUpdateFailed,
    #[orion_error(transparent)]
    Uvs(UnifiedReason),
}

pub(crate) type InstallerError = StructError<InstallerReason>;
pub(crate) type InstallerResult<T> = Result<T, InstallerError>;

pub(crate) fn invalid_request(detail: impl Into<String>) -> InstallerError {
    InstallerReason::InvalidRequest.to_err().with_detail(detail)
}

pub(crate) fn skill_install_failed(detail: impl Into<String>) -> InstallerError {
    InstallerReason::SkillInstallFailed.to_err().with_detail(detail)
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
}
