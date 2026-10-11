# Install-time project discovery

## Behavior

When ax is installed or upgraded, it scans the home directory and runs `ax init` on every project it finds. The person installing ax does not run init per project.

A directory is a project when it has `.ax/ax.db`, a `.git` entry, or a workspace manifest (`Cargo.toml`, `package.json`, `go.mod`, and the other manifests `looks_like_project_root` already lists). The walk:

- starts at the home directory and goes four levels down
- does not return the home directory or `/`
- does not enter hidden directories, `node_modules`, `target`, `target-dev`, `dist`, `build`, `vendor`, or the other skip names
- does not follow symlinks
- stops at the first project, so a nested package inside that project is not initialized separately

`ax init --all` is the same scan. It does not take a path and does not combine with `--workspace`. It does not prompt: saved stacks and IDEs stay, a new project uses the detected defaults. `AX_SKIP_PROJECT_INIT=1` skips the scan. One project's failure is printed and the rest still run.

Install (`install.sh`, `install.ps1`) and upgrade (Unix after the binary is replaced, Windows after the bundle is activated) run `ax init --all`.

## Out of scope

This does not change `ax init` on one directory. It does not index the home directory itself.
