"""Offline tests for new-dependency OSV advisory review."""

import unittest
from unittest.mock import patch

import check_dependency_advisories as advisory


class AdvisoryTests(unittest.TestCase):
    def test_no_advisories(self):
        queries = [{"package": {"ecosystem": "npm", "name": "safe"}, "version": "1.0"}]
        self.assertEqual(advisory.findings(queries, [{}]), [])

    def test_vulnerability_blocks(self):
        queries = [{"package": {"ecosystem": "npm", "name": "unsafe"}, "version": "1.0"}]
        result = advisory.findings(queries, [{"vulns": [{"id": "GHSA-test"}]}])
        self.assertEqual(len(result), 1)
        self.assertIn("GHSA-test", result[0])

    def test_pagination_fails_closed(self):
        queries = [{"package": {"ecosystem": "npm", "name": "safe"}, "version": "1.0"}]
        result = advisory.findings(queries, [{"next_page_token": "more"}])
        self.assertEqual(len(result), 1)

    def test_batch_length_must_match(self):
        class Response:
            def __enter__(self):
                return self
            def __exit__(self, *_):
                return False
            def read(self):
                return b'{"results":[]}'
        queries = [{"package": {"ecosystem": "npm", "name": "safe"}, "version": "1.0"}]
        with patch.object(advisory.urllib.request, "urlopen", return_value=Response()):
            with self.assertRaises(advisory.PolicyError):
                advisory.fetch_batch(queries)


if __name__ == "__main__":
    unittest.main()
