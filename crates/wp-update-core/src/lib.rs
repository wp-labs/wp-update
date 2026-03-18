mod manifest;
mod platform;
mod types;
mod versioning;

pub use manifest::updates_manifest_url;
pub use types::{
    CheckReport, CheckRequest, ResolvedRelease, SourceConfig, UpdateChannel, UpdateProduct,
    UpdateReport, UpdateRequest, VersionRelation,
};
pub use versioning::{compare_versions_str, relation_message};

#[doc(hidden)]
pub use manifest::{parse_v2_release, updates_manifest_path};
#[doc(hidden)]
pub use versioning::validate_artifact_version_consistency;
