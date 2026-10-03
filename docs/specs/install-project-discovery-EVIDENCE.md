# Evidence: install-time project discovery

Spec: `docs/specs/install-project-discovery.md`
Spec approval: not obtained as a separate review. The request to bump, build, and publish included this behavior.

| Scenario | Test |
|---|---|
| Git repo is found; a plain folder is not | `finds_a_git_repo_and_skips_a_plain_folder` |
| Manifest without git is found | `finds_a_manifest_without_git` |
| Nested package inside a project is not a second project | `does_not_descend_into_a_project` |
| `node_modules` is skipped and depth 2 does not reach depth 3 | `skips_node_modules_and_stops_at_max_depth` |
| Existing `.ax/ax.db` is found; a symlinked directory is not | `finds_an_initialized_directory_and_ignores_a_symlinked_directory` |
| Home itself is not initialized; a child project is | `skips_the_home_directory_itself` |

Command (after the last edit to the discovery tests):

```
CARGO_TARGET_DIR=/Users/gary/io/ax/target-dev cargo test -p ax-context --lib discover_tests
```

Result: 6 passed, 0 failed, finished in 0.00s.

`cargo check -p ax-cli` finished with exit 0. The pre-existing unused `std::io::Write` import in `upgrade.rs` still warns. Mutation testing was not run.
