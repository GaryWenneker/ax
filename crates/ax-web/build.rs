use std::path::Path;
use std::process::Command;

fn main() {
    let manifest = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let web_ui = Path::new(&manifest).join("web-ui");

    // Rerun if frontend source changes.
    println!("cargo:rerun-if-changed=web-ui/src");
    println!("cargo:rerun-if-changed=web-ui/index.html");
    println!("cargo:rerun-if-changed=web-ui/package.json");
    println!("cargo:rerun-if-env-changed=AX_SKIP_WEB_BUILD");
    println!("cargo:rerun-if-env-changed=AX_FORCE_WEB_BUILD");

    if std::env::var("AX_SKIP_WEB_BUILD").is_ok() {
        // Caller opted out — dist/ must already exist.
        return;
    }

    let dist = web_ui.join("dist");
    let force = std::env::var("AX_FORCE_WEB_BUILD").is_ok();

    if dist.exists() && !force {
        // Already built; skip unless forced.
        return;
    }

    if !web_ui.join("package.json").exists() {
        panic!("ax-web: web-ui/package.json not found. Run `npm install && npm run build` in crates/ax-web/web-ui/ first.");
    }

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
}
