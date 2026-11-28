"""tools/ax-admin-e2e.sh runs on minimal images that lack uuidgen and shasum.

The script is driven with fake ax and ax-admin binaries in --serve mode, on a PATH
that holds only basic tools plus sha256sum, so a missing uuidgen or shasum fails it.
"""
import hashlib
import json
import os
import re
import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "tools" / "ax-admin-e2e.sh"
TOOLS = ["bash", "env", "dirname", "basename", "mktemp", "rm", "mkdir", "cat", "tr", "cut",
         "grep", "head", "seq", "sleep", "od", "sha256sum", "cp"]

FAKE_AX = """#!/bin/sh
case "$1" in
  init) mkdir -p .ax && : > .ax/ax.db ;;
  index) ;;
  mcp) cp "$5" "$CAPTURE"; echo "listening on http://127.0.0.1:1" ;;
esac
"""
FAKE_ADMIN = """#!/bin/sh
echo "ax-admin on http://127.0.0.1:2"
"""


class MinimalImageTest(unittest.TestCase):
    def test_runs_without_uuidgen_or_shasum(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp = Path(tmp)
            bin_dir = tmp / "bin"
            bin_dir.mkdir()
            for tool in TOOLS:
                found = shutil.which(tool)
                if found is None:
                    self.skipTest(f"{tool} is not installed")
                (bin_dir / tool).symlink_to(found)
            for name, body in (("ax", FAKE_AX), ("ax-admin", FAKE_ADMIN)):
                path = tmp / name
                path.write_text(body)
                path.chmod(0o755)
            capture = tmp / "config.json"
            env = {"PATH": str(bin_dir), "CAPTURE": str(capture), "TMPDIR": str(tmp)}
            run = subprocess.run(
                [str(bin_dir / "bash"), str(SCRIPT), str(tmp / "ax"), str(tmp / "ax-admin"), "--serve"],
                env=env, capture_output=True, text=True, timeout=60,
            )
            self.assertEqual(run.returncode, 0, run.stderr)
            self.assertNotIn("command not found", run.stderr)
            keys = dict(re.findall(r"^(alpha \(context\)|alpha \(admin\)|beta \(context\)):.*key: (\S+)$",
                                   run.stdout, re.M))
            self.assertEqual(len(keys), 3, run.stdout)
            for key in keys.values():
                self.assertRegex(key, r"^e2e-[a-z]+-[0-9a-f]{32}$")
            config = json.loads(capture.read_text())
            hashes = {k["id"]: k["sha256"] for k in config["api_keys"]}
            sha = lambda s: hashlib.sha256(s.encode()).hexdigest()
            self.assertEqual(hashes["context"], sha(keys["alpha (context)"]))
            self.assertEqual(hashes["admin"], sha(keys["alpha (admin)"]))
            self.assertEqual(hashes["beta"], sha(keys["beta (context)"]))
            self.assertEqual(len(set(keys.values())), 3)


if __name__ == "__main__":
    unittest.main()
