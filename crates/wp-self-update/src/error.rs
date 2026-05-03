use derive_more::From;
use orion_error::conversion::ToStructError;
use orion_error::{OrionError, StructError, UnifiedReason};

#[derive(Debug, Clone, PartialEq, From, OrionError)]
pub enum UpdateReason {
    #[orion_error(identity = "conf.update_invalid_request")]
    InvalidRequest,
    #[orion_error(identity = "sys.update_install_failed")]
    InstallFailed,
    #[orion_error(identity = "sys.update_remote_fetch_failed")]
    RemoteFetchFailed,
    #[orion_error(identity = "sys.update_integrity_check_failed")]
    IntegrityCheckFailed,
    #[orion_error(identity = "logic.update_state_conflict")]
    StateConflict,
    #[orion_error(transparent)]
    Uvs(UnifiedReason),
}

pub type UpdateError = StructError<UpdateReason>;
pub type UpdateResult<T> = Result<T, UpdateError>;

pub(crate) fn invalid_request(detail: impl Into<String>) -> UpdateError {
    UpdateReason::InvalidRequest.to_err().with_detail(detail)
}

pub(crate) fn install_failed(detail: impl Into<String>) -> UpdateError {
    UpdateReason::InstallFailed.to_err().with_detail(detail)
}

pub(crate) fn remote_fetch_failed(detail: impl Into<String>) -> UpdateError {
    UpdateReason::RemoteFetchFailed.to_err().with_detail(detail)
}

pub(crate) fn integrity_check_failed(detail: impl Into<String>) -> UpdateError {
    UpdateReason::IntegrityCheckFailed.to_err().with_detail(detail)
}

pub(crate) fn state_conflict(detail: impl Into<String>) -> UpdateError {
    UpdateReason::StateConflict.to_err().with_detail(detail)
}
