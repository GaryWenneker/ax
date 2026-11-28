//! Builds the browser UI (Vite) and stages it for embedding in the binary.
//! `AX_SKIP_WEB_BUILD=1` skips npm and embeds whatever `web-ui/dist` holds, or
//! a placeholder page when it holds nothing.
use std::path::Path;
use std::process::Command;

fn main() {
    let manifest = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
    let web_ui = Path::new(&manifest).join("web-ui");
    let dist = web_ui.join("dist");
    for path in [
        "src",
        "index.html",
        "package.json",
        "package-lock.json",
        "vite.config.ts",
        "dist/index.html",
    ] {
        println!("cargo:rerun-if-changed=web-ui/{path}");
    }
    // vite.config.ts aliases `@cc` to the Command Center sources, and their bare
    // imports resolve through that package's own dependencies.
    for path in ["src", "package-lock.json"] {
        println!("cargo:rerun-if-changed=../ax-web/web-ui/{path}");
    }
    println!("cargo:rerun-if-env-changed=AX_SKIP_WEB_BUILD");

    if std::env::var("AX_SKIP_WEB_BUILD").is_err() {
        let npm = if cfg!(windows) { "npm.cmd" } else { "npm" };
        // `npm ci` installs exactly what package-lock.json lists and never
        // rewrites it. It runs only when node_modules is missing or older than
        // the lock file, so an ordinary rebuild needs no network.
        let install = &["ci", "--prefer-offline"][..];
        let steps: Vec<&[&str]> = if needs_install(&web_ui) {
            vec![install, &["run", "build"][..]]
        } else {
            vec![&["run", "build"][..]]
        };
        for args in steps {
            let ok = Command::new(npm)
                .args(args)
                .current_dir(&web_ui)
                .status()
                .map(|s| s.success())
                .unwrap_or(false);
            if !ok {
                panic!(
                    "ax-admin: npm {} failed in web-ui/. Set AX_SKIP_WEB_BUILD=1 to skip.",
                    args.join(" ")
                );
            }
        }
    }

    let out = Path::new(&std::env::var("OUT_DIR").expect("OUT_DIR")).join("admin_dist");
    let _ = std::fs::remove_dir_all(&out);
    std::fs::create_dir_all(&out).expect("create staging directory");
    if dist.join("index.html").is_file() {
        copy_dir(&dist, &out);
    } else {
        std::fs::write(
            out.join("index.html"),
            "<!doctype html><html lang=\"en\"><head><meta charset=\"UTF-8\"><title>ax-admin</title></head><body><p>ax-admin was built without its UI. Run <code>npm run build</code> in crates/ax-admin/web-ui and rebuild.</p></body></html>\n",
        )
        .expect("write placeholder page");
    }
}

/// True when `node_modules` does not match `package-lock.json`: npm writes
/// `node_modules/.package-lock.json` on every install, so a lock file newer
/// than that copy (or no copy) means the dependencies must be installed.
fn needs_install(web_ui: &Path) -> bool {
    let modified = |path: &Path| std::fs::metadata(path).and_then(|m| m.modified()).ok();
    match (
        modified(&web_ui.join("package-lock.json")),
        modified(&web_ui.join("node_modules/.package-lock.json")),
    ) {
        (Some(lock), Some(installed)) => lock > installed,
        _ => true,
    }
}

fn copy_dir(from: &Path, to: &Path) {
    for entry in std::fs::read_dir(from).expect("read dist") {
        let entry = entry.expect("read UI file entry");
        let target = to.join(entry.file_name());
        if entry.path().is_dir() {
            std::fs::create_dir_all(&target).expect("create directory");
            copy_dir(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), &target).expect("copy UI file");
        }
    }
}
