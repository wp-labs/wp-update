use derive_more::From;
use orion_error::{OrionError, StructError, UvsReason};

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
    Uvs(UvsReason),
}

pub(crate) type InstallerError = StructError<InstallerReason>;
pub(crate) type InstallerResult<T> = Result<T, InstallerError>;

pub(crate) fn invalid_request(detail: impl Into<String>) -> InstallerError {
    StructError::from(InstallerReason::InvalidRequest).with_detail(detail)
}

pub(crate) fn skill_install_failed(detail: impl Into<String>) -> InstallerError {
    StructError::from(InstallerReason::SkillInstallFailed).with_detail(detail)
}
