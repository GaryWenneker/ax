//! Open Knowledge Format (OKF) bundle export (`ax export okf`).

use std::path::PathBuf;

use ax_core::{
    export_okf_bundle, publish_okf_wiki, validate_okf_bundle, OkfConfig, OkfExportOptions,
    OkfPublishOptions,
};

use crate::commands::resolve_path;
use crate::ui::SpinnerGuard;

pub struct ExportOkfArgs {
    pub path: Option<String>,
    pub out: Option<String>,
    pub limit: usize,
    pub check: bool,
    pub ci: bool,
    pub publish_wiki: bool,
    pub dry_run: bool,
    pub no_push: bool,
    pub json: bool,
}

pub async fn run(args: ExportOkfArgs) -> Result<(), String> {
    let root = resolve_path(args.path);
    let cfg = OkfConfig::load(&root);
    let out_override = args.out.map(PathBuf::from);
    let out_dir = match &out_override {
        Some(p) if p.is_absolute() => p.clone(),
        Some(p) => root.join(p),
        None => cfg.out_dir_abs(&root),
    };

    ax_usage::log_cli(
        Some(&root),
        format!(
            "cmd=export-okf start out={} check={} publish_wiki={}",
            out_dir.display(),
            args.check,
            args.publish_wiki
        ),
    );

    {
        let _spinner = SpinnerGuard::new(
            "Exporting Open Knowledge Format (OKF) bundle...",
            false,
        );
        let ax = ax_core::Ax::open(&root).await.map_err(|e| e.to_string())?;
        let nodes = ax.queries().get_all_nodes().await.map_err(|e| e.to_string())?;
        let edges = ax.queries().get_all_edges().await.map_err(|e| e.to_string())?;
        let report = export_okf_bundle(
            &root,
            &nodes,
            &edges,
            &OkfExportOptions {
                out: out_override.clone(),
                limit: args.limit,
            },
        )?;
        if !args.json {
            println!(
                "Open Knowledge Format (OKF): exported {} concepts into {}",
                report.exported,
                report.out_dir.display()
            );
            for (kind, count) in &report.by_kind {
                println!("  {kind:<16} {count}");
            }
        }
        ax_usage::log_cli(
            Some(&root),
            format!("cmd=export-okf ok count={}", report.exported),
        );
    }

    if args.check {
        let _spinner = SpinnerGuard::new("Validating Open Knowledge Format (OKF) bundle...", false);
        let report = validate_okf_bundle(&out_dir)?;
        if args.json {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "ok": report.ok,
                    "missingIndex": report.missing_index,
                    "pages": report.pages,
                    "danglingLinks": report.dangling_links,
                    "outDir": out_dir.display().to_string(),
                }))
                .unwrap_or_else(|_| "{}".into())
            );
        } else if report.ok {
            println!(
                "Open Knowledge Format (OKF) — no issues found ({} concept pages)",
                report.pages
            );
        } else {
            if report.missing_index {
                println!("error: missing index.md in {}", out_dir.display());
            }
            for link in &report.dangling_links {
                println!("error: dangling link {link}");
            }
            println!(
                "Open Knowledge Format (OKF) — {} issue(s)",
                usize::from(report.missing_index) + report.dangling_links.len()
            );
        }
        if args.ci && !report.ok {
            return Err("OKF bundle validation failed (--ci)".into());
        }
    }

    if args.publish_wiki {
        let _spinner = SpinnerGuard::new(
            "Publishing Open Knowledge Format (OKF) bundle to wiki...",
            false,
        );
        let pub_report = publish_okf_wiki(
            &root,
            &out_dir,
            &OkfPublishOptions {
                dry_run: args.dry_run,
                no_push: args.no_push,
            },
        )?;
        if args.json {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "wikiAction": pub_report.wiki_action,
                    "subdir": pub_report.subdir.display().to_string(),
                    "filesCopied": pub_report.files_copied,
                    "committed": pub_report.committed,
                    "pushed": pub_report.pushed,
                    "dryRun": pub_report.dry_run,
                }))
                .unwrap_or_else(|_| "{}".into())
            );
        } else if pub_report.dry_run {
            println!(
                "Open Knowledge Format (OKF) wiki publish dry-run: {} file(s) → {}",
                pub_report.files_copied,
                pub_report.subdir.display()
            );
        } else {
            println!(
                "Open Knowledge Format (OKF) wiki: {} — copied {} file(s) to {}",
                pub_report.wiki_action,
                pub_report.files_copied,
                pub_report.subdir.display()
            );
            if pub_report.committed {
                println!(
                    "  committed{}",
                    if pub_report.pushed {
                        " and pushed"
                    } else if args.no_push {
                        " (push skipped --no-push)"
                    } else {
                        ""
                    }
                );
            } else {
                println!("  no wiki changes to commit");
            }
        }
    }

    Ok(())
}
