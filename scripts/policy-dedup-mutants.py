#!/usr/bin/env python3
"""Manual mutation run for policy dedup (docs/specs/policy-dedup.md).

Each mutant is a plausible bug. The suite must report a failing test for every
one; a mutant that does not apply, does not compile, or survives fails the run.
Sources are restored byte-for-byte and verified by hash after each mutant.

Usage: python3 scripts/policy-dedup-mutants.py   (from the repo root)
"""

import hashlib
import os
import subprocess
import sys

LEVEL = "crates/ax-policy/src/global_level.rs"
INDEX = "crates/ax-policy/src/index.rs"
ENGINE = "crates/ax-core/src/policy_dedup.rs"
CORE = "crates/ax-core/src/lib.rs"
GPOLICY = "crates/ax-global-db/src/policy.rs"
GSYNC = "crates/ax-global-db/src/sync/mod.rs"
WEB = "crates/ax-web/src/workspace_state.rs"
SEED = "crates/ax-policy/src/seed.rs"

DEDUP_TESTS = ["cargo", "test", "-p", "ax-policy", "-p", "ax-core", "--lib", "--",
               "global_level", "policy_dedup", "import_"]
HOOK_TESTS = ["cargo", "test", "-p", "ax-core", "--test", "policy_dedup_hooks"]
GLOBAL_TESTS = ["cargo", "test", "-p", "ax-global-db", "--lib", "policy_level_tests"]
WEB_TESTS = ["cargo", "test", "-p", "ax-web", "--lib", "policy_dedup_tests"]
SEED_TESTS = ["cargo", "test", "-p", "ax-policy", "--lib", "seed::"]

MUTANTS = [
    ("shorter copy wins", LEVEL,
     "l > g || (l == g && lower_ms > global_ms)", "l < g || (l == g && lower_ms > global_ms)", DEDUP_TESTS),
    ("tie goes to the older copy", LEVEL,
     "l > g || (l == g && lower_ms > global_ms)", "l > g || (l == g && lower_ms < global_ms)", DEDUP_TESTS),
    ("identical bodies compared by time", LEVEL,
     "if same_text(lower, global) {", "if lower == \"\\u{0}\" {", DEDUP_TESTS),
    ("line endings count as content", LEVEL,
     "body.replace(\"\\r\\n\", \"\\n\").trim().to_string()", "body.trim().to_string()", DEDUP_TESTS),
    ("mirror rows counted as global", LEVEL,
     "\"WHERE g.level = 'global'\"", "\"\"", DEDUP_TESTS),
    ("import skip removed for skills", INDEX,
     "if shadowed(Kind::Skill,", "if false && shadowed(Kind::Skill,", DEDUP_TESTS),
    ("import skip removed for rules", INDEX,
     "if shadowed(Kind::Rule,", "if false && shadowed(Kind::Rule,", DEDUP_TESTS),
    ("import skips everything without global.db", INDEX,
     "global.is_some_and(|g| g.shadows(", "global.is_none_or(|g| g.shadows(", DEDUP_TESTS),
    ("missing global.db gets created", ENGINE,
     "if !global_path.is_file() {", "if false {", DEDUP_TESTS),
    ("dry run on an old schema runs anyway", ENGINE,
     "    let current = if dry_run {", "    let current = if false {", DEDUP_TESTS),
    ("stop reason reported as a skip", ENGINE,
     "        report.error = Some(format!(\"global.db not opened: {e}\"));",
     "        report.skipped = Some(format!(\"global.db not opened: {e}\"));", DEDUP_TESTS),
    ("error line dropped from the report", ENGINE,
     "            lines.push(format!(\"error: {error}\"));", "            let _ = error;", DEDUP_TESTS),
    ("init seeds global-level skills into the project", SEED,
     "GLOBAL_SKILL_BUNDLES.iter().filter(|b| !b.global_db)", "GLOBAL_SKILL_BUNDLES.iter()", SEED_TESTS),
    ("longer copy removed without promotion", ENGINE,
     "let version = promote(global, lead, kind, pool, &row.name, dry_run).await?;", "let version = 2;", DEDUP_TESTS),
    ("project row removed without a revision", ENGINE,
     "    ax_policy::revisions::record_if_changed(\n        pool,\n        kind.revision_kind(),\n"
     "        &row.name,\n        &row.body,\n        ax_policy::revisions::SOURCE_DEDUP,\n    )\n    .await?;\n",
     "", DEDUP_TESTS),
    ("dry run deletes project rows", ENGINE,
     "        if !dry_run {\n            remove_project_row(", "        if true {\n            remove_project_row(", DEDUP_TESTS),
    ("dry run deletes mirrors", ENGINE,
     "gpolicy::shadowed_mirrors(global, gkind(kind), !dry_run)", "gpolicy::shadowed_mirrors(global, gkind(kind), true)",
     DEDUP_TESTS),
    ("global collapse removes the winner", ENGINE,
     "        if i == lead {", "        if i != lead {", DEDUP_TESTS),
    ("no dedup after sync", CORE,
     "            let _ = ax_policy::index_policy(self.db.pool(), &self.project_root, false).await;\n"
     "            self.dedup_policy(false).await;\n        }\n        result\n    }\n\n    fn merge_index_opts(",
     "            let _ = ax_policy::index_policy(self.db.pool(), &self.project_root, false).await;\n"
     "        }\n        result\n    }\n\n    fn merge_index_opts(", HOOK_TESTS),
    ("no dedup after policy index", CORE,
     "        let result = ax_policy::index_policy(self.db.pool(), &self.project_root, force).await?;\n"
     "        self.dedup_policy(false).await;\n",
     "        let result = ax_policy::index_policy(self.db.pool(), &self.project_root, force).await?;\n", HOOK_TESTS),
    ("version not incremented", GPOLICY,
     "    .bind(next)\n    .bind(payload)", "    .bind(1_i64)\n    .bind(payload)", GLOBAL_TESTS),
    ("cap keeps one version too many", GPOLICY,
     "AND version <= ?\")", "AND version < ?\")", GLOBAL_TESTS),
    ("unchanged write records a revision", GPOLICY,
     "== Some(text.as_str())", "== Some(\"\")", GLOBAL_TESTS),
    ("old body of a row without history is lost", GPOLICY,
     "                record_revision(&mut tx, kind, item_id, project_id, old, \"baseline\").await?;\n", "",
     GLOBAL_TESTS),
    ("dry run predicts the wrong version for a row without history", GPOLICY,
     "Ok(if stored.is_some() { 2 } else { 1 })", "Ok(1)", GLOBAL_TESTS),
    ("delete records no revision", GPOLICY,
     "    record_revision(&mut tx, kind, item_id, project_id, &payload, source).await?;\n", "", GLOBAL_TESTS),
    ("agents load mirror skills", GPOLICY,
     "WHERE level = 'global' ORDER BY item_id, project_id", "ORDER BY item_id, project_id", GLOBAL_TESTS),
    ("global sync mirrors global names", GSYNC,
     "        if at_global_level.is_some() {\n            continue;\n        }\n", "", GLOBAL_TESTS),
    ("read-only web still dedups", WEB,
     "    if readonly {\n        return None;\n    }\n    let global_path = global_path?;",
     "    let global_path = global_path?;", WEB_TESTS),
    ("web never dedups", WEB,
     "ax_core::policy_dedup::run(&global_path, Some((&pool, &root)), false)",
     "ax_core::policy_dedup::run(&global_path, None, false)", WEB_TESTS),
]


def sha(path):
    with open(path, "rb") as f:
        return hashlib.sha256(f.read()).hexdigest()


def run(cmd):
    env = dict(os.environ)
    env.pop("CARGO_TARGET_DIR", None)
    return subprocess.run(cmd, capture_output=True, text=True, env=env)


def main():
    paths = sorted({m[1] for m in MUTANTS})
    originals = {p: open(p, "rb").read() for p in paths}
    hashes = {p: sha(p) for p in paths}
    for cmd in (DEDUP_TESTS, HOOK_TESTS, GLOBAL_TESTS, WEB_TESTS, SEED_TESTS):
        base = run(cmd)
        if base.returncode != 0:
            print(f"BASELINE RED: {' '.join(cmd)}\n{base.stdout[-2000:]}{base.stderr[-2000:]}")
            return 1
    killed, problems = 0, []
    for name, path, old, new, cmd in MUTANTS:
        src = originals[path].decode()
        if src.count(old) != 1:
            problems.append(f"NOT APPLIED ({src.count(old)} matches): {name}")
            continue
        try:
            with open(path, "w") as f:
                f.write(src.replace(old, new))
            out = run(cmd)
        finally:
            with open(path, "wb") as f:
                f.write(originals[path])
        if sha(path) != hashes[path]:
            print(f"RESTORE FAILED for {path}")
            return 1
        text = out.stdout + out.stderr
        if "error[E" in text or "could not compile" in text:
            problems.append(f"DID NOT COMPILE: {name}")
        elif out.returncode != 0 and "test result: FAILED" in text:
            failed = [l.split()[1] for l in text.splitlines() if l.startswith("test ") and l.endswith("FAILED")]
            killed += 1
            print(f"killed   {name}  <- {', '.join(failed[:3])}")
        elif out.returncode == 0:
            problems.append(f"SURVIVED: {name}")
        else:
            problems.append(f"UNEXPECTED EXIT {out.returncode}: {name}")
    print(f"\n{killed}/{len(MUTANTS)} mutants killed; sources restored (sha256 verified)")
    for p in problems:
        print(p)
    return 0 if killed == len(MUTANTS) and not problems else 1


if __name__ == "__main__":
    sys.exit(main())
