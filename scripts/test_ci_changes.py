"""Regression coverage for the CI boundaries: never skip backend source changes."""
import importlib.util
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location('ci_changes', Path(__file__).with_name('ci-changes.py'))
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


class ChangeClassification(unittest.TestCase):
    def test_frontend_source_and_embedded_assets_only(self):
        self.assertEqual(module.classify(['crates/ax-web/web-ui/src/App.tsx', 'crates/ax-web/web-ui/dist/assets/index.js']), {'backend': False, 'frontend': True})

    def test_backend_web_routes_are_backend(self):
        self.assertEqual(module.classify(['crates/ax-web/src/api.rs']), {'backend': True, 'frontend': True})

    def test_rust_and_dependency_inputs(self):
        for path in ['database/schema/global.db.schema.sql', 'Cargo.lock', 'crates/ax-core/Cargo.toml', '.cargo/config.toml', 'tests/integration.rs', 'rust-toolchain.toml']:
            with self.subTest(path=path):
                self.assertTrue(module.classify([path])['backend'])

    def test_mixed_change_keeps_backend_checks(self):
        self.assertTrue(module.classify(['crates/ax-web/web-ui/src/App.tsx', 'crates/ax-server/src/lib.rs'])['backend'])

    def test_docs_and_screenshots_do_not_build_runtime(self):
        self.assertEqual(module.classify(['site/public/screenshots/cc-settings.png', 'site/src/content/docs/guides/command-center.md']), {'backend': False, 'frontend': False})

    def test_ci_configuration_validates_frontend(self):
        self.assertEqual(module.classify(['.github/workflows/ci.yml', 'scripts/ci-changes.py']), {'backend': False, 'frontend': True})


if __name__ == '__main__':
    unittest.main()
