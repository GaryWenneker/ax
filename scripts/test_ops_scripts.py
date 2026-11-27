#!/usr/bin/env python3
"""Ops scripts never publish an unreviewed working tree or overwrite live data.

A script that stages everything and then pushes, or creates a public repository,
publishes whatever untracked file happens to be in the tree (.env, keys, local
databases). A schema script run against the live ~/.ax/global.db must refuse
instead of writing into it.
"""

from __future__ import annotations

import os
import re
import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
STAGE_ALL = re.compile(r"git\s+add\s+(-A|--all|\.)(\s|$)")
PUBLISH = re.compile(r"git\s+push|gh\s+repo\s+create")


def ops_scripts() -> list[Path]:
    found = []
    for folder in ("scripts", "tools"):
        for path in sorted((ROOT / folder).rglob("*")):
            if path.suffix in {".sh", ".ps1", ".py", ".mjs", ".js"} and path.is_file() and not path.name.startswith("test_"):
                found.append(path)
    return found


class PublishingScripts(unittest.TestCase):
    def test_no_script_stages_everything_and_publishes_it(self) -> None:
        offenders = []
        for path in ops_scripts():
            text = path.read_text(encoding="utf-8", errors="replace")
            if PUBLISH.search(text) and STAGE_ALL.search(text):
                offenders.append(path.relative_to(ROOT).as_posix())
        self.assertEqual(offenders, [], "these scripts stage the whole tree and push or create a repo")

    def test_no_script_creates_a_public_repository(self) -> None:
        offenders = [
            path.relative_to(ROOT).as_posix()
            for path in ops_scripts()
            if re.search(r"gh\s+repo\s+create[^\n]*--public", path.read_text(encoding="utf-8", errors="replace"))
        ]
        self.assertEqual(offenders, [])


@unittest.skipUnless(shutil.which("sqlite3") and shutil.which("bash"), "needs sqlite3 and bash")
class CreateGlobalDb(unittest.TestCase):
    SCRIPT = ROOT / "scripts" / "create-global-db.sh"

    def run_script(self, home: Path) -> subprocess.CompletedProcess[str]:
        env = {**os.environ, "HOME": str(home)}
        return subprocess.run(["bash", str(self.SCRIPT)], env=env, capture_output=True, text=True)

    def test_refuses_an_existing_global_db(self) -> None:
        home = Path(tempfile.mkdtemp())
        (home / ".ax").mkdir()
        db = home / ".ax" / "global.db"
        subprocess.run(["sqlite3", str(db), "CREATE TABLE keep(x); INSERT INTO keep VALUES (1);"], check=True)
        before = db.read_bytes()
        done = self.run_script(home)
        self.assertNotEqual(done.returncode, 0, done.stdout + done.stderr)
        self.assertIn("already exists", done.stdout + done.stderr)
        self.assertEqual(db.read_bytes(), before)

    def test_creates_a_new_global_db(self) -> None:
        home = Path(tempfile.mkdtemp())
        done = self.run_script(home)
        self.assertEqual(done.returncode, 0, done.stdout + done.stderr)
        self.assertTrue((home / ".ax" / "global.db").is_file())


if __name__ == "__main__":
    unittest.main()
