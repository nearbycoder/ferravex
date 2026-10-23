"""Exercise capture validation and per-game failures without launching a display."""
import argparse
import contextlib
import hashlib
import importlib.util
import io
import json
import os
from pathlib import Path
import struct
import subprocess
import sys
import tempfile
import tomllib
import unittest
from unittest.mock import Mock, patch
import zlib

spec = importlib.util.spec_from_file_location('smoke', Path(__file__).with_name('smoke-games.py'))
smoke = importlib.util.module_from_spec(spec)
spec.loader.exec_module(smoke)


def chunk(kind, data):
    return struct.pack('>I', len(data)) + kind + data + struct.pack('>I', zlib.crc32(kind + data))


def png(width=112, height=112, blank=False, filter_byte=0, compressed=None, color=6):
    palette = [bytes([i, i // 2, 100, 255]) for i in (10, 70, 140, 240)]
    scanline = bytes([filter_byte]) + b''.join(palette[0 if blank else x % 4] for x in range(width))
    header = struct.pack('>IIBBBBB', width, height, 8, color, 0, 0, 0)
    return b'\x89PNG\r\n\x1a\n' + chunk(b'IHDR', header) + chunk(b'IDAT', zlib.compress(scanline * height) if compressed is None else compressed) + chunk(b'IEND', b'')


def cargo_artifact(root, executable, game='owned-orbit', fresh=False):
    return {'reason': 'compiler-artifact',
            'package_id': f'path+{(root / "examples" / game).as_uri()}#{game}@0.1.0',
            'manifest_path': str(root / 'examples' / game / 'Cargo.toml'),
            'target': {'kind': ['bin'], 'crate_types': ['bin'], 'name': game,
                       'src_path': str(root / 'examples' / game / 'src/main.rs')},
            'profile': {'opt_level': '0', 'debuginfo': 2, 'debug_assertions': True,
                        'overflow_checks': True, 'test': False},
            'features': [], 'filenames': [str(executable)], 'executable': str(executable),
            'fresh': fresh}


def cargo_output(*messages):
    return ''.join(json.dumps(message) + '\n' for message in messages)


BUILD_FINISHED = {'reason': 'build-finished', 'success': True}


class CaptureTests(unittest.TestCase):
    background = bytes([10, 20, 30, 255])
    palette = [bytes([50, 60, 70, 255]), bytes([90, 100, 110, 255]), bytes([150, 160, 170, 255])]

    def test_windows_stat_and_fstat_have_separate_mode_and_ctime_baselines(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'artifact.exe'
            path.write_bytes(b'unchanged artifact')
            path.chmod(0o755)
            actual_fstat = os.fstat

            def handle_stat(fd):
                from types import SimpleNamespace
                info = actual_fstat(fd)
                return SimpleNamespace(**{key: getattr(info, key) for key in
                    ('st_dev', 'st_ino', 'st_size', 'st_mtime_ns')},
                    st_mode=info.st_mode & ~0o111,
                    st_ctime_ns=info.st_ctime_ns + 100)

            with patch.object(smoke.sys, 'platform', 'win32'), \
                    patch.object(smoke.os, 'fstat', side_effect=handle_stat):
                result = smoke._artifact(path)
            self.assertEqual(result['status'], 'present', result)
            self.assertEqual(result['sha256'], hashlib.sha256(path.read_bytes()).hexdigest())

    def test_windows_handle_changes_still_reject_artifacts(self):
        from types import SimpleNamespace
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'artifact.bin'
            path.write_bytes(b'original artifact')
            actual_fstat = os.fstat
            for field in ('st_ctime_ns', 'st_mtime_ns', 'st_size', 'st_ino', 'st_mode'):
                calls = 0

                def changing_stat(fd):
                    nonlocal calls
                    calls += 1
                    info = actual_fstat(fd)
                    values = {key: getattr(info, key) for key in
                        ('st_dev', 'st_ino', 'st_mode', 'st_size', 'st_mtime_ns', 'st_ctime_ns')}
                    if calls == 2:
                        values[field] += 1
                    return SimpleNamespace(**values)

                with self.subTest(field=field), patch.object(smoke.sys, 'platform', 'win32'), \
                        patch.object(smoke.os, 'fstat', side_effect=changing_stat):
                    result = smoke._artifact(path)
                    self.assertEqual(result['status'], 'unavailable', result)
                    self.assertIn('changed while hashing', result['error'])

    def test_windows_path_replacement_after_hashing_is_rejected(self):
        from types import SimpleNamespace
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'artifact.bin'
            path.write_bytes(b'original artifact')
            original = path.lstat()
            values = {key: getattr(original, key) for key in
                ('st_dev', 'st_ino', 'st_mode', 'st_size', 'st_mtime_ns', 'st_ctime_ns')}
            values['st_ino'] += 1
            with patch.object(smoke.sys, 'platform', 'win32'), \
                    patch.object(Path, 'lstat', side_effect=[original, SimpleNamespace(**values)]):
                result = smoke._artifact(path, reject_symlink=True)
            self.assertEqual(result['status'], 'unavailable', result)
            self.assertIn('changed while hashing', result['error'])

    def test_accepts_small_authored_palette_with_substantial_geometry(self):
        samples = [self.background] * 800 + [self.palette[i % 3] for i in range(200)]
        result = smoke.validate_samples(iter(samples))
        self.assertEqual(result['sampled_colors'], 4)
        self.assertAlmostEqual(result['foreground_fraction'], 0.2)

    def test_rejects_clear_only_and_tiny_error_marks(self):
        for samples in ([self.background] * 1000, [self.background] * 997 + self.palette):
            with self.subTest(colors=len(set(samples))), self.assertRaisesRegex(ValueError, 'blank'):
                smoke.validate_samples(iter(samples))

    def test_rejects_transparent_output_even_with_color_variation(self):
        with self.assertRaisesRegex(ValueError, 'blank'):
            smoke.validate_samples(bytes([i, i, i, 0]) for i in range(200))

    def test_png_metrics_and_digest(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'capture.png'
            data = png()
            path.write_bytes(data)
            result = smoke.validate_capture(path)
        self.assertEqual((result['width'], result['height']), (112, 112))
        self.assertEqual(result['sampled_colors'], 4)
        self.assertEqual(result['sha256'], hashlib.sha256(data).hexdigest())

    def test_rejects_malformed_or_blank_captures(self):
        valid = png()
        corrupted = bytearray(valid)
        corrupted[30] ^= 1
        cases = [
            (b'not a png', 'invalid PNG'),
            (valid[:-1], 'truncated'),
            (valid[:-12], 'incomplete'),
            (valid + b'trailing', 'trailing'),
            (bytes(corrupted), 'checksum'),
            (png(width=99), 'too small'),
            (png(blank=True), 'blank'),
            (png(filter_byte=1), 'scanline'),
            (png(color=2), 'pixel layout'),
            (png(compressed=b'bad deflate'), 'compressed data'),
            (png(compressed=zlib.compress(b'x' * 70000)), 'scanline'),
            (png(compressed=zlib.compress(b'x')[:-1]), 'scanline'),
        ]
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'capture.png'
            for data, message in cases:
                with self.subTest(message=message):
                    path.write_bytes(data)
                    with self.assertRaisesRegex(ValueError, message):
                        smoke.validate_capture(path)


class SmokeTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name).resolve()
        self.binaries = self.root / 'build/debug'
        self.binaries.mkdir(parents=True)
        self.args = argparse.Namespace(profile='debug', frames=12, timeout=3,
                                       output=self.root / 'captures with spaces', build=False, build_timeout=7)
        self.runner = Mock(side_effect=self.success)

    def binary(self, game):
        path = self.binaries / game
        path.write_bytes(b'smoke binary fixture')
        return path

    def success(self, command, **kwargs):
        if command[:2] == ['cargo', 'metadata']:
            return subprocess.CompletedProcess(command, 0, json.dumps({'target_directory': str(self.binaries.parent)}), '')
        if command[:2] == ['cargo', 'build']:
            executable = self.binary(command[-1])
            return subprocess.CompletedProcess(command, 0, cargo_output(
                cargo_artifact(self.root, executable, command[-1]), BUILD_FINISHED), '')
        else:
            Path(command[-1]).write_bytes(png())
        return subprocess.CompletedProcess(command, 0, 'simulated stdout', '')

    def run_game(self, game='owned-orbit', environ=None, platform='linux'):
        return smoke.run_game(game, self.binaries, self.args, root=self.root,
                              runner=self.runner, environ={} if environ is None else environ, platform=platform)

    def test_headless_pass_is_prebuilt_not_build_verified(self):
        self.binary('owned-orbit')
        report = self.run_game()
        self.assertTrue(report['passed'], report)
        self.assertEqual(report['build']['status'], 'prebuilt')
        self.assertEqual(report['host'], 'headless-software')
        self.assertEqual(report['launch']['status'], 'passed')
        self.assertEqual(report['capture']['status'], 'passed')
        self.assertEqual(report['stdout'], 'simulated stdout')
        self.assertEqual(report['sha256'], report['capture']['sha256'])
        self.assertEqual(self.runner.call_args.kwargs['timeout'], 3)

    def test_artifact_hashes_sizes_and_times_match_published_files(self):
        executable = self.binary('owned-orbit')
        report = self.run_game()
        for artifact, path in ((report['build']['artifact'], executable),
                               (report['capture']['artifact'], self.args.output / 'owned-orbit.png')):
            with self.subTest(path=path):
                info = path.stat()
                self.assertEqual(artifact['status'], 'present')
                self.assertEqual(artifact['path'], str(path))
                self.assertEqual(artifact['sha256'], hashlib.sha256(path.read_bytes()).hexdigest())
                self.assertEqual(artifact['size_bytes'], info.st_size)
                self.assertEqual(artifact['mtime_ns'], info.st_mtime_ns)
                self.assertEqual(artifact['ctime_ns'], info.st_ctime_ns)
                self.assertIsInstance(artifact['observed_at_ns'], int)
        self.assertEqual(report['build']['artifact']['freshness'], 'unverified')
        self.assertEqual(report['capture']['artifact']['freshness'], 'fresh')
        self.assertTrue(report['launch']['binary_unchanged'])
        self.assertEqual(report['launch']['binary_after']['sha256'], report['build']['artifact']['sha256'])
        window = report['capture']['freshness_window']
        self.assertLessEqual(window['not_before_mtime_ns'], report['capture']['artifact']['mtime_ns'])
        self.assertLessEqual(report['capture']['artifact']['mtime_ns'], window['not_after_mtime_ns'])
        self.assertFalse(Path(report['capture']['requested_path']).parent.exists())

    def test_launch_records_exact_parameters_without_dumping_inherited_secrets(self):
        self.binary('owned-orbit')
        environ = {'DISPLAY': ':42', 'FERRAVEX_LOOK': 'ink', 'FERRAVEX_AUDIO': '',
                   'LIBGL_ALWAYS_SOFTWARE': '1', 'PRIVATE_API_TOKEN': 'not-for-report'}
        report = self.run_game(environ=environ)
        launch = report['launch']
        self.assertEqual(launch['command'], self.runner.call_args.args[0])
        self.assertEqual(launch['cwd'], str(self.runner.call_args.kwargs['cwd']))
        self.assertEqual(launch['timeout_seconds'], self.runner.call_args.kwargs['timeout'])
        for key in ('DISPLAY', 'FERRAVEX_LOOK', 'FERRAVEX_AUDIO', 'LIBGL_ALWAYS_SOFTWARE'):
            self.assertEqual(launch['environment'][key], environ[key])
        self.assertIsNone(launch['environment']['FERRAVEX_QUALITY'])
        self.assertNotIn('not-for-report', json.dumps(report))
        self.assertEqual(self.runner.call_args.kwargs['env'], environ)
        self.assertLessEqual(launch['started_at_ns'], launch['finished_at_ns'])

    def test_preserves_all_original_host_flags_and_paths_with_spaces(self):
        expectations = {
            'owned-lab': ['--frames', '12', '--output'],
            'owned-orbit': ['--autoplay', '--frames', '12', '--screenshot'],
            'strike': ['--play', '--smoke-frames', '12', '--screenshot'],
            'emberline': ['--autoplay', '--smoke-frames', '12', '--screenshot'],
        }
        for game, flags in expectations.items():
            with self.subTest(game=game):
                self.binary(game)
                report = self.run_game(game, environ={'DISPLAY': ':1'})
                self.assertTrue(report['passed'], report)
                command = self.runner.call_args.args[0]
                self.assertEqual(command[1:-1], flags)
                requested = Path(command[-1])
                self.assertEqual(requested.name, f'{game}.png')
                self.assertEqual(requested.parent.parent, self.args.output)
                self.assertEqual(command[-1], report['capture']['requested_path'])
                self.assertEqual(self.root / report['screenshot'], self.args.output / f'{game}.png')
                self.assertTrue((self.root / report['screenshot']).is_file())

    def test_missing_binary_is_separate_from_launch_and_capture(self):
        report = self.run_game()
        self.assertFalse(report['passed'])
        self.assertEqual(report['build']['status'], 'missing')
        self.assertEqual(report['build']['artifact']['status'], 'missing')
        self.assertEqual(report['build']['artifact']['freshness'], 'unverified')
        self.assertEqual(report['launch']['status'], 'skipped')
        self.assertEqual(report['capture']['status'], 'skipped')
        self.assertIn('--build', report['error'])
        self.runner.assert_not_called()

    def test_display_or_platform_limit_blocks_only_windowed_game(self):
        self.binary('emberline')
        for platform, environ, expected in [('linux', {}, 'DISPLAY'), ('darwin', {'DISPLAY': ':1'}, 'Linux')]:
            with self.subTest(platform=platform):
                report = self.run_game('emberline', environ=environ, platform=platform)
                self.assertEqual(report['status'], 'blocked')
                self.assertEqual(report['build']['status'], 'prebuilt')
                self.assertEqual(report['build']['artifact']['freshness'], 'unverified')
                self.assertIn('sha256', report['build']['artifact'])
                self.assertEqual(report['launch']['status'], 'blocked')
                self.assertIn(expected, report['environment_limitation'])
        self.runner.assert_not_called()

    def test_launch_failure_does_not_claim_capture(self):
        self.binary('owned-orbit')
        self.runner.side_effect = None
        self.runner.return_value = subprocess.CompletedProcess([], 7, 'partial', 'launch failed')
        report = self.run_game()
        self.assertEqual(report['exit_code'], 7)
        self.assertEqual(report['launch']['status'], 'failed')
        self.assertEqual(report['capture']['status'], 'skipped')
        self.assertIn('launch failed', report['error'])

    def test_launch_timeout_preserves_partial_output_as_json_strings(self):
        self.binary('owned-orbit')
        self.runner.side_effect = subprocess.TimeoutExpired('game', 3, output=b'partial\xff', stderr=b'waiting')
        report = self.run_game()
        self.assertEqual(report['launch']['status'], 'timeout')
        self.assertEqual(report['stderr'], 'waiting')
        self.assertIn('partial', report['stdout'])
        self.assertFalse(report['passed'])
        json.dumps(report)

    def test_launch_os_error_is_reported(self):
        self.binary('owned-orbit')
        self.runner.side_effect = PermissionError('not executable')
        report = self.run_game()
        self.assertEqual(report['launch']['status'], 'unavailable')
        self.assertIn('not executable', report['error'])

    def test_stale_capture_is_removed_even_if_process_returns_success(self):
        self.binary('owned-orbit')
        self.args.output.mkdir()
        screenshot = self.args.output / 'owned-orbit.png'
        screenshot.write_bytes(png())
        self.runner.side_effect = None
        self.runner.return_value = subprocess.CompletedProcess([], 0, '', '')
        report = self.run_game()
        self.assertFalse(screenshot.exists())
        self.assertEqual(report['launch']['status'], 'passed')
        self.assertEqual(report['capture']['status'], 'failed')
        self.assertEqual(report['capture']['artifact']['status'], 'missing')
        self.assertFalse(report['passed'])

    def test_backdated_and_future_captures_fail_even_when_png_is_valid(self):
        self.binary('owned-orbit')
        for stamp in (1_000_000_000, 4_000_000_000_000_000_000):
            with self.subTest(stamp=stamp):
                def stale_capture(command, **kwargs):
                    path = Path(command[-1])
                    path.write_bytes(png())
                    os.utime(path, ns=(stamp, stamp))
                    return subprocess.CompletedProcess(command, 0, '', '')
                self.runner.side_effect = stale_capture
                report = self.run_game()
                self.assertFalse(report['passed'])
                self.assertEqual(report['launch']['status'], 'passed')
                self.assertEqual(report['capture']['status'], 'failed')
                self.assertEqual(report['capture']['artifact']['freshness'], 'stale')
                self.assertEqual(report['capture']['artifact']['sha256'], hashlib.sha256(png()).hexdigest())
                self.assertIn('stale capture', report['error'])
                self.assertFalse((self.args.output / 'owned-orbit.png').exists())

    def test_same_pixels_from_new_launch_are_fresh_and_use_distinct_paths(self):
        self.binary('owned-orbit')
        first, second = self.run_game(), self.run_game()
        self.assertTrue(first['passed'], first)
        self.assertTrue(second['passed'], second)
        self.assertEqual(first['sha256'], second['sha256'])
        self.assertNotEqual(first['capture']['requested_path'], second['capture']['requested_path'])
        self.assertEqual(list(self.args.output.iterdir()), [self.args.output / 'owned-orbit.png'])

    def test_coarse_filesystem_timestamps_accept_equal_launch_boundaries(self):
        self.binary('owned-orbit')
        stamp = 1_000_000_000
        def rounded_capture(command, **kwargs):
            path = Path(command[-1])
            path.write_bytes(png())
            os.utime(path, ns=(stamp, stamp))
            return subprocess.CompletedProcess(command, 0, '', '')
        self.runner.side_effect = rounded_capture
        with patch.object(smoke, '_capture_boundary', return_value=stamp):
            report = self.run_game()
        self.assertTrue(report['passed'], report)
        self.assertEqual(report['capture']['artifact']['freshness'], 'fresh')

    def test_other_launch_writing_shared_capture_cannot_supply_this_launch(self):
        self.binary('owned-orbit')
        def wrong_capture(command, **kwargs):
            (self.args.output / 'owned-orbit.png').write_bytes(png())
            return subprocess.CompletedProcess(command, 0, '', '')
        self.runner.side_effect = wrong_capture
        report = self.run_game()
        self.assertFalse(report['passed'])
        self.assertEqual(report['capture']['artifact']['status'], 'missing')

    @unittest.skipUnless(os.name == 'posix', 'symlink creation requires platform support')
    def test_old_capture_symlink_does_not_delete_its_target(self):
        self.binary('owned-orbit')
        source = self.root / 'original.png'
        source.write_bytes(png())
        self.args.output.mkdir()
        (self.args.output / 'owned-orbit.png').symlink_to(source)
        report = self.run_game()
        self.assertTrue(report['passed'], report)
        self.assertEqual(source.read_bytes(), png())
        self.assertFalse((self.args.output / 'owned-orbit.png').is_symlink())

    @unittest.skipUnless(os.name == 'posix', 'symlink creation requires platform support')
    def test_launch_cannot_return_a_symlink_to_an_old_capture(self):
        self.binary('owned-orbit')
        source = self.root / 'original.png'
        source.write_bytes(png())
        def linked_capture(command, **kwargs):
            Path(command[-1]).symlink_to(source)
            return subprocess.CompletedProcess(command, 0, '', '')
        self.runner.side_effect = linked_capture
        report = self.run_game()
        self.assertFalse(report['passed'])
        self.assertEqual(report['capture']['status'], 'failed')
        self.assertIn('symlink', report['error'])
        self.assertEqual(source.read_bytes(), png())

    def test_binary_changed_or_removed_during_launch_never_passes(self):
        for removed in (False, True):
            with self.subTest(removed=removed):
                executable = self.binary('owned-orbit')
                def replaced_binary(command, **kwargs):
                    Path(command[-1]).write_bytes(png())
                    if removed:
                        executable.unlink()
                    else:
                        info = executable.stat()
                        executable.write_bytes(b'x' * info.st_size)
                        os.utime(executable, ns=(info.st_atime_ns, info.st_mtime_ns))
                    return subprocess.CompletedProcess(command, 0, '', '')
                self.runner.side_effect = replaced_binary
                report = self.run_game()
                self.assertFalse(report['passed'])
                self.assertFalse(report['launch']['binary_unchanged'])
                self.assertEqual(report['capture']['status'], 'skipped')
                self.assertIn('executable changed', report['error'])
                self.assertFalse((self.args.output / 'owned-orbit.png').exists())

    def test_binary_changed_while_hashing_does_not_launch_or_claim_a_digest(self):
        executable = self.binary('owned-orbit')
        digest = hashlib.sha256()
        def changing_update(block):
            digest.update(block)
            executable.write_bytes(b'different binary')
        wrapped = Mock(wraps=digest)
        wrapped.update.side_effect = changing_update
        with patch.object(smoke.hashlib, 'sha256', return_value=wrapped):
            report = self.run_game()
        self.assertFalse(report['passed'])
        self.assertEqual(report['build']['status'], 'unavailable')
        self.assertIn('changed while hashing', report['error'])
        self.assertNotIn('sha256', report['build']['artifact'])
        self.runner.assert_not_called()

    def test_capture_changed_during_validation_is_not_published(self):
        self.binary('owned-orbit')
        validate = smoke.validate_capture
        def replaced_capture(path):
            metrics = validate(path)
            path.write_bytes(png(width=113))
            return metrics
        with patch.object(smoke, 'validate_capture', side_effect=replaced_capture):
            report = self.run_game()
        self.assertFalse(report['passed'])
        self.assertEqual(report['capture']['artifact']['freshness'], 'changed')
        self.assertIn('changed during validation', report['error'])
        self.assertFalse((self.args.output / 'owned-orbit.png').exists())

    def test_capture_replaced_during_publication_is_not_reported_as_passed(self):
        self.binary('owned-orbit')
        replace = Path.replace
        def replaced_publication(path, target):
            result = replace(path, target)
            target.write_bytes(png(width=113))
            return result
        with patch.object(Path, 'replace', replaced_publication):
            report = self.run_game()
        self.assertFalse(report['passed'])
        self.assertEqual(report['capture']['artifact']['freshness'], 'changed')
        self.assertIn('changed during publication', report['error'])

    def test_failed_launch_keeps_diagnostic_hash_without_accepting_capture(self):
        self.binary('owned-orbit')
        def failed_launch(command, **kwargs):
            Path(command[-1]).write_bytes(png())
            return subprocess.CompletedProcess(command, 5, '', 'host failed')
        self.runner.side_effect = failed_launch
        report = self.run_game()
        self.assertFalse(report['passed'])
        self.assertEqual(report['capture']['status'], 'skipped')
        self.assertEqual(report['capture']['artifact']['sha256'], hashlib.sha256(png()).hexdigest())
        self.assertFalse((self.args.output / 'owned-orbit.png').exists())

    def test_corrupt_capture_is_recorded_after_successful_launch(self):
        self.binary('owned-orbit')
        def bad_capture(command, **kwargs):
            Path(command[-1]).write_bytes(png(compressed=b'bad'))
            return subprocess.CompletedProcess(command, 0, '', '')
        self.runner.side_effect = bad_capture
        report = self.run_game()
        self.assertEqual(report['launch']['status'], 'passed')
        self.assertEqual(report['capture']['status'], 'failed')
        self.assertEqual(report['capture']['artifact']['status'], 'present')
        self.assertIn('sha256', report['capture']['artifact'])

    def test_opt_in_build_uses_offline_locked_two_jobs_and_correct_profile(self):
        self.args.build = True
        report = self.run_game()
        self.assertTrue(report['passed'], report)
        self.assertEqual(report['build']['status'], 'passed')
        self.assertEqual(report['build']['artifact']['freshness'], 'build_verified')
        command = self.runner.call_args_list[0].args[0]
        self.assertEqual(command, ['cargo', 'build', '--offline', '--locked', '-j', '2', '--message-format=json', '--profile', 'dev', '-p', 'owned-orbit', '--bin', 'owned-orbit'])
        self.assertEqual(self.runner.call_args_list[0].kwargs['timeout'], 7)

    def test_cached_build_verifies_old_unchanged_binary(self):
        executable = self.binary('owned-orbit')
        old = 1_000_000_000
        os.utime(executable, ns=(old, old))
        self.args.build = True
        def cached_build(command, **kwargs):
            if command[:2] == ['cargo', 'build']:
                return subprocess.CompletedProcess(command, 0, cargo_output(
                    cargo_artifact(self.root, executable, fresh=True), BUILD_FINISHED), '')
            Path(command[-1]).write_bytes(png())
            return subprocess.CompletedProcess(command, 0, '', '')
        self.runner.side_effect = cached_build
        report = self.run_game()
        self.assertTrue(report['passed'], report)
        self.assertEqual(report['build']['artifact']['mtime_ns'], old)
        self.assertEqual(report['build']['artifact']['freshness'], 'build_verified')
        self.assertTrue(report['build']['cargo_artifact']['fresh'])

    def test_build_target_launches_distinct_fresh_qualified_bytes_not_stale_unqualified(self):
        self.args.build = True
        game, triple = 'owned-orbit', 'x86_64-unknown-linux-gnu'
        config = self.root / '.cargo/config.toml'
        config.parent.mkdir()
        for source in ('environment', 'config'):
            for profile in ('debug', 'release', 'small'):
                with self.subTest(source=source, profile=profile):
                    self.args.profile = profile
                    # A custom metadata target directory must also be honored.
                    target = self.root / 'custom target directory' / source
                    stale = target / profile / game
                    stale.parent.mkdir(parents=True, exist_ok=True)
                    stale.write_bytes(b'old unqualified binary')
                    os.utime(stale, ns=(1_000_000_000, 1_000_000_000))
                    fresh = target / triple / profile / game
                    fresh_bytes = f'new qualified binary: {source}/{profile}'.encode()
                    config.write_text('[build]\n' + (f'target = "{triple}"\n' if source == 'config' else ''))
                    environ = {'CARGO_TARGET_DIR': str(target), 'KEEP_INHERITED': 'yes'}
                    if source == 'environment':
                        environ['CARGO_BUILD_TARGET'] = triple
                    launched = []

                    def configured_build(command, **kwargs):
                        self.assertEqual(kwargs['env'], environ)
                        if command[:2] == ['cargo', 'metadata']:
                            return subprocess.CompletedProcess(command, 0, json.dumps({'target_directory': str(target)}), '')
                        if command[:2] == ['cargo', 'build']:
                            self.assertIn('--message-format=json', command)
                            self.assertNotIn('--target', command)
                            selected_target = kwargs['env'].get('CARGO_BUILD_TARGET') or tomllib.loads(config.read_text())['build']['target']
                            selected_profile = command[command.index('--profile') + 1]
                            self.assertEqual(selected_profile, 'dev' if profile == 'debug' else profile)
                            built = Path(kwargs['env']['CARGO_TARGET_DIR']) / selected_target / ('debug' if selected_profile == 'dev' else selected_profile) / game
                            built.parent.mkdir(parents=True, exist_ok=True)
                            built.write_bytes(fresh_bytes)
                            return subprocess.CompletedProcess(command, 0, cargo_output(
                                cargo_artifact(self.root, built), BUILD_FINISHED), '')
                        launched.append((command[0], Path(command[0]).read_bytes()))
                        Path(command[-1]).write_bytes(png())
                        return subprocess.CompletedProcess(command, 0, '', '')

                    self.runner.side_effect = configured_build
                    report = smoke.run_smoke([game], self.args, root=self.root, runner=self.runner,
                                             environ=environ, platform='linux')
                    self.assertTrue(report['passed'], report)
                    row = report['results'][0]
                    self.assertEqual(launched, [(str(fresh), fresh_bytes)])
                    self.assertEqual(stale.read_bytes(), b'old unqualified binary')
                    artifact = row['build']['artifact']
                    self.assertEqual(artifact['path'], str(fresh))
                    self.assertEqual(artifact['freshness'], 'build_verified')
                    self.assertEqual(artifact['sha256'], hashlib.sha256(fresh_bytes).hexdigest())
                    self.assertNotEqual(artifact['sha256'], hashlib.sha256(stale.read_bytes()).hexdigest())
                    self.assertEqual(row['build']['cargo_artifact']['executable'], str(fresh))
                    self.assertEqual(row['launch']['binary_after']['sha256'], artifact['sha256'])

    def test_build_artifact_needs_no_unqualified_file_and_preserves_exe_path(self):
        self.args.build = True
        executable = self.root / 'target with spaces/triple/debug/owned-orbit.exe'
        launched = []
        def qualified_build(command, **kwargs):
            if command[:2] == ['cargo', 'build']:
                executable.parent.mkdir(parents=True)
                executable.write_bytes(b'qualified exe fixture')
                return subprocess.CompletedProcess(command, 0, cargo_output(
                    cargo_artifact(self.root, executable), BUILD_FINISHED), '')
            launched.append(Path(command[0]).read_bytes())
            Path(command[-1]).write_bytes(png())
            return subprocess.CompletedProcess(command, 0, '', '')
        self.runner.side_effect = qualified_build
        report = self.run_game()
        self.assertTrue(report['passed'], report)
        self.assertEqual(launched, [b'qualified exe fixture'])
        self.assertEqual(report['launch']['command'][0], str(executable))
        self.assertFalse((self.binaries / 'owned-orbit').exists())

    def rejected_build(self, output, expected, returncode=0):
        """Leave a tempting old file in place; no rejected evidence may run it."""
        self.args.build = True
        self.binary('owned-orbit')
        self.runner.reset_mock()
        self.runner.side_effect = None
        self.runner.return_value = subprocess.CompletedProcess([], returncode, output, 'build diagnostic')
        report = self.run_game()
        self.assertFalse(report['passed'], report)
        self.assertEqual(report['build']['status'], 'failed')
        self.assertEqual(report['build']['artifact']['freshness'], 'unverified')
        self.assertEqual(report['launch']['status'], 'skipped')
        self.assertEqual(report['capture']['status'], 'skipped')
        self.assertIn(expected, report['error'])
        self.assertEqual(self.runner.call_count, 1)
        json.dumps(report)
        return report

    def test_build_rejects_absent_or_malformed_json_evidence(self):
        for output, expected in [('', 'build-finished'), ('\n', 'build-finished'),
                                 (cargo_output(BUILD_FINISHED), 'no compiler-artifact'),
                                 ('{\n', 'malformed Cargo JSON'), ('native output\n', 'malformed Cargo JSON'),
                                 ('null\n', 'expected an object'), ('[]\n', 'expected an object'),
                                 ('{}\n', 'expected an object'), ('{"reason":null}\n', 'expected an object')]:
            with self.subTest(output=output):
                self.rejected_build(output, expected)

    def test_build_rejects_incomplete_unsuccessful_or_nonterminal_completion(self):
        artifact = cargo_artifact(self.root, self.binaries / 'owned-orbit')
        for messages, expected in [([artifact], 'build-finished'),
                                   ([artifact, {'reason': 'build-finished', 'success': False}], 'success=true'),
                                   ([artifact, {'reason': 'build-finished', 'success': 1}], 'success=true'),
                                   ([artifact, {'reason': 'build-finished'}], 'success=true'),
                                   ([artifact, BUILD_FINISHED, BUILD_FINISHED], 'after build-finished'),
                                   ([BUILD_FINISHED, artifact], 'after build-finished')]:
            with self.subTest(messages=messages):
                self.rejected_build(cargo_output(*messages), expected)

    def test_failed_process_cannot_verify_an_otherwise_complete_artifact_stream(self):
        output = cargo_output(cargo_artifact(self.root, self.binaries / 'owned-orbit'), BUILD_FINISHED)
        report = self.rejected_build(output, 'exit 101', returncode=101)
        self.assertEqual(report['build']['exit_code'], 101)
        self.assertNotIn('cargo_artifact', report['build'])

    def test_build_rejects_ambiguous_distinct_and_duplicate_artifacts(self):
        first = cargo_artifact(self.root, self.binaries / 'owned-orbit')
        for executable in (self.binaries / 'owned-orbit', self.root / 'other-target/debug/owned-orbit'):
            with self.subTest(executable=executable):
                second = cargo_artifact(self.root, executable)
                self.rejected_build(cargo_output(first, second, BUILD_FINISHED), 'ambiguous')

    def test_build_rejects_unrelated_package_target_and_library_artifacts(self):
        executable = self.binaries / 'owned-orbit'
        other_manifest = cargo_artifact(self.root / 'unrelated', executable)
        other_target = cargo_artifact(self.root, executable, game='other-game')
        library = cargo_artifact(self.root, executable)
        library['target']['kind'] = ['rlib', 'cdylib']
        for artifact, expected in [(other_manifest, 'unrelated'), (other_target, 'no compiler-artifact'),
                                   (library, 'no compiler-artifact')]:
            with self.subTest(artifact=artifact):
                self.rejected_build(cargo_output(artifact, BUILD_FINISHED), expected)

    def test_build_rejects_malformed_candidate_artifact_fields(self):
        cases = [('target', None, 'target'), ('target', [], 'target'),
                 ('target', {'name': 'owned-orbit', 'kind': ['bin'], 'crate_types': ['rlib']}, 'crate_types'),
                 ('manifest_path', None, 'manifest_path'), ('manifest_path', 'relative/Cargo.toml', 'manifest_path'),
                 ('manifest_path', 'bad\0path', 'manifest_path'),
                 ('profile', None, 'non-test'), ('profile', {}, 'non-test'),
                 ('profile', {'test': True}, 'non-test'), ('profile', {'test': 0}, 'non-test'),
                 ('executable', None, 'absolute executable'), ('executable', '', 'absolute executable'),
                 ('executable', [], 'absolute executable'), ('executable', 'relative/owned-orbit', 'absolute executable'),
                 ('executable', '/bad\0path', 'absolute executable'),
                 ('filenames', [], 'filenames'), ('filenames', None, 'filenames'),
                 ('filenames', [str(self.binaries / 'owned-orbit'), None], 'filenames'),
                 ('package_id', None, 'package_id'), ('package_id', '', 'package_id'),
                 ('fresh', 1, 'fresh'), ('fresh', None, 'fresh')]
        for field, value, expected in cases:
            with self.subTest(field=field, value=value):
                artifact = cargo_artifact(self.root, self.binaries / 'owned-orbit')
                artifact[field] = value
                self.rejected_build(cargo_output(artifact, BUILD_FINISHED), expected)

    def test_build_rejects_missing_candidate_fields(self):
        for field in ('executable', 'manifest_path', 'target', 'profile', 'filenames', 'package_id', 'fresh'):
            with self.subTest(field=field):
                artifact = cargo_artifact(self.root, self.binaries / 'owned-orbit')
                del artifact[field]
                self.rejected_build(cargo_output(artifact, BUILD_FINISHED), 'cannot verify Cargo executable')

    def test_missing_or_nonregular_reported_executable_never_falls_back_to_old_binary(self):
        self.args.build = True
        self.binary('owned-orbit')
        for directory in (False, True):
            with self.subTest(directory=directory):
                executable = self.root / ('directory' if directory else 'missing-qualified-binary')
                if directory:
                    executable.mkdir()
                self.runner.reset_mock()
                self.runner.side_effect = None
                self.runner.return_value = subprocess.CompletedProcess([], 0, cargo_output(
                    cargo_artifact(self.root, executable), BUILD_FINISHED), '')
                report = self.run_game()
                self.assertFalse(report['passed'])
                self.assertEqual(report['build']['status'], 'unavailable' if directory else 'missing')
                self.assertEqual(report['build']['artifact']['path'], str(executable))
                self.assertEqual(report['build']['artifact']['freshness'], 'unverified')
                self.assertEqual(report['launch']['status'], 'skipped')
                self.assertEqual(self.runner.call_count, 1)
                self.assertIn('regular file' if directory else 'Cargo-reported executable is missing', report['error'])

    def test_build_parses_past_report_limit_and_ignores_dependency_messages(self):
        self.args.build = True
        executable = self.binary('owned-orbit')
        library = cargo_artifact(self.root, executable)
        library['target']['kind'] = ['rlib', 'cdylib']
        library['executable'] = None
        dependency = cargo_artifact(self.root, self.root / 'dependency-bin', game='dependency')
        output = cargo_output(library, dependency,
                              {'reason': 'compiler-message', 'message': {'rendered': 'warning ' * 5000}},
                              {'reason': 'build-script-executed', 'package_id': 'dependency'},
                              cargo_artifact(self.root, executable, fresh=True), BUILD_FINISHED)
        def verbose_build(command, **kwargs):
            if command[:2] == ['cargo', 'build']:
                return subprocess.CompletedProcess(command, 0, output, 'warning ' * 5000)
            return self.success(command, **kwargs)
        self.runner.side_effect = verbose_build
        report = self.run_game()
        self.assertTrue(report['passed'], report)
        self.assertTrue(report['build']['stdout_truncated'])
        self.assertTrue(report['build']['stderr_truncated'])
        self.assertLessEqual(len(report['build']['stdout']), smoke.MAX_BUILD_REPORT_OUTPUT)
        self.assertLessEqual(len(report['build']['stderr']), smoke.MAX_BUILD_REPORT_OUTPUT)
        self.assertEqual(report['build']['cargo_artifact']['executable'], str(executable))
        self.assertTrue(report['build']['cargo_artifact']['fresh'])

    def test_build_evidence_budgets_fail_closed(self):
        valid = cargo_output(cargo_artifact(self.root, self.binaries / 'owned-orbit'), BUILD_FINISHED)
        for setting, limit, output, expected in [
                ('MAX_BUILD_OUTPUT', 100, valid, 'output limit'),
                ('MAX_BUILD_MESSAGE', 100, valid, 'message count/size limit'),
                ('MAX_BUILD_MESSAGES', 1, valid, 'message count/size limit')]:
            with self.subTest(setting=setting), patch.object(smoke, setting, limit):
                self.rejected_build(output, expected)

    def test_build_timeout_retains_bounded_partial_spooled_output_and_never_launches(self):
        self.args.build = True
        self.binary('owned-orbit')
        def timed_out(command, **kwargs):
            kwargs['stdout'].write(b'partial JSON\n')
            kwargs['stderr'].write(b'waiting for cargo\n')
            raise subprocess.TimeoutExpired(command, kwargs['timeout'])
        self.runner.side_effect = timed_out
        report = self.run_game()
        self.assertEqual(report['build']['status'], 'timeout')
        self.assertEqual(report['build']['stdout'], 'partial JSON\n')
        self.assertEqual(report['build']['stderr'], 'waiting for cargo\n')
        self.assertEqual(report['build']['artifact']['freshness'], 'unverified')
        self.assertEqual(report['launch']['status'], 'skipped')
        self.assertEqual(self.runner.call_count, 1)

    def test_spooled_oversized_output_cannot_verify_a_complete_prefix(self):
        self.args.build = True
        executable = self.binary('owned-orbit')
        valid = cargo_output(cargo_artifact(self.root, executable), BUILD_FINISHED).encode()
        def oversized_build(command, **kwargs):
            kwargs['stdout'].write(valid + b' ')
            return subprocess.CompletedProcess(command, 0)
        self.runner.side_effect = oversized_build
        with patch.object(smoke, 'MAX_BUILD_OUTPUT', len(valid)):
            report = self.run_game()
        self.assertFalse(report['passed'])
        self.assertEqual(report['build']['status'], 'failed')
        self.assertTrue(report['build']['stdout_truncated'])
        self.assertIn('output limit', report['error'])
        self.assertEqual(report['build']['artifact']['freshness'], 'unverified')
        self.assertEqual(self.runner.call_count, 1)

    def test_failed_build_retains_final_error_after_verbose_stderr(self):
        self.args.build = True
        self.binary('owned-orbit')
        for spooled in (False, True):
            with self.subTest(spooled=spooled):
                def verbose_failure(command, **kwargs):
                    diagnostic = 'verbose ' * 5000 + 'final compiler error'
                    if spooled:
                        kwargs['stderr'].write(diagnostic.encode())
                        return subprocess.CompletedProcess(command, 101)
                    return subprocess.CompletedProcess(command, 101, '', diagnostic)
                self.runner.reset_mock()
                self.runner.side_effect = verbose_failure
                report = self.run_game()
                self.assertEqual(report['build']['status'], 'failed')
                self.assertTrue(report['build']['stderr_truncated'])
                self.assertLessEqual(len(report['build']['stderr']), smoke.MAX_BUILD_REPORT_OUTPUT)
                self.assertIn('final compiler error', report['error'])
                self.assertEqual(report['launch']['status'], 'skipped')
                self.assertEqual(self.runner.call_count, 1)

    @unittest.skipUnless(os.name == 'posix', 'fixture uses POSIX executable scripts')
    def test_real_subprocess_builder_output_selects_and_executes_qualified_bytes(self):
        self.args.build = True
        stale = self.binary('owned-orbit')
        stale.write_text(f'#!{sys.executable}\nprint("stale executable was launched")\n')
        stale.chmod(0o700)
        executable = self.root / 'target/triple/debug/owned-orbit'
        fresh_bytes = (f'#!{sys.executable}\nfrom pathlib import Path\nimport sys\n'
                       'print("fresh qualified executable ran")\n'
                       f'Path(sys.argv[-1]).write_bytes(bytes.fromhex({png().hex()!r}))\n').encode()
        builder = ('from pathlib import Path\nimport sys\n'
                   f'p = Path({str(executable)!r})\np.parent.mkdir(parents=True)\n'
                   f'p.write_bytes({fresh_bytes!r})\np.chmod(0o700)\n'
                   f'sys.stdout.write({cargo_output(cargo_artifact(self.root, executable), BUILD_FINISHED)!r})\n')
        def real_runner(command, **kwargs):
            # Cargo is modeled by a real Python subprocess; no Cargo/native
            # builds run. The created executable and its output are real bytes.
            if command[:2] == ['cargo', 'build']:
                return subprocess.run([sys.executable, '-B', '-c', builder], **kwargs)
            return subprocess.run(command, **kwargs)
        report = smoke.run_game('owned-orbit', self.binaries, self.args, root=self.root,
                                runner=real_runner, environ={}, platform='linux')
        self.assertTrue(report['passed'], report)
        self.assertEqual(report['stdout'], 'fresh qualified executable ran\n')
        self.assertEqual(report['launch']['command'][0], str(executable))
        self.assertEqual(report['build']['artifact']['sha256'], hashlib.sha256(fresh_bytes).hexdigest())
        self.assertEqual(report['capture']['artifact']['sha256'], hashlib.sha256(png()).hexdigest())
        self.assertNotEqual(executable.read_bytes(), stale.read_bytes())

    def test_failed_build_never_uses_old_executable(self):
        self.args.build = True
        self.binary('owned-orbit')
        for outcome, status in [(subprocess.CompletedProcess([], 101, '', 'compiler error'), 'failed'),
                                (subprocess.TimeoutExpired('cargo', 7), 'timeout')]:
            with self.subTest(status=status):
                self.runner.reset_mock()
                self.runner.side_effect = outcome if isinstance(outcome, Exception) else None
                self.runner.return_value = outcome
                report = self.run_game()
                self.assertEqual(report['build']['status'], status)
                self.assertEqual(report['build']['artifact']['freshness'], 'unverified')
                self.assertIn('sha256', report['build']['artifact'])
                self.assertEqual(report['launch']['status'], 'skipped')
                self.assertEqual(self.runner.call_count, 1)

    def test_metadata_failure_reports_every_selected_game(self):
        for failure in (FileNotFoundError('cargo unavailable'), subprocess.TimeoutExpired('cargo', 3)):
            with self.subTest(failure=type(failure).__name__):
                self.runner.side_effect = failure
                report = smoke.run_smoke(['owned-lab', 'emberline'], self.args, self.root, self.runner, {})
                self.assertEqual(report['summary']['games'], 2)
                self.assertEqual(report['summary']['blocked'], 2)
                self.assertEqual(report['summary']['stages']['capture'], {'skipped': 2})
                self.assertFalse(report['passed'])
                json.dumps(report)

    def test_invalid_metadata_is_actionable_not_a_traceback(self):
        self.runner.side_effect = None
        for output in ('{', '{}', 'null', '{"target_directory": []}'):
            with self.subTest(output=output):
                self.runner.return_value = subprocess.CompletedProcess([], 0, output, '')
                report = smoke.run_smoke(['owned-lab'], self.args, self.root, self.runner, {})
                self.assertFalse(report['passed'])
                self.assertIn('invalid cargo metadata', report['metadata']['error'])

    def test_report_counts_mixed_outcomes_and_continues_after_failure(self):
        self.binary('owned-lab')
        self.binary('emberline')
        report = smoke.run_smoke(['missing-demo', 'owned-lab', 'emberline'], self.args, self.root, self.runner, {}, 'linux')
        self.assertEqual([row['status'] for row in report['results']], ['failed', 'passed', 'blocked'])
        self.assertEqual({key: report['summary'][key] for key in ('passed', 'failed', 'blocked')}, {'passed': 1, 'failed': 1, 'blocked': 1})
        self.assertFalse(report['passed'])
        metadata_command = self.runner.call_args_list[0].args[0]
        self.assertIn('--offline', metadata_command)
        self.assertIn('--locked', metadata_command)
        json.dumps(report)

    def test_cli_json_report_and_legacy_flags(self):
        (self.root / 'examples').mkdir()
        (self.root / 'examples/catalog.json').write_text(json.dumps([{'id': 'owned-orbit'}, {'id': 'emberline'}]))
        self.binary('owned-orbit')
        with patch.object(smoke, 'ROOT', self.root), patch.object(smoke.subprocess, 'run', self.runner), contextlib.redirect_stdout(io.StringIO()) as stdout:
            code = smoke.main(['--game', 'owned-orbit', '--game', 'owned-orbit', '--profile', 'debug', '--frames', '12', '--timeout', '3', '--output', str(self.args.output), '--json'])
        self.assertEqual(code, 0)
        report = json.loads(stdout.getvalue())
        self.assertEqual(report, json.loads((self.args.output / 'smoke-results.json').read_text()))
        self.assertEqual(report['summary']['games'], 1)
        self.assertEqual(report['schema_version'], 2)

    @unittest.skipUnless(os.name == 'posix', 'fixture uses a POSIX executable script')
    def test_real_subprocess_fixture_publishes_fresh_capture(self):
        executable = self.binary('owned-orbit')
        executable.write_text(f'#!{sys.executable}\n'
                              'from pathlib import Path\nimport sys\n'
                              f'Path(sys.argv[-1]).write_bytes(bytes.fromhex({png().hex()!r}))\n')
        executable.chmod(0o700)
        report = smoke.run_game('owned-orbit', self.binaries, self.args, root=self.root,
                                environ={}, platform='linux')
        self.assertTrue(report['passed'], report)
        self.assertEqual(report['capture']['artifact']['sha256'], hashlib.sha256(png()).hexdigest())
        self.assertEqual(report['launch']['command'][0], str(executable))

    def test_cli_invalid_selection_and_budgets_do_not_launch(self):
        (self.root / 'examples').mkdir()
        (self.root / 'examples/catalog.json').write_text('[{"id":"owned-orbit"}]')
        for args in (['--game', 'unknown'], ['--frames', '0'], ['--timeout', '-1'], ['--build-timeout', '0']):
            with self.subTest(args=args), patch.object(smoke, 'ROOT', self.root), patch.object(smoke.subprocess, 'run', self.runner), contextlib.redirect_stderr(io.StringIO()):
                with self.assertRaises(SystemExit) as error:
                    smoke.main(args)
                self.assertEqual(error.exception.code, 2)
        self.runner.assert_not_called()

    def test_cli_blocked_environment_writes_json_and_returns_failure(self):
        (self.root / 'examples').mkdir()
        (self.root / 'examples/catalog.json').write_text('[{"id":"emberline"}]')
        self.binary('emberline')
        run_smoke = smoke.run_smoke

        def linux_smoke(*args, **kwargs):
            # Simulate display policy without changing the real filesystem's
            # platform-specific stat/fstat provenance rules on Windows.
            return run_smoke(*args, platform='linux', **kwargs)

        with patch.object(smoke, 'ROOT', self.root), patch.object(smoke.subprocess, 'run', self.runner), patch.dict(smoke.os.environ, {}, clear=True), patch.object(smoke, 'run_smoke', side_effect=linux_smoke), contextlib.redirect_stdout(io.StringIO()) as stdout:
            code = smoke.main(['--profile', 'debug', '--json', '--output', str(self.args.output)])
        report = json.loads(stdout.getvalue())
        self.assertEqual(code, 1)
        self.assertEqual(report['summary']['blocked'], 1)
        self.assertEqual(report['environment']['platform'], 'linux')
        self.assertIn('DISPLAY is unset', report['results'][0]['environment_limitation'])
        self.assertEqual(report, json.loads((self.args.output / 'smoke-results.json').read_text()))
        self.assertEqual(self.runner.call_count, 1)  # Metadata only; no game launch.

if __name__ == '__main__':
    unittest.main()
