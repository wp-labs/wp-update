use crate::error::{InstallerReason, InstallerResult};
use crate::skills::{SkillCheckReport, SkillInstallReport};
use crate::source::CUSTOM_PRODUCT_LABEL;
use wp_self_update::{CheckReport, UpdateReport};

pub(crate) fn print_check_report(json: bool, report: &CheckReport) -> InstallerResult<()> {
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(report).map_err(|e| {
                orion_error::StructError::builder(InstallerReason::OutputFailed)
                    .detail("failed to serialize check report")
                    .source_std(e)
                    .finish()
            })?
        );
        return Ok(());
    }
    println!("{} check", display_product_label(&report.product));
    println!("  Channel  : {}", report.channel);
    println!("  Current  : {}", report.current_version);
    println!("  Latest   : {}", report.latest_version);
    println!("  Target   : {}", report.platform_key);
    println!("  Artifact : {}", report.artifact);
    println!(
        "  Status   : {}",
        if report.update_available {
            "update available"
        } else {
            "up-to-date"
        }
    );
    Ok(())
}

pub(crate) fn print_update_report(
    action: &str,
    json: bool,
    report: &UpdateReport,
) -> InstallerResult<()> {
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(report).map_err(|e| {
                orion_error::StructError::builder(InstallerReason::OutputFailed)
                    .detail("failed to serialize update report")
                    .source_std(e)
                    .finish()
            })?
        );
        return Ok(());
    }
    println!("{} {}", display_product_label(&report.product), action);
    println!("  Channel  : {}", report.channel);
    println!("  Current  : {}", report.current_version);
    println!("  Latest   : {}", report.latest_version);
    println!("  Install  : {}", report.install_dir);
    println!("  Artifact : {}", report.artifact);
    println!("  Status   : {}", report.status);
    Ok(())
}

pub(crate) fn print_skill_check_report(
    json: bool,
    report: &SkillCheckReport,
) -> InstallerResult<()> {
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(report).map_err(|e| {
                orion_error::StructError::builder(InstallerReason::OutputFailed)
                    .detail("failed to serialize skill check report")
                    .source_std(e)
                    .finish()
            })?
        );
        return Ok(());
    }
    println!("{} check", report.skill);
    println!("  Repo     : {}", report.repo);
    println!("  Path     : {}", report.path);
    println!("  Tag      : {}", report.tag);
    println!("  Archive  : {}", report.archive);
    println!("  Status   : {}", report.status);
    Ok(())
}

pub(crate) fn print_skill_install_report(
    json: bool,
    report: &SkillInstallReport,
) -> InstallerResult<()> {
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(report).map_err(|e| {
                orion_error::StructError::builder(InstallerReason::OutputFailed)
                    .detail("failed to serialize skill install report")
                    .source_std(e)
                    .finish()
            })?
        );
        return Ok(());
    }
    println!("Installed: {}", report.skill);
    println!("Source   : {}", report.repo);
    println!("Path     : {}", report.path);
    println!("Tag      : {}", report.tag);
    println!("Archive  : {}", report.archive);
    for install in &report.locations {
        println!("Platform : {}", install.platform);
        println!("Location : {}", install.location);
        if install.files.is_empty() {
            continue;
        }
        println!("Files    :");
        for file in install.files.iter().take(20) {
            println!("  - {}", file);
        }
        if install.files.len() > 20 {
            println!("  - ... and {} more", install.files.len() - 20);
        }
    }
    Ok(())
}

fn display_product_label(product: &str) -> &str {
    if product == CUSTOM_PRODUCT_LABEL {
        "wp-inst"
    } else {
        product
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_product_label_uses_github_repo_name() {
        assert_eq!(display_product_label("wpl-check"), "wpl-check");
    }

    #[test]
    fn display_product_label_falls_back_to_wp_inst_for_manifest_mode() {
        assert_eq!(display_product_label(CUSTOM_PRODUCT_LABEL), "wp-inst");
    }
}
