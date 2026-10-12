"""Real CLI import and Command Center HTTP provenance, using owned fixtures only."""
from pathlib import Path
import datetime
from contextlib import closing
import json
import re
import hashlib
import os
import socket
import sqlite3
import subprocess
import tempfile
import time
import urllib.error
import urllib.request

def main() -> None:
    if not __debug__:
        raise RuntimeError("Run verification without Python optimization")
    binary = str(Path(os.environ["AX_VERIFY_BINARY"]).resolve())
    with tempfile.TemporaryDirectory(prefix="ax-provenance-") as fixture:
        base = Path(fixture)
        root = base / "project"
        root.mkdir()
        env = {**os.environ, "AX_NO_UPDATE_CHECK": "1", "AX_GLOBAL_DB": str(base / "global.db"), "AX_USAGE_DB": str(base / "usage.db")}
        with closing(sqlite3.connect(env["AX_USAGE_DB"])) as db:
            db.execute("CREATE TABLE pricing_sync_meta(source TEXT PRIMARY KEY,last_attempt_at INTEGER,last_success_at INTEGER,last_success_date TEXT,status TEXT,error TEXT,models_count INTEGER)")
            db.execute("INSERT INTO pricing_sync_meta VALUES ('openrouter',0,0,?,'ok',NULL,1)", (datetime.date.today().isoformat(),))
            db.commit()
        def cli(*args):
            return subprocess.run([binary, *map(str, args)], env=env, capture_output=True, text=True, timeout=180, check=True)
        cli("init", root)
        config_path = root / "ax.json"
        config = json.loads(config_path.read_text(encoding="utf-8")) if config_path.exists() else {}
        config.setdefault("policy", {})["requireReview"] = True
        config_path.write_text(json.dumps(config), encoding="utf-8")
        pack = base / "team-pack"
        (pack / "rules").mkdir(parents=True)
        (pack / "rules/provenance-smoke.mdc").write_text("---\nid: provenance-smoke\nlevel: INFO\nalwaysApply: true\nstatus: approved\n---\nImported policy content.\n", encoding="utf-8")
        cli("policy", "pack", "import", root, "--pack", pack)
        pending = root / ".ax/policy/pending/rules/provenance-smoke.mdc"
        assert pending.with_suffix(".origin.json").is_file()
        with socket.socket() as sock:
            sock.bind(("127.0.0.1", 0))
            port = sock.getsockname()[1]
        url = f"http://127.0.0.1:{port}"
        opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))
        def request(path, body=None):
            data = json.dumps(body).encode() if body is not None else None
            headers = {"Content-Type": "application/json", "Origin": url}
            try:
                with opener.open(urllib.request.Request(url + path, data, headers), timeout=10) as response:
                    return response.status, json.loads(response.read())
            except urllib.error.HTTPError as error:
                return error.code, json.loads(error.read())
        def start():
            process = subprocess.Popen([binary, "web", str(root), "--port", str(port)], env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
            for _ in range(200):
                if process.poll() is not None:
                    raise RuntimeError("Fixture Command Center exited before readiness")
                try:
                    if request("/api/stats")[0] == 200:
                        return process
                except (OSError, TimeoutError):
                    pass  # Expected while the owned server is binding.
                time.sleep(.1)
            process.terminate()
            process.wait(timeout=10)
            raise RuntimeError("Fixture Command Center did not become ready")
        def item():
            status, body = request("/api/policy/review")
            assert status == 200
            return next(entry for entry in body["items"] if entry["id"] == "provenance-smoke")
        process = start()
        try:
            if os.environ.get("AX_VERIFY_NAV") == "1":
                dist = Path("crates/ax-web/web-ui/dist")
                with opener.open(url, timeout=10) as response:
                    embedded = response.read()
                assert embedded == (dist / "index.html").read_bytes(), "Embedded index matches the rebuilt UI"
                for asset in re.findall(rb'(?:src|href)="(/assets/index-[^"]+)"', embedded):
                    with opener.open(url + asset.decode(), timeout=10) as response:
                        assert hashlib.sha256(response.read()).digest() == hashlib.sha256((dist / asset.decode().lstrip("/")).read_bytes()).digest(), "Embedded asset matches the rebuilt UI"
            if os.environ.get("AX_VERIFY_NAV") == "1":
                subprocess.run(["npm", "exec", "--", "playwright", "test", "e2e/agent-retired.spec.ts", "e2e/control-spacing.spec.ts", "e2e/policy-review-density.spec.ts", "--project=desktop-chrome", "--project=mobile-chrome"], cwd="crates/ax-web/web-ui", env={**env, "AX_WEB_URL": url}, timeout=180, check=True)
            first = item()
            assert first["effect"] == "new"
            assert "Local policy pack team-pack" in first["origin"]
            process.terminate()
            process.wait(timeout=10)
            process = start()
            assert item()["origin"] == first["origin"], "Source survives a service restart"
            pending.write_text(pending.read_text(encoding="utf-8").replace("Imported policy", "Edited policy"), encoding="utf-8")
            edited = item()
            assert edited["origin"] == "Origin was not recorded for this content"
            assert request("/api/policy/review/provenance-smoke/approve?kind=rule", {"expectedIncoming": first["incomingContent"], "expectedCurrent": None})[0] == 409
            assert request("/api/policy/review/provenance-smoke/reject?kind=rule", {"expectedIncoming": edited["incomingContent"], "expectedCurrent": None})[0] == 200
            assert not pending.with_suffix(".origin.json").exists()
            assert not pending.exists()
            print("PASS real CLI pack import and HTTP review: source, restart, stale attribution, snapshot conflict, rejection and cleanup", flush=True)
        finally:
            process.terminate()
            process.wait(timeout=10)


if __name__ == "__main__":
    main()
