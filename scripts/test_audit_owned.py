"""Ensure the workspace boundary cannot silently skip inactive/external packages."""
import copy
import importlib.util
from pathlib import Path
import tempfile
import unittest

spec = importlib.util.spec_from_file_location('audit', Path(__file__).with_name('audit-owned.py'))
audit = importlib.util.module_from_spec(spec)
spec.loader.exec_module(audit)

class AuditTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.metadata = {'workspace_members': ['engine'], 'packages': [
            {'id': 'engine', 'name': 'engine', 'source': None, 'manifest_path': str(self.root / 'Cargo.toml')}
        ]}

    def test_accepts_workspace_only(self):
        self.assertEqual(audit.validate_metadata(self.metadata, self.root), 1)

    def test_rejects_registry_and_git_dependencies_even_without_resolve_edges(self):
        for source in ['registry+https://example.invalid/index', 'git+https://example.invalid/library']:
            metadata = copy.deepcopy(self.metadata)
            metadata['packages'].append({'id': 'optional', 'name': 'optional', 'source': source, 'manifest_path': str(self.root / 'vendor/Cargo.toml')})
            with self.subTest(source=source), self.assertRaisesRegex(ValueError, 'non-workspace'):
                audit.validate_metadata(metadata, self.root)

    def test_rejects_non_workspace_path_package(self):
        self.metadata['packages'].append({'id': 'vendor', 'name': 'vendor', 'source': None, 'manifest_path': str(self.root / 'vendor/Cargo.toml')})
        with self.assertRaisesRegex(ValueError, 'non-workspace'):
            audit.validate_metadata(self.metadata, self.root)

    def test_rejects_workspace_member_outside_repository(self):
        self.metadata['packages'][0]['manifest_path'] = str(self.root.parent / 'outside/Cargo.toml')
        with self.assertRaisesRegex(ValueError, 'outside'):
            audit.validate_metadata(self.metadata, self.root)

    def test_rejects_empty_or_incomplete_metadata(self):
        with self.assertRaisesRegex(ValueError, 'empty'):
            audit.validate_metadata({'workspace_members': [], 'packages': []}, self.root)
        self.metadata['packages'].clear()
        with self.assertRaisesRegex(ValueError, 'incomplete'):
            audit.validate_metadata(self.metadata, self.root)

if __name__ == '__main__':
    unittest.main()
