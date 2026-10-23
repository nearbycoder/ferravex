"""Catalog/source regressions using isolated fixtures, without Cargo or a display."""
import contextlib
import copy
import importlib.util
import io
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location('catalog_audit', Path(__file__).with_name('audit-catalog.py'))
audit = importlib.util.module_from_spec(spec)
spec.loader.exec_module(audit)

ACTIVE = '''
use ferravex_core::{session::{GameSession as Loop, SessionState}};
struct Host { clock: Loop }
impl Host {
    fn new() -> Self { Self { clock: Loop :: new (60.0, 4, true) } }
    fn update(&mut self, elapsed: f64) {
        let step = self.clock.advance(elapsed);
        self.clock.set_paused(false);
    }
}
'''
CAPABILITIES = {
    'schema_version': 1,
    'pause': {'simulation': {'evidence': [
        {'path': 'examples/alpha/src/lib.rs', 'snippet': 'self.clock.set_paused(false)'},
    ]}},
}


class MigrationTests(unittest.TestCase):
    def report(self, code):
        return audit.session_usage({'src/main.rs': code})

    def test_grouped_alias_and_typed_field_are_active(self):
        report = self.report(ACTIVE)
        self.assertEqual(report['status'], 'active')
        self.assertEqual(report['timing'], 'session')
        self.assertEqual(report['methods'], ['advance', 'set_paused'])
        self.assertIn('clock', report['evidence'][0]['receivers'])

    def test_qualified_module_alias_and_glob_imports(self):
        for prefix, constructor in [
            ('', 'ferravex_core::session::GameSession'),
            ('use ferravex_core::session as lifecycle;', 'lifecycle::GameSession'),
            ('use ferravex_core::session::{self as lifecycle};', 'lifecycle::GameSession'),
            ('use ferravex_core::session::*;', 'GameSession'),
            ('use ferravex::prelude::*;', 'GameSession'),
        ]:
            with self.subTest(constructor=constructor):
                report = self.report(prefix + f'fn run() {{ let mut unusual_name = {constructor}::new(60., 4, true); unusual_name.advance(0.1); }}')
                self.assertEqual(report['status'], 'active')

    def test_lifecycle_without_clock_ownership_is_valid_evidence(self):
        report = self.report('use ferravex_core::session::GameSession; fn run() { let mut host = GameSession::new(64., 1, true); host.toggle_pause(); }')
        self.assertEqual(report['status'], 'active')
        self.assertEqual(report['timing'], 'external_or_unobserved')

    def test_checked_and_configured_constructors_bind_inferred_receivers(self):
        for prefix, session in (
            ('', 'ferravex_core::session::GameSession'),
            ('', 'ferravex::core::session::GameSession'),
            ('use ferravex_core::session::{GameSession as Loop};', 'Loop'),
            ('use ferravex_core::session as lifecycle;', 'lifecycle::GameSession'),
            ('use ferravex_core::session::*;', 'GameSession'),
            ('use ferravex::prelude::*;', 'GameSession'),
        ):
            for constructor in (
                'try_new(60., 4, true).expect("valid clock")',
                'try_new(60., 4, true).unwrap()',
                'try_new(60., 4, true)?',
                'from_config(config)',
                'from_config(ferravex_core::session::SessionConfig::try_new(60., 4, true)?)',
            ):
                with self.subTest(session=session, constructor=constructor):
                    report = self.report(prefix + f'fn run() {{ let mut clock = {session}::{constructor}; clock.advance(dt); }}')
                    self.assertEqual(report['status'], 'active')
                    self.assertEqual(report['timing'], 'session')
                    self.assertEqual(report['constructors'], 1)
                    self.assertEqual(report['methods'], ['advance'])
                    self.assertEqual(report['evidence'][0]['receivers'], ['clock'])

    def test_checked_and_configured_constructors_bind_typed_fields(self):
        for constructor in ('Loop::try_new(60., 4, true).expect("valid clock")',
                            'Loop::from_config(config)'):
            with self.subTest(constructor=constructor):
                source = ACTIVE.replace('Loop :: new (60.0, 4, true)', constructor)
                report = self.report(source)
                self.assertEqual(report['status'], 'active')
                self.assertEqual(report['constructors'], 1)
                self.assertEqual(report['methods'], ['advance', 'set_paused'])

    def test_config_or_unrelated_types_are_not_session_constructors(self):
        for prefix, session in (
            ('', 'ferravex_core::session::SessionConfig'),
            ('use ferravex_core::session::SessionConfig as GameSession;', 'GameSession'),
            ('use other_engine::GameSession;', 'GameSession'),
            ('', 'GameSession'),
        ):
            for constructor in ('try_new(60., 4, true).unwrap()', 'from_config(config)'):
                with self.subTest(session=session, prefix=prefix, constructor=constructor):
                    report = self.report(prefix + f'fn run() {{ let mut clock = {session}::{constructor}; clock.advance(dt); }}')
                    self.assertEqual(report['status'], 'absent')
                    self.assertEqual(report['constructors'], 0)
                    self.assertEqual(report['methods'], [])

    def test_checked_constructor_without_session_calls_is_only_constructed(self):
        for constructor in ('try_new(60., 4, true).unwrap()', 'from_config(config)'):
            with self.subTest(constructor=constructor):
                report = self.report('use ferravex_core::session::GameSession; '
                                     f'fn run() {{ let unused = GameSession::{constructor}; other.advance(dt); }}')
                self.assertEqual(report['status'], 'constructed')
                self.assertEqual(report['constructors'], 1)
                self.assertEqual(report['methods'], [])

    def test_constructor_names_must_be_exact_calls_not_function_references(self):
        for expression in ('GameSession::try_new', 'GameSession::from_config',
                           'GameSession::try_new_unchecked(60., 4, true)',
                           'GameSession::from_configuration(config)',
                           'config.try_new(60., 4, true)', 'config.from_config(config)'):
            with self.subTest(expression=expression):
                report = self.report('use ferravex_core::session::GameSession; '
                                     f'fn run() {{ let mut clock = {expression}; clock.advance(dt); }}')
                self.assertEqual(report['status'], 'referenced')
                self.assertEqual(report['constructors'], 0)
                self.assertEqual(report['methods'], [])

    def test_checked_forms_in_comments_literals_tests_and_macros_are_ignored(self):
        for constructor in ('Loop::try_new(60., 4, true).unwrap()', 'Loop::from_config(config)'):
            source = ACTIVE.replace('Loop :: new (60.0, 4, true)', constructor)
            for hidden in (
                '// ' + source.replace('\n', '\n// '),
                '/* nested /* comment */ ' + source + ' */',
                'const DOC: &str = r###"' + source + '"###;',
                'const DOC: &[u8] = br###"' + source + '"###;',
                'const DOC: &str = ' + json.dumps(source) + ';',
                '#[cfg(test)] mod tests {' + source + '}',
                '#[cfg(all(test, unix))] mod tests {' + source + '}',
                '#![cfg(test)]\n' + source,
                '#[test] fn exercise() {' + source + '}',
                'macro_rules! unused { () => {' + source + '} }',
            ):
                with self.subTest(constructor=constructor, hidden=hidden[:60]):
                    report = self.report(hidden)
                    self.assertEqual(report['status'], 'absent')
                    self.assertEqual(report['constructors'], 0)

    def test_test_only_checked_construction_cannot_activate_production_reference(self):
        production = ('use ferravex_core::session::GameSession; '
                      'fn update(clock: &mut GameSession) { clock.advance(dt); }')
        for constructor in ('try_new(60., 4, true).unwrap()', 'from_config(config)'):
            with self.subTest(constructor=constructor):
                report = self.report(production + '#[cfg(test)] fn exercise() {'
                                     f'let mut clock = GameSession::{constructor}; clock.advance(dt); }}')
                self.assertEqual(report['status'], 'referenced')
                self.assertEqual(report['constructors'], 0)
                self.assertEqual(report['methods'], ['advance'])

    def test_comments_literals_and_test_only_items_are_not_evidence(self):
        for code in [
            '// ' + ACTIVE.replace('\n', '\n// '),
            '/* nested /* GameSession */ ' + ACTIVE + ' */',
            'const DOC: &str = r###"' + ACTIVE + '"###;',
            'const DOC: &str = "use ferravex_core::session::GameSession; GameSession::new()";',
            '#[cfg(test)] mod tests { ' + ACTIVE + ' }',
            '#[test] fn test_session() { ' + ACTIVE + ' }',
            'mod tests { #[test] fn sample() { ' + ACTIVE + ' } }',
        ]:
            with self.subTest(code=code[:35]):
                self.assertEqual(self.report(code)['status'], 'absent')

    def test_import_or_unused_constructor_is_not_active(self):
        self.assertEqual(self.report('use ferravex_core::session::GameSession;')['status'], 'referenced')
        report = self.report('use ferravex_core::session::GameSession; let unused = GameSession::new(60., 4, true); other.advance(0.1);')
        self.assertEqual(report['status'], 'constructed')
        self.assertEqual(report['methods'], [])

    def test_unrelated_type_and_method_do_not_count(self):
        report = self.report('use other_engine::GameSession; let mut clock = GameSession::new(); clock.advance(0.1);')
        self.assertEqual(report['status'], 'absent')

    def test_target_conditional_code_and_lifetimes_survive_tokenization(self):
        report = self.report('#[cfg(not(test))] mod native { ' + ACTIVE + " fn label<'a>(s: &'a str) -> &'a str { s } }")
        self.assertEqual(report['status'], 'active')

    def test_compound_cfg_and_inner_test_attributes_do_not_fake_migration(self):
        for code in (
            '#[cfg(all(test, target_os = "linux"))] mod tests {' + ACTIVE + '}',
            '#[cfg(any(all(test, unix), all(test, windows)))] mod tests {' + ACTIVE + '}',
            '#[cfg(not(any(not(test), unix)))] mod tests {' + ACTIVE + '}',
            '#![cfg(test)]\n' + ACTIVE,
            'mod tests { #![cfg(test)] ' + ACTIVE + '}',
            '#[cfg(any())] mod disabled {' + ACTIVE + '}',
            'macro_rules! unused { () => {' + ACTIVE + '} }',
        ):
            with self.subTest(code=code[:70]):
                self.assertEqual(self.report(code)['status'], 'absent')
        for cfg in ('any(test, unix)', 'all(not(test), unix)', 'not(test)', 'all()'):
            with self.subTest(cfg=cfg):
                self.assertEqual(self.report(f'#[cfg({cfg})] mod game {{ {ACTIVE} }}')['status'], 'active')

    def test_inner_cfg_does_not_hide_other_production_modules(self):
        source = 'mod hidden { #![cfg(test)] struct Test; } mod live {' + ACTIVE + '}'
        self.assertEqual(self.report(source)['status'], 'active')


class CapabilityTests(unittest.TestCase):
    def report(self, capabilities=None, source=ACTIVE):
        return audit.capability_usage(CAPABILITIES if capabilities is None else capabilities,
                                      {'examples/alpha/src/lib.rs': source})

    def test_whitespace_comments_and_notes_preserve_static_evidence(self):
        capabilities = copy.deepcopy(CAPABILITIES)
        capabilities['pause']['simulation']['note'] = 'Offline practice only.'
        report = self.report(capabilities, ACTIVE.replace('self.clock.set_paused(false)',
                                                       'self /* receiver */ . clock . set_paused ( false )'))
        self.assertTrue(report['passed'], report)
        self.assertEqual(report['basis'], 'static-source')
        self.assertFalse(report['runtime_verified'])
        self.assertEqual(report['claims'][0]['status'], 'source_matched')
        self.assertEqual(report['claims'][0]['note'], 'Offline practice only.')
        self.assertNotIn('supported', report)

    def test_all_declared_anchors_must_match_and_none_are_inferred(self):
        capabilities = copy.deepcopy(CAPABILITIES)
        anchors = capabilities['pause']['simulation']['evidence']
        anchors.append({'path': 'examples/alpha/src/lib.rs', 'snippet': 'self.clock.restart()'})
        report = self.report(capabilities)
        self.assertFalse(report['passed'])
        self.assertEqual([item['matched'] for item in report['claims'][0]['evidence']], [True, False])
        self.assertEqual(report['claims'][0]['status'], 'unverified')
        self.assertEqual([item['feature'] for item in report['claims']], ['pause.simulation'])

    def test_schema_versions_unknown_fields_and_empty_groups_are_rejected(self):
        malformed = [None, [], True, {}, {'schema_version': 1},
                     {'schema_version': 1, 'pause': {}},
                     {'schema_version': 1, 'controls': ['keyboard']}]
        for version in (0, 2, True, '1', 1.5, None):
            malformed.append({**CAPABILITIES, 'schema_version': version})
        malformed.extend([
            {**CAPABILITIES, 'runtime_verified': True},
            {**CAPABILITIES, 'pasue': CAPABILITIES['pause']},
            {'schema_version': 1, 'pause': {'everything': CAPABILITIES['pause']['simulation']}},
            {'schema_version': 1, 'pause': {'simulation': True}},
        ])
        for capabilities in malformed:
            with self.subTest(capabilities=capabilities):
                report = audit.capability_usage(capabilities, {})
                self.assertFalse(report['passed'])
                self.assertTrue(all(error['code'] == 'capability_schema' for error in report['errors']))
                self.assertEqual(report['claims'], [])
                json.dumps(report)

    def test_claim_fields_types_and_bounds_are_checked(self):
        anchor = CAPABILITIES['pause']['simulation']['evidence'][0]
        claims = [None, [], {}, {'evidence': []}, {'evidence': {}}, {'evidence': [None]},
                  {'evidence': [anchor, anchor]}, {'evidence': [anchor] * 5},
                  {'evidence': [anchor], 'supported': True},
                  {'evidence': [anchor], 'note': ' '},
                  {'evidence': [anchor], 'note': 'x' * 401}]
        for field in ('path', 'snippet'):
            for value in (None, 1, [], {}, '', ' ', 'x' * 257 if field == 'snippet' else 'x' * 241):
                claims.append({'evidence': [{**anchor, field: value}]})
            claims.append({'evidence': [{key: value for key, value in anchor.items() if key != field}]})
        claims.append({'evidence': [{**anchor, 'line': 10}]})
        for claim in claims:
            with self.subTest(claim=claim):
                capabilities = {'schema_version': 1, 'pause': {'simulation': claim}}
                self.assertFalse(self.report(capabilities)['passed'])

    def test_source_comments_literals_tests_and_macro_definitions_do_not_count(self):
        for source in (
            '// ' + ACTIVE.replace('\n', '\n// '),
            '/* ' + ACTIVE + ' */',
            'const DOC: &str = r###"' + ACTIVE + '"###;',
            '#[cfg(test)] mod tests {' + ACTIVE + '}',
            '#[cfg(all(test, unix))] mod tests {' + ACTIVE + '}',
            '#![cfg(test)]' + ACTIVE,
            'macro_rules! unused { () => {' + ACTIVE + '} }',
            ACTIVE.replace('set_paused', 'set_paused_elsewhere'),
        ):
            with self.subTest(source=source[:50]):
                report = self.report(source=source)
                self.assertFalse(report['passed'])
                self.assertEqual(report['errors'][0]['code'], 'capability_evidence')

    def test_bare_identifiers_punctuation_and_literal_anchors_are_rejected(self):
        for snippet in ('clock', '// self.clock.set_paused(false)', '{};',
                        'include_str!("scene.fxscene")', 'clock = r#"data"#'):
            with self.subTest(snippet=snippet):
                capabilities = copy.deepcopy(CAPABILITIES)
                capabilities['pause']['simulation']['evidence'][0]['snippet'] = snippet
                self.assertFalse(self.report(capabilities, source=snippet)['passed'])

    def test_schema_documents_all_five_groups_without_mutable_shared_output(self):
        schema = audit.capabilities_schema()
        self.assertEqual(set(schema['properties']),
                         {'schema_version', 'controls', 'pause', 'replay', 'content', 'presentation'})
        self.assertEqual(schema['$schema'], 'https://json-schema.org/draft/2020-12/schema')
        self.assertFalse(schema['additionalProperties'])
        schema['properties']['pause']['properties'].clear()
        self.assertIn('simulation', audit.capabilities_schema()['properties']['pause']['properties'])
        # JSON Schema treats an integral JSON number as an integer, but not bool.
        self.assertTrue(self.report({**CAPABILITIES, 'schema_version': 1.0})['passed'])


class CatalogTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.games = []
        for game in ('alpha', 'beta'):
            self.write(f'examples/{game}/Cargo.toml', f'[package]\nname="{game}"\n[lib]\ncrate-type=["rlib", "cdylib"]\n')
            self.write(f'examples/{game}/README.md', '# Demo\n')
            self.write(f'examples/{game}/src/lib.rs', ACTIVE)
            self.write(f'examples/{game}/src/main.rs', 'fn main() {}')
            self.write(f'web/{game}/index.html', f'<canvas data-game="{game}"></canvas>')
            self.write(f'docs/screenshots/{game}.png', 'fixture image')
            self.games.append({'id': game, 'title': game.title(), 'dimension': '2D', 'kind': 'game',
                               'host': game, 'genre': 'Arcade', 'description': 'A demo',
                               'controls': 'Space: start', 'image': f'{game}.png',
                               'source': f'examples/{game}/README.md'})
        self.write('Cargo.toml', '[workspace]\nmembers=["examples/*"]\n')
        self.save()

    def write(self, path, text):
        path = self.root / path
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text, encoding='utf-8')

    def save(self):
        self.write('examples/catalog.json', json.dumps(self.games))

    def report(self, **kwargs):
        self.save()
        return audit.audit_catalog(self.root, **kwargs)

    def codes(self, report):
        return {error['code'] for error in report['errors'] + [error for row in report['results'] for error in row['errors']]}

    def test_valid_catalog_checks_every_game_and_optional_session(self):
        report = self.report(migration=True)
        self.assertTrue(report['passed'], report)
        self.assertEqual(report['summary']['games'], 2)
        self.assertEqual(report['summary']['migration'], {'active': 2})
        self.assertTrue(self.report(require_session=True)['passed'])
        self.assertNotIn('migration', self.report()['results'][0])
        self.assertNotIn('capabilities', self.report()['results'][0])

    def test_missing_catalog_game_cannot_hide_workspace_demo(self):
        self.games.pop()
        report = self.report()
        self.assertFalse(report['passed'])
        self.assertIn('missing_game', self.codes(report))

    def test_duplicate_id_host_and_image_are_reported(self):
        self.games.append(copy.deepcopy(self.games[0]))
        report = self.report()
        self.assertFalse(report['passed'])
        self.assertEqual(sum(error['code'] == 'duplicate' for error in report['results'][-1]['errors']), 3)

    def test_invalid_metadata_continues_to_other_games(self):
        self.games[0].update(title=' ', dimension=[], kind='engine', controls=None)
        report = self.report()
        self.assertFalse(report['passed'])
        self.assertTrue(report['results'][1]['passed'])
        self.assertIn('metadata', self.codes(report))

    def test_missing_files_host_mismatch_and_invalid_package(self):
        (self.root / 'docs/screenshots/alpha.png').unlink()
        self.write('web/beta/index.html', '<canvas data-game="alpha"></canvas>')
        self.write('examples/beta/Cargo.toml', '[package]\nname="wrong"\n[lib]\ncrate-type=["rlib"]\n')
        report = self.report()
        self.assertFalse(report['passed'])
        self.assertTrue({'path', 'package', 'wasm_target'}.issubset(self.codes(report)))

    def test_workspace_exclusion_is_reported(self):
        self.write('Cargo.toml', '[workspace]\nmembers=["examples/*"]\nexclude=["examples/beta"]\n')
        self.assertIn('workspace', self.codes(self.report()))

    def test_paths_cannot_escape_the_game_or_repository(self):
        for source in ('examples/beta/README.md', '../secret', '/etc/passwd', 'examples/alpha/../beta/README.md', r'examples\alpha\README.md'):
            with self.subTest(source=source):
                self.games[0]['source'] = source
                self.assertIn('path', self.codes(self.report()))

    def test_symlink_outside_game_is_rejected(self):
        source = self.root / 'examples/alpha/README.md'
        source.unlink()
        source.symlink_to(self.root / 'examples/beta/README.md')
        self.assertIn('path', self.codes(self.report()))

    def test_optional_scene_and_custom_cargo_paths_are_validated(self):
        self.games[0]['editable_scene'] = 'examples/alpha/content/scene.fxscene'
        self.assertFalse(self.report()['passed'])
        self.write(self.games[0]['editable_scene'], 'scene')
        self.write('examples/alpha/Cargo.toml', '[package]\nname="alpha"\nautobins=false\n[lib]\npath="custom.rs"\ncrate-type=["cdylib"]\n[[bin]]\nname="alpha"\npath="driver.rs"\n')
        self.write('examples/alpha/custom.rs', ACTIVE)
        self.write('examples/alpha/driver.rs', 'fn main() {}')
        self.write('examples/alpha/src/lib.rs', '// unused original library')
        self.assertTrue(self.report(require_session=True)['passed'])
        (self.root / 'examples/alpha/driver.rs').unlink()
        self.assertFalse(self.report()['passed'])

    def test_malformed_input_and_manifest_produce_reports(self):
        for text in ('{', '{}', '[]', '[null]'):
            with self.subTest(text=text):
                self.write('examples/catalog.json', text)
                report = audit.audit_catalog(self.root)
                self.assertFalse(report['passed'])
                json.dumps(report)
        self.save()
        for text in ('[package', 'package=[]', '[package]\nname="alpha"\nlib=[]'):
            with self.subTest(text=text):
                self.write('examples/alpha/Cargo.toml', text)
                self.assertFalse(audit.audit_catalog(self.root)['passed'])

    def test_report_only_migration_does_not_fail_unmigrated_demo(self):
        self.write('examples/alpha/src/lib.rs', '// GameSession migration pending')
        report = self.report(migration=True)
        self.assertTrue(report['passed'])
        self.assertEqual(report['results'][0]['migration']['status'], 'absent')
        self.assertFalse(self.report(require_session=True)['passed'])

    def test_unregistered_or_test_only_modules_cannot_fake_migration(self):
        self.write('examples/alpha/src/lib.rs', '#[cfg(test)] mod session_tests;')
        self.write('examples/alpha/src/session_tests.rs', ACTIVE)
        self.write('examples/alpha/src/unused.rs', ACTIVE)
        self.write('examples/alpha/src/bin/helper.rs', ACTIVE)
        report = self.report(migration=True)
        self.assertTrue(report['passed'])
        self.assertEqual(report['results'][0]['migration']['status'], 'absent')

    def test_nested_declared_modules_contribute_session_evidence(self):
        self.write('examples/alpha/src/lib.rs', 'mod outer { mod runtime; }')
        self.write('examples/alpha/src/outer/runtime.rs', 'mod lifecycle;')
        self.write('examples/alpha/src/outer/runtime/lifecycle.rs', ACTIVE)
        report = self.report(require_session=True)
        self.assertTrue(report['passed'], report)
        (self.root / 'examples/alpha/src/outer/runtime/lifecycle.rs').unlink()
        self.assertIn('session_source', self.codes(self.report(migration=True)))

    def test_checked_sessions_require_registered_production_modules(self):
        for constructor in ('Loop::try_new(60., 4, true).expect("valid clock")',
                            'Loop::from_config(config)'):
            source = ACTIVE.replace('Loop :: new (60.0, 4, true)', constructor)
            self.write('examples/alpha/src/runtime.rs', source)
            for declaration, accepted in (('mod runtime;', True),
                                          ('#[cfg(test)] mod runtime;', False),
                                          ('// mod runtime;', False)):
                with self.subTest(constructor=constructor, declaration=declaration):
                    self.write('examples/alpha/src/lib.rs', declaration)
                    report = self.report(migration=True, require_session=True)
                    self.assertEqual(report['passed'], accepted, report)
                    self.assertEqual(report['results'][0]['migration']['constructors'], int(accepted))
                    self.assertTrue(report['results'][1]['passed'])

    def test_relocated_capability_anchor_requires_its_current_production_path(self):
        self.games[0]['capabilities'] = copy.deepcopy(CAPABILITIES)
        anchor = self.games[0]['capabilities']['pause']['simulation']['evidence'][0]
        self.write('examples/alpha/src/lib.rs', 'mod presentation;')
        self.write('examples/alpha/src/presentation.rs', ACTIVE)
        self.assertIn('capability_evidence', self.codes(self.report()))
        anchor['path'] = 'examples/alpha/src/presentation.rs'
        report = self.report()
        self.assertTrue(report['passed'], report)
        self.assertEqual(report['results'][0]['capabilities']['basis'], 'static-source')
        self.assertFalse(report['results'][0]['capabilities']['runtime_verified'])
        self.write(anchor['path'], '#[cfg(test)] mod tests {' + ACTIVE + '}')
        self.assertIn('capability_evidence', self.codes(self.report()))

    def test_capabilities_are_optional_but_always_audited_when_declared(self):
        self.games[0]['capabilities'] = copy.deepcopy(CAPABILITIES)
        report = self.report()
        self.assertTrue(report['passed'], report)
        self.assertNotIn('migration', report['results'][0])
        self.assertNotIn('capabilities', report['results'][1])
        self.assertEqual(report['summary']['capabilities'], {
            'declared_games': 1, 'source_matched': 1, 'unverified': 0, 'runtime_verified': False,
        })
        self.assertIn('Static source', report['capability_note'])
        self.games[0]['capabilities']['pause']['simulation']['evidence'][0]['snippet'] = 'clock.removed_api()'
        report = self.report(migration=True)
        self.assertFalse(report['passed'])
        self.assertEqual(report['summary']['migration'], {'active': 2})
        self.assertEqual(report['summary']['capabilities']['unverified'], 1)
        self.assertTrue(report['results'][1]['passed'])

    def test_capability_paths_must_be_registered_in_the_owning_example(self):
        self.games[0]['capabilities'] = copy.deepcopy(CAPABILITIES)
        anchor = self.games[0]['capabilities']['pause']['simulation']['evidence'][0]
        self.write('examples/alpha/src/unused.rs', ACTIVE)
        self.write('examples/alpha/src/tests.rs', ACTIVE)
        self.write('examples/alpha/src/lib.rs', '#[cfg(all(test, unix))] mod tests;')
        for path in ('examples/beta/src/lib.rs', 'examples/alpha/src/unused.rs',
                     'examples/alpha/src/tests.rs', 'examples/alpha/README.md',
                     '../alpha/src/lib.rs', '/etc/passwd', 'examples/alpha/src/../src/lib.rs',
                     'examples/alpha//src/lib.rs', r'examples\alpha\src\lib.rs'):
            with self.subTest(path=path):
                anchor['path'] = path
                self.assertIn('capability_evidence', self.codes(self.report()))

    def test_capability_module_graph_and_symlink_failures_produce_errors(self):
        self.games[0]['capabilities'] = copy.deepcopy(CAPABILITIES)
        anchor = self.games[0]['capabilities']['pause']['simulation']['evidence'][0]
        anchor['path'] = 'examples/alpha/src/runtime.rs'
        self.write('examples/alpha/src/lib.rs', '#[cfg(target_arch="wasm32")] mod runtime;')
        self.write(anchor['path'], ACTIVE)
        self.assertTrue(self.report()['passed'])
        module = self.root / anchor['path']
        module.unlink()
        report = self.report()
        self.assertIn('capability_source', self.codes(report))
        self.assertFalse(report['results'][0]['capabilities']['passed'])
        module.symlink_to(self.root / 'examples/beta/src/lib.rs')
        self.assertIn('capability_source', self.codes(self.report()))

    def test_custom_module_paths_cannot_substitute_unregistered_default_file(self):
        self.games[0]['capabilities'] = copy.deepcopy(CAPABILITIES)
        anchor = self.games[0]['capabilities']['pause']['simulation']['evidence'][0]
        anchor['path'] = 'examples/alpha/src/runtime.rs'
        self.write(anchor['path'], ACTIVE)
        self.write('examples/alpha/src/custom.rs', '// Real module has no session.')
        for attribute in ('#[path="custom.rs"]', '#[cfg_attr(unix, path="custom.rs")]'):
            with self.subTest(attribute=attribute):
                self.write('examples/alpha/src/lib.rs', attribute + ' mod runtime;')
                report = self.report(migration=True)
                self.assertIn('session_source', self.codes(report))
                self.assertIn('capability_evidence', self.codes(report))
                self.assertNotIn('migration', report['results'][0])

    def test_duplicate_json_keys_cannot_replace_declared_evidence(self):
        for duplicate in ('"id":"alpha","id":"beta"',
                          '"capabilities":{"schema_version":1,"schema_version":2}'):
            with self.subTest(duplicate=duplicate):
                self.write('examples/catalog.json', '[{' + duplicate + '}]')
                report = audit.audit_catalog(self.root)
                self.assertIn('catalog_input', self.codes(report))
                self.assertIn('duplicate JSON field', report['errors'][0]['message'])

    def test_schema_cli_needs_no_checkout_and_rejects_migration_flags(self):
        output = self.root / 'capabilities.schema.json'
        with patch.object(audit, 'ROOT', self.root / 'missing'), contextlib.redirect_stdout(io.StringIO()) as stdout:
            self.assertEqual(audit.main(['--capabilities-schema', '--output', str(output)]), 0)
        self.assertEqual(json.loads(stdout.getvalue()), audit.capabilities_schema())
        self.assertEqual(json.loads(output.read_text()), audit.capabilities_schema())
        with contextlib.redirect_stderr(io.StringIO()), self.assertRaises(SystemExit) as raised:
            audit.main(['--capabilities-schema', '--require-session'])
        self.assertEqual(raised.exception.code, 2)

    def test_cli_capability_errors_keep_json_output_and_nonzero_status(self):
        self.games[0]['capabilities'] = None
        self.save()
        with patch.object(audit, 'ROOT', self.root), contextlib.redirect_stdout(io.StringIO()) as stdout:
            self.assertEqual(audit.main(['--json']), 1)
        report = json.loads(stdout.getvalue())
        self.assertIn('capability_schema', self.codes(report))
        self.assertTrue(report['results'][1]['passed'])

    def test_cli_json_file_and_exit_codes(self):
        output = self.root / 'reports/catalog.json'
        with patch.object(audit, 'ROOT', self.root), contextlib.redirect_stdout(io.StringIO()) as stdout:
            code = audit.main(['--json', '--migration', '--output', str(output)])
        self.assertEqual(code, 0)
        self.assertEqual(json.loads(stdout.getvalue()), json.loads(output.read_text()))
        self.games[0]['source'] = 'missing.rs'
        self.save()
        with patch.object(audit, 'ROOT', self.root), contextlib.redirect_stdout(io.StringIO()) as stdout:
            self.assertEqual(audit.main(['--json']), 1)
        self.assertFalse(json.loads(stdout.getvalue())['passed'])


if __name__ == '__main__':
    unittest.main()
