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
        stamp_embedded_index(&dist);
        stage_web_dist_for_embed(&dist);
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
    stamp_embedded_index(&dist);
    stage_web_dist_for_embed(&dist);
}

/// Force rustc to rerun `include_dir!` after Vite writes a new `dist/index.html`.
/// Running only the build script does not rebuild the rlib, so Command Center can
/// keep serving a stale JS bundle.
fn stamp_embedded_index(dist: &Path) {
    let index = dist.join("index.html");
    let body = std::fs::read_to_string(&index).unwrap_or_default();
    let mut hash: u64 = 0xcbf29ce484222325;
    for b in body.as_bytes() {
        hash ^= u64::from(*b);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    println!("cargo:rustc-env=AX_WEB_DIST_STAMP={hash:016x}");
    println!("cargo:rerun-if-changed=web-ui/dist/index.html");
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

/// Copy Vite output into OUT_DIR so `include_dir!` cannot reuse a stale compile of `web-ui/dist`.
fn stage_web_dist_for_embed(dist: &Path) {
    let out = Path::new(&std::env::var("OUT_DIR").expect("OUT_DIR")).join("web_dist");
    let _ = std::fs::remove_dir_all(&out);
    copy_dir(dist, &out);
    let index = std::fs::read_to_string(out.join("index.html")).unwrap_or_default();
    if let Some(src) = index.split("src=\"").nth(1).and_then(|s| s.split('"').next()) {
        println!("cargo:warning=ax-web embedding {src}");
    }
    let path_lit = out.to_string_lossy().replace('\\', "\\\\");
    let gen = Path::new(&std::env::var("OUT_DIR").expect("OUT_DIR")).join("web_dist.rs");
    std::fs::write(
        &gen,
        format!("static WEB_DIST: Dir = include_dir::include_dir!(\"{path_lit}\");\n"),
    )
    .expect("write web_dist.rs");
}

fn copy_dir(src: &Path, dst: &Path) {
    std::fs::create_dir_all(dst).expect("create OUT_DIR/web_dist");
    for entry in std::fs::read_dir(src).expect("read web-ui/dist") {
        let entry = entry.expect("dist entry");
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if from.is_dir() {
            copy_dir(&from, &to);
        } else {
            std::fs::copy(&from, &to).expect("copy dist file");
        }
    }
}
