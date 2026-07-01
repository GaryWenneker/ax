use std::path::Path;
use std::process::Command;

fn main() {
    let manifest = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let web_ui = Path::new(&manifest).join("web-ui");

    println!("cargo:rerun-if-changed=web-ui/src");
    println!("cargo:rerun-if-changed=web-ui/index.html");
    println!("cargo:rerun-if-changed=web-ui/package.json");
    println!("cargo:rerun-if-changed=web-ui/package-lock.json");
    println!("cargo:rerun-if-env-changed=AX_SKIP_WEB_BUILD");
    println!("cargo:rerun-if-env-changed=AX_FORCE_WEB_BUILD");

    let dist = web_ui.join("dist");

    if std::env::var("AX_SKIP_WEB_BUILD").is_ok() {
        register_dist_rerun(&dist);
        return;
    }

    if !web_ui.join("package.json").exists() {
        panic!(
            "ax-web: web-ui/package.json not found. Run `npm install && npm run build` in crates/ax-web/web-ui/ first."
        );
    }

    // Never skip npm when this build script runs: committed dist/ can be stale while src changed.
    // AX_FORCE_WEB_BUILD is kept for manual force (same behavior as default now).
    let _force = std::env::var("AX_FORCE_WEB_BUILD").is_ok();

    let npm = if cfg!(windows) { "npm.cmd" } else { "npm" };

    let install_ok = Command::new(npm)
        .args(["install", "--prefer-offline"])
        .current_dir(&web_ui)
        .status()
        .map(|s| s.success())
        .unwrap_or(false);

    if !install_ok {
        panic!("ax-web: npm install failed in web-ui/. Set AX_SKIP_WEB_BUILD=1 to skip.");
    }

    let build_ok = Command::new(npm)
        .args(["run", "build"])
        .current_dir(&web_ui)
        .status()
        .map(|s| s.success())
        .unwrap_or(false);

    if !build_ok {
        panic!("ax-web: npm run build failed in web-ui/. Set AX_SKIP_WEB_BUILD=1 to skip.");
    }

    register_dist_rerun(&dist);
}

/// Ensure `include_dir!(web-ui/dist)` recompiles when Vite output changes.
fn register_dist_rerun(dist: &Path) {
    if !dist.is_dir() {
        return;
    }
    walk_dist(dist, dist);
}

fn walk_dist(root: &Path, dir: &Path) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk_dist(root, &path);
        } else if let Ok(rel) = path.strip_prefix(root) {
            let key = rel.to_string_lossy().replace('\\', "/");
            println!("cargo:rerun-if-changed=web-ui/dist/{key}");
        }
    }
}
