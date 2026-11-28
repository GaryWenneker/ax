#!/usr/bin/env python3
"""Manual mutation run for the review loop (docs/specs/review-loop.md).

Each mutant is a plausible bug. The suite must report a failing test for every
one; a mutant that does not apply, does not compile, or survives fails the run.
Sources are restored byte-for-byte and verified by hash after each mutant.

Usage: python3 scripts/review-loop-mutants.py   (from the repo root)
"""

import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

from mutants_lib import run_mutants  # noqa: E402

SEED = "crates/ax-policy/src/seed.rs"
INIT = "crates/ax-cli/src/commands/init.rs"
RULE = "crates/ax-policy/templates/rules/old-coder-mandatory.mdc"
SKILL = "crates/ax-policy/templates/skills/review-loop/SKILL.md"
PRC = "crates/ax-policy/templates/skills/pr-review-comments/SKILL.md"
POLICY_TESTS = ["cargo", "test", "-p", "ax-policy", "--lib", "seed::"]
INIT_TESTS = ["cargo", "test", "-p", "ax-cli", "--bin", "ax", "commands::init"]

MUTANTS = [
    ("newer seedVersion never upgrades", SEED,
     "if seed_version(template) > seed_version(existing) {",
     "if seed_version(template) > seed_version(existing) + 100 {", POLICY_TESTS),
    ("equal seedVersion still upgrades (clobbers hand edits)", SEED,
     "if seed_version(template) > seed_version(existing) {",
     "if seed_version(template) >= seed_version(existing) {", POLICY_TESTS),
    ("seedVersion read from the body too", SEED,
     ".and_then(|rest| rest.split(\"\\n---\").next())",
     ".map(|rest| rest)", POLICY_TESTS),
    ("review-loop not a global.db skill", SEED,
     "review-loop/SKILL.md\"),\n        }],\n        global_db: true,",
     "review-loop/SKILL.md\"),\n        }],\n        global_db: false,", POLICY_TESTS),
    ("pr-review-comments not a global.db skill", SEED,
     "pr-review-comments/SKILL.md\"),\n        }],\n        global_db: true,",
     "pr-review-comments/SKILL.md\"),\n        }],\n        global_db: false,", POLICY_TESTS),
    ("pr comments: batch approval allowed", PRC,
     "Never ask for a batch approval of several comments", "You may ask for a batch approval of several comments", POLICY_TESTS),
    ("pr comments: agent fixes the colleague's code", PRC,
     "Do not fix anything.", "Fix what you find.", POLICY_TESTS),
    ("pr comments: no free text option", PRC,
     "- free text: the user writes their own comment", "- the user can only pick a proposed comment", POLICY_TESTS),
    ("pr comments: posts without a choice", PRC,
     "Never post a comment the user did not choose", "Post every comment", POLICY_TESTS),
    ("review-loop keeps fixing colleague PRs", SKILL,
     "For a colleague's pull request, load `pr-review-comments` instead", "For a colleague's pull request, run the loop too", POLICY_TESTS),
    ("review-loop seedVersion not bumped", SKILL, "seedVersion: 3", "seedVersion: 2", POLICY_TESTS),
    ("old-coder leaks into global.db", SEED,
     "        global_db: false,\n    },\n    SkillBundle {\n        name: \"old-coder-api\",",
     "        global_db: true,\n    },\n    SkillBundle {\n        name: \"old-coder-api\",", POLICY_TESTS),
    ("rule drops the review loop step", RULE,
     "REVIEW LOOP", "REVIEW", POLICY_TESTS),
    ("rule seedVersion not bumped", RULE,
     "seedVersion: 3", "seedVersion: 2", POLICY_TESTS),
    ("rule forgets colleague PRs", RULE,
     "call **`ax_skill({ name: \"pr-review-comments\" })`** instead", "run the same loop", POLICY_TESTS),
    ("skill allows a round cap", SKILL,
     "There is no round cap", "Stop after two rounds", POLICY_TESTS),
    ("skill stops before zero findings", SKILL,
     "first round with **zero findings**", "first round with **one finding or fewer**", POLICY_TESTS),
    ("store writes the rules table", INIT,
     "ax_global_db::policy::PolicyKind::Skills,", "ax_global_db::policy::PolicyKind::Rules,", INIT_TESTS),
    ("store writes project scope", INIT,
     "\"scope\": \"company\",", "\"scope\": \"project\",", INIT_TESTS),
    ("store swallows open failure", INIT,
     "    let pool = ax_global_db::open_and_init(db_path)\n        .await\n        .map_err(|e| format!(\"{e:#}\"))?;",
     "    let Ok(pool) = ax_global_db::open_and_init(db_path).await else { return Ok(Vec::new()) };", INIT_TESTS),
]


if __name__ == "__main__":
    sys.exit(run_mutants(MUTANTS, (POLICY_TESTS, INIT_TESTS)))
