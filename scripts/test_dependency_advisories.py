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


    def test_candidate_mode_skips_known_vulnerable_release(self):
        queries = [{"package": {"ecosystem": "npm", "name": "sharp"}, "version": "0.35.4"}]
        result = [{"vulns": [{"id": "GHSA-wq5f-xc86-pv6w"}]}]
        with patch.object(advisory, "proposed_additions", return_value=queries), \
             patch.object(advisory, "fetch_batch", return_value=result), \
             patch.object(advisory.sys, "argv", ["check", "--base", "HEAD", "--candidate"]):
            self.assertEqual(advisory.main(), 2)

    def test_strict_pr_mode_still_fails_known_advisory(self):
        queries = [{"package": {"ecosystem": "npm", "name": "sharp"}, "version": "0.35.4"}]
        result = [{"vulns": [{"id": "GHSA-wq5f-xc86-pv6w"}]}]
        with patch.object(advisory, "proposed_additions", return_value=queries), \
             patch.object(advisory, "fetch_batch", return_value=result), \
             patch.object(advisory.sys, "argv", ["check", "--base", "HEAD"]):
            self.assertEqual(advisory.main(), 1)

    def test_candidate_mode_fails_closed_on_incomplete_result(self):
        queries = [{"package": {"ecosystem": "npm", "name": "sharp"}, "version": "0.35.4"}]
        with patch.object(advisory, "proposed_additions", return_value=queries), \
             patch.object(advisory, "fetch_batch", return_value=[{"next_page_token": "more"}]), \
             patch.object(advisory.sys, "argv", ["check", "--candidate"]):
            self.assertEqual(advisory.main(), 1)

    def test_candidate_mode_fails_closed_on_osv_outage(self):
        with patch.object(advisory, "proposed_additions", return_value=[
            {"package": {"ecosystem": "npm", "name": "sharp"}, "version": "0.35.4"}
        ]), patch.object(advisory, "fetch_batch", side_effect=advisory.PolicyError("OSV offline")), \
             patch.object(advisory.sys, "argv", ["check", "--candidate"]):
            self.assertEqual(advisory.main(), 1)

    def test_clean_candidate_is_eligible(self):
        queries = [{"package": {"ecosystem": "npm", "name": "safe"}, "version": "1.0.0"}]
        with patch.object(advisory, "proposed_additions", return_value=queries), \
             patch.object(advisory, "fetch_batch", return_value=[{}]), \
             patch.object(advisory.sys, "argv", ["check", "--candidate"]):
            self.assertEqual(advisory.main(), 0)

    def test_malformed_advisory_is_error_not_skip(self):
        queries = [{"package": {"ecosystem": "npm", "name": "sharp"}, "version": "1.0"}]
        advisory_hits, errors = advisory.classify_findings(queries, [{"vulns": [{"id": None}]}])
        self.assertFalse(advisory_hits)
        self.assertEqual(len(errors), 1)

if __name__ == "__main__":
    unittest.main()
