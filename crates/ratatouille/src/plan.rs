use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Host {
    Unix,
    Windows,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Website,
    WebsiteStart,
    Ax,
    Deploy,
    PullRebuild,
    Publish,
}

impl Action {
    pub const ALL: [Action; 6] = [
        Action::Website,
        Action::WebsiteStart,
        Action::Ax,
        Action::Deploy,
        Action::PullRebuild,
        Action::Publish,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Action::Website => "Website — build and run",
            Action::WebsiteStart => "Website — start",
            Action::Ax => "ax — build and run locally",
            Action::Deploy => "Deploy website to Netlify",
            Action::PullRebuild => "Pull main and rebuild locally",
            Action::Publish => "Publish release and deploy the website",
        }
    }

    pub fn detail(self) -> &'static str {
        match self {
            Action::Website => "Build the Astro site and serve the result on 127.0.0.1:4321.",
            Action::WebsiteStart => "Start the Astro dev server on 127.0.0.1:4321. Does not build the production site.",
            Action::Ax => "Rebuild the local ax binary, then start ax web on port 7070. Other ax processes stay running.",
            Action::Deploy => "Production deploy of getax.wenneker.io. Requires a clean main that matches origin and a green CI run.",
            Action::PullRebuild => "Fast-forward to origin/main, then rebuild the local ax binary. Does not start the server.",
            Action::Publish => "Read the changes since the published release and choose micro, minor, or major. A new number has to reach main through a PR before it is tagged.",
        }
    }

    pub fn needs_confirm(self) -> bool {
        matches!(self, Action::Deploy | Action::Publish)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Step {
    pub label: String,
    pub program: String,
    pub args: Vec<String>,
    pub cwd: PathBuf,
}

pub fn plan(action: Action, root: &Path, host: Host, site_deps_installed: bool) -> Vec<Step> {
    match action {
        Action::Website => website(root, host, site_deps_installed),
        Action::WebsiteStart => website_start(root, host, site_deps_installed),
        Action::Ax => ax_local(root, host, true),
        Action::Deploy => vec![pwsh_file(
            root,
            "Deploy website to Netlify",
            "scripts/deploy-netlify.ps1",
            &[],
        )],
        Action::PullRebuild => {
            let mut steps = vec![Step {
                label: "Pull origin/main".to_string(),
                program: "git".to_string(),
                args: ["pull", "--ff-only", "origin", "main"]
                    .map(str::to_string)
                    .to_vec(),
                cwd: root.to_path_buf(),
            }];
            steps.extend(reinstall(root, host));
            steps
        }
        Action::Publish => vec![
            pwsh_file(
                root,
                "Publish GitHub release",
                "scripts/release-tag.ps1",
                &["-Wait"],
            ),
            pwsh_file(
                root,
                "Deploy website to Netlify",
                "scripts/deploy-netlify.ps1",
                &[],
            ),
        ],
    }
}

fn website_start(root: &Path, host: Host, site_deps_installed: bool) -> Vec<Step> {
    let site = root.join("site");
    let mut steps = Vec::new();
    if !site_deps_installed {
        steps.push(npm(host, &site, "Install site dependencies", &["ci"]));
    }
    steps.push(npm(
        host,
        &site,
        "Start website",
        &[
            "run",
            "start",
            "--",
            "--host",
            "127.0.0.1",
            "--port",
            "4321",
        ],
    ));
    steps
}

fn website(root: &Path, host: Host, site_deps_installed: bool) -> Vec<Step> {
    let site = root.join("site");
    let mut steps = Vec::new();
    if !site_deps_installed {
        steps.push(npm(host, &site, "Install site dependencies", &["ci"]));
    }
    steps.push(npm(host, &site, "Build website", &["run", "build"]));
    steps.push(npm(
        host,
        &site,
        "Run website preview",
        &[
            "run",
            "preview",
            "--",
            "--host",
            "127.0.0.1",
            "--port",
            "4321",
        ],
    ));
    steps
}

fn ax_local(root: &Path, host: Host, start_web: bool) -> Vec<Step> {
    let mut steps = reinstall(root, host);
    if start_web {
        let program = match host {
            Host::Unix => root.join("target-dev/release/ax"),
            Host::Windows => root.join("target-dev/release/ax.exe"),
        };
        steps.push(Step {
            label: "Run ax web".to_string(),
            program: program.to_string_lossy().into_owned(),
            args: ["web", "--port", "7070"].map(str::to_string).to_vec(),
            cwd: root.to_path_buf(),
        });
    }
    steps
}

fn reinstall(root: &Path, host: Host) -> Vec<Step> {
    match host {
        Host::Unix => vec![Step {
            label: "Rebuild local ax".to_string(),
            program: "bash".to_string(),
            args: vec![root
                .join("scripts/reinstall-cli.sh")
                .to_string_lossy()
                .into_owned()],
            cwd: root.to_path_buf(),
        }],
        Host::Windows => vec![Step {
            label: "Rebuild local ax".to_string(),
            program: "powershell".to_string(),
            args: vec![
                "-NoProfile".to_string(),
                "-File".to_string(),
                root.join("scripts/reinstall-cli.ps1")
                    .to_string_lossy()
                    .into_owned(),
            ],
            cwd: root.to_path_buf(),
        }],
    }
}

fn npm(host: Host, cwd: &Path, label: &str, npm_args: &[&str]) -> Step {
    match host {
        Host::Unix => Step {
            label: label.to_string(),
            program: "npm".to_string(),
            args: npm_args.iter().map(|arg| (*arg).to_string()).collect(),
            cwd: cwd.to_path_buf(),
        },
        Host::Windows => {
            let mut args = vec!["/C".to_string(), "npm".to_string()];
            args.extend(npm_args.iter().map(|arg| (*arg).to_string()));
            Step {
                label: label.to_string(),
                program: "cmd".to_string(),
                args,
                cwd: cwd.to_path_buf(),
            }
        }
    }
}

fn pwsh_file(root: &Path, label: &str, script: &str, extra: &[&str]) -> Step {
    let mut args = vec![
        "-NoProfile".to_string(),
        "-File".to_string(),
        root.join(script).to_string_lossy().into_owned(),
    ];
    args.extend(extra.iter().map(|arg| (*arg).to_string()));
    Step {
        label: label.to_string(),
        program: "pwsh".to_string(),
        args,
        cwd: root.to_path_buf(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root() -> PathBuf {
        PathBuf::from("/repo")
    }

    #[test]
    fn website_builds_then_previews_when_dependencies_exist() {
        let steps = plan(Action::Website, &root(), Host::Unix, true);
        assert_eq!(steps.len(), 2);
        assert_eq!(steps[0].program, "npm");
        assert_eq!(steps[0].args, ["run", "build"]);
        assert_eq!(steps[0].cwd, PathBuf::from("/repo/site"));
        assert_eq!(
            steps[1].args,
            [
                "run",
                "preview",
                "--",
                "--host",
                "127.0.0.1",
                "--port",
                "4321"
            ]
        );
    }

    #[test]
    fn website_start_serves_the_dev_server_without_a_build() {
        let steps = plan(Action::WebsiteStart, &root(), Host::Unix, true);
        assert_eq!(steps.len(), 1);
        assert_eq!(steps[0].cwd, PathBuf::from("/repo/site"));
        assert_eq!(
            steps[0].args,
            [
                "run",
                "start",
                "--",
                "--host",
                "127.0.0.1",
                "--port",
                "4321"
            ]
        );
        assert!(steps
            .iter()
            .all(|step| !step.args.iter().any(|arg| arg == "build")));
        assert!(!Action::WebsiteStart.needs_confirm());
    }

    #[test]
    fn website_start_installs_dependencies_first_when_missing() {
        let steps = plan(Action::WebsiteStart, &root(), Host::Unix, false);
        assert_eq!(steps[0].args, ["ci"]);
        assert_eq!(
            steps[1].args,
            [
                "run",
                "start",
                "--",
                "--host",
                "127.0.0.1",
                "--port",
                "4321"
            ]
        );
    }

    #[test]
    fn website_installs_dependencies_first_when_missing() {
        let steps = plan(Action::Website, &root(), Host::Unix, false);
        assert_eq!(steps[0].args, ["ci"]);
        assert_eq!(steps[1].args, ["run", "build"]);
    }

    #[test]
    fn ax_rebuilds_then_starts_web_without_a_kill_flag() {
        let steps = plan(Action::Ax, &root(), Host::Unix, true);
        assert_eq!(steps[0].program, "bash");
        assert_eq!(steps[0].args, ["/repo/scripts/reinstall-cli.sh"]);
        assert!(steps[0].args.iter().all(|arg| !arg.contains("pkill")));
        assert_eq!(steps[1].program, "/repo/target-dev/release/ax");
        assert_eq!(steps[1].args, ["web", "--port", "7070"]);
    }

    #[test]
    fn windows_rebuild_uses_the_powershell_script() {
        let steps = plan(Action::Ax, &root(), Host::Windows, true);
        assert_eq!(steps[0].program, "powershell");
        assert!(steps[0]
            .args
            .iter()
            .any(|arg| arg.ends_with("reinstall-cli.ps1")));
        assert_eq!(steps[1].program, "/repo/target-dev/release/ax.exe");
    }

    #[test]
    fn pull_fast_forwards_then_rebuilds_without_starting_web() {
        let steps = plan(Action::PullRebuild, &root(), Host::Unix, true);
        assert_eq!(steps[0].args, ["pull", "--ff-only", "origin", "main"]);
        assert_eq!(steps[1].program, "bash");
        assert!(steps
            .iter()
            .all(|step| step.args != ["web", "--port", "7070"]));
    }

    #[test]
    fn deploy_is_the_production_script_and_asks_first() {
        let steps = plan(Action::Deploy, &root(), Host::Unix, true);
        assert_eq!(steps.len(), 1);
        assert_eq!(steps[0].program, "pwsh");
        assert!(steps[0]
            .args
            .iter()
            .any(|arg| arg.ends_with("deploy-netlify.ps1")));
        assert!(steps[0].args.iter().all(|arg| arg != "-Preview"));
        assert!(Action::Deploy.needs_confirm());
    }

    #[test]
    fn publish_waits_for_the_release_then_deploys_without_a_local_bump() {
        let steps = plan(Action::Publish, &root(), Host::Unix, true);
        assert_eq!(steps.len(), 2);
        assert!(steps[0]
            .args
            .iter()
            .any(|arg| arg.ends_with("release-tag.ps1")));
        assert!(steps[0].args.iter().any(|arg| arg == "-Wait"));
        assert!(steps[0]
            .args
            .iter()
            .all(|arg| arg != "-Bump" && arg != "-Force" && arg != "patch"));
        assert!(steps[1]
            .args
            .iter()
            .any(|arg| arg.ends_with("deploy-netlify.ps1")));
        assert!(Action::Publish.needs_confirm());
        assert!(!Action::Website.needs_confirm());
    }
}
