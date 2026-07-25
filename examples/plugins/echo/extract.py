#!/usr/bin/env python3
"""Minimal process plugin: one File node per input path."""
import json
import sys
import time

payload = json.load(sys.stdin)
path = payload.get("path", "unknown")
name = path.replace("\\", "/").split("/")[-1]
now = int(time.time() * 1000)
result = {
    "nodes": [
        {
            "id": f"file:{path}",
            "kind": "file",
            "name": name,
            "qualifiedName": path,
            "filePath": path,
            "language": "unknown",
            "startLine": 1,
            "endLine": 1,
            "startColumn": 0,
            "endColumn": 0,
            "updatedAt": now,
        }
    ],
    "edges": [],
    "unresolvedReferences": [],
    "errors": [],
    "durationMs": 0,
}
json.dump(result, sys.stdout)
