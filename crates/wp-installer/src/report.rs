use crate::cli::SourceArgs;
use crate::source::CUSTOM_PRODUCT_LABEL;
use wp_self_update::{CheckReport, UpdateReport};

pub(crate) fn print_check_report(
    source: &SourceArgs,
    report: &CheckReport,
) -> Result<(), Box<dyn std::error::Error>> {
    if source.json {
        println!("{}", serde_json::to_string_pretty(report)?);
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
    source: &SourceArgs,
    report: &UpdateReport,
) -> Result<(), Box<dyn std::error::Error>> {
    if source.json {
        println!("{}", serde_json::to_string_pretty(report)?);
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
