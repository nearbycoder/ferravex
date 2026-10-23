import copy
from contextlib import redirect_stderr, redirect_stdout
import hashlib
import importlib.util
import io
import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest import mock

spec=importlib.util.spec_from_file_location('perf',Path(__file__).with_name('perf-games.py'))
perf=importlib.util.module_from_spec(spec); spec.loader.exec_module(perf)

class PerformanceFixtures:
    def fixture(self):
        samples=[{key:float(n) for key in (*perf.TIMINGS,*perf.WORK)} for n in range(1,101)]
        return {'schema':2,'machine':{'node':'test'},'config':{'frames':100},'games':{'demo':{
            'backend':'test','dimensions':[640,480],'metrics':perf.summarize(samples),'sizes':{'native_bytes':10000}}}}


class PerformanceContracts(PerformanceFixtures, unittest.TestCase):
    def test_nearest_rank_and_invalid_samples(self):
        row=self.fixture()['games']['demo']['metrics']['cpu_us']
        self.assertEqual((row['p50'],row['p95'],row['p99'],row['mean']),(50,95,99,50.5))
        with self.assertRaises(ValueError):perf.summarize([])
        with self.assertRaises(ValueError):perf.summarize([{'cpu_us':float('nan')}])
    def test_optional_async_gpu_and_quality_contracts(self):
        row=perf.summarize([{'cpu_us':1,'gpu_us':2},{'cpu_us':2,'gpu_us':None}])['gpu_us']
        self.assertEqual((row['samples'],row['coverage'],row['mean']),(1,.5,2))
        self.assertIsNone(perf.summarize([{'cpu_us':1}])['gpu_us'])
        with self.assertRaises(ValueError):perf.summarize([{'cpu_us':1},{'cpu_us':None}])
        old=self.fixture();current=copy.deepcopy(old)
        current['games']['demo']['metrics']['gpu_us']['coverage']=.4
        self.assertTrue(any('coverage' in x for x in perf.compare(current,old)))
        current=copy.deepcopy(old);current['games']['demo']['metrics']['gpu_us']['p99']+=100
        self.assertTrue(any('gpu_us/p99' in x for x in perf.compare(current,old)))
        current=copy.deepcopy(old);current['games']['demo']['render_quality']=2
        self.assertTrue(perf.compare(current,old))
    def test_regressions_fail_and_baseline_is_not_mutated(self):
        old=self.fixture(); current=copy.deepcopy(old)
        current['games']['demo']['metrics']['cpu_us']['p99']=999
        current['games']['demo']['sizes']['native_bytes']=20000
        failures=perf.compare(current,old)
        self.assertTrue(any('cpu_us/p99' in f for f in failures));self.assertTrue(any('native_bytes' in f for f in failures))
        self.assertEqual(old,self.fixture())
    def test_small_timer_noise_passes_but_work_growth_fails(self):
        old=self.fixture(); current=copy.deepcopy(old)
        current['games']['demo']['metrics']['cpu_us']['p99']+=50
        self.assertEqual(perf.compare(current,old),[])
        current['games']['demo']['metrics']['vertices']['max']=100000 # maxima are diagnostic; quantiles gate
        current['games']['demo']['metrics']['vertices']['mean']*=2
        self.assertTrue(perf.compare(current,old))
    def test_missing_coverage_and_incompatible_machine_fail_closed(self):
        old=self.fixture();current=copy.deepcopy(old);current['machine']['node']='other'
        self.assertTrue(perf.compare(current,old))
        current=copy.deepcopy(old);current['games']['demo']['metrics']['cpu_us']=None
        self.assertTrue(perf.compare(current,old))
        current=copy.deepcopy(old);current['games']={}
        self.assertTrue(perf.compare(current,old))


class ComparablePerformanceReports(PerformanceFixtures, unittest.TestCase):
    def checks_for(self, result, metric):
        return [check for check in result['checks'] if check.get('metric') == metric]

    def test_noise_preserves_samples_and_nearest_rank(self):
        frames = [{'cpu_us': value} for value in (9, 1, 3, 3)]
        before = copy.deepcopy(frames)
        summary = perf.summarize(frames)['cpu_us']
        self.assertEqual(frames, before)
        self.assertEqual(summary['samples'], 4)
        self.assertEqual(summary['mean'], 4)
        self.assertEqual(summary['p50'], 3)
        self.assertEqual(summary['p99'], 9)
        self.assertAlmostEqual(summary['noise']['sample_stdev'], 12 ** .5)
        self.assertEqual(summary['noise']['median_absolute_deviation'], 1)
        single = perf.summarize([{'cpu_us': 0}])['cpu_us']['noise']
        self.assertIsNone(single['sample_stdev'])
        self.assertEqual(single['median_absolute_deviation'], 0)
        for value in (True, False, -1, float('inf'), float('-inf'), '1'):
            with self.subTest(value=value), self.assertRaises(ValueError):
                perf.summarize([{'cpu_us': value}])

    def test_repetition_noise_keeps_sparse_gpu_and_single_runs_truthful(self):
        repetitions = [
            {'metrics': perf.summarize([{'cpu_us': 2, 'gpu_us': 4}] * 2)},
            {'metrics': perf.summarize([{'cpu_us': 6}] * 2)},
        ]
        noise = perf._repetition_noise(repetitions)
        cpu = noise['cpu_us']
        self.assertEqual((cpu['measured_runs'], cpu['total_runs']), (2, 2))
        self.assertEqual((cpu['mean_min'], cpu['mean_max']), (2, 6))
        self.assertAlmostEqual(cpu['noise_of_run_means']['sample_stdev'], 8 ** .5)
        self.assertEqual(noise['gpu_us']['measured_runs'], 1)
        self.assertIsNone(noise['gpu_us']['noise_of_run_means']['sample_stdev'])
        self.assertEqual(noise['vertices']['measured_runs'], 0)
        self.assertIsNone(noise['vertices']['noise_of_run_means'])

    def test_explicit_passing_limits_and_zero_baseline_delta(self):
        old = self.fixture()
        current = copy.deepcopy(old)
        row = current['games']['demo']['metrics']['cpu_us']
        row['p99'] += 200
        before = copy.deepcopy((current, old))
        result = perf.compare_reports(current, old)
        check = self.checks_for(result, 'cpu_us')[-1]
        self.assertEqual(result['status'], 'passed')
        self.assertEqual(result['schema'], 1)
        self.assertEqual(result['failures'], [])
        self.assertEqual(check['current'], 299)
        self.assertEqual(check['baseline'], 99)
        self.assertEqual(check['delta'], 200)
        self.assertAlmostEqual(check['delta_percent'], 200 / 99 * 100)
        self.assertEqual(check['limit'], 299)
        self.assertEqual(check['current_samples'], 100)
        self.assertEqual((current, old), before)
        row['p99'] += .01
        self.assertEqual(perf.compare_reports(current, old)['status'], 'regression')

        old['games']['demo']['metrics']['cpu_us']['mean'] = 0
        check = self.checks_for(perf.compare_reports(current, old), 'cpu_us')[0]
        self.assertEqual(check['delta'], 50.5)
        self.assertIsNone(check['delta_percent'])
        json.dumps(check, allow_nan=False)

    def test_insufficient_samples_on_either_side_do_not_claim_regressions(self):
        for side in ('current', 'baseline'):
            with self.subTest(side=side):
                old = self.fixture()
                current = copy.deepcopy(old)
                report = current if side == 'current' else old
                report['games']['demo']['metrics']['cpu_us']['samples'] = 99
                current['games']['demo']['metrics']['cpu_us']['p99'] = 9999
                result = perf.compare_reports(current, old)
                self.assertEqual(result['status'], 'insufficient_samples')
                checks = self.checks_for(result, 'cpu_us')
                self.assertTrue(all(c['status'] == 'insufficient_samples' for c in checks))
                self.assertTrue(all(f'{side} samples 99 < 100' in c['reason'] for c in checks))
                self.assertEqual(perf.compare(current, old), result['failures'])
                self.assertEqual(perf.compare_reports(current, old, min_samples=99)['status'],
                                 'regression')

    def test_sparse_gpu_baseline_and_coverage_drop_are_insufficient(self):
        for baseline_coverage, current_coverage in ((.4, 1.), (1., .4), (.9, .79)):
            with self.subTest(coverage=(baseline_coverage, current_coverage)):
                old = self.fixture()
                current = copy.deepcopy(old)
                old['games']['demo']['metrics']['gpu_us']['coverage'] = baseline_coverage
                current['games']['demo']['metrics']['gpu_us']['coverage'] = current_coverage
                current['games']['demo']['metrics']['gpu_us']['p99'] = 99999
                result = perf.compare_reports(current, old)
                self.assertEqual(result['status'], 'insufficient_samples')
                self.assertTrue(all(check['status'] == 'insufficient_samples'
                                    for check in self.checks_for(result, 'gpu_us')))
        old = self.fixture()
        current = copy.deepcopy(old)
        current['games']['demo']['metrics']['gpu_us']['coverage'] = .9
        self.assertEqual(perf.compare_reports(current, old)['status'], 'passed')
        current['games']['demo']['metrics']['cpu_us']['coverage'] = .99
        self.assertEqual(perf.compare_reports(current, old)['status'], 'insufficient_samples')

    def test_legacy_noise_absence_is_not_zero_or_an_incompatibility(self):
        current = self.fixture()
        old = copy.deepcopy(current)
        for summary in old['games']['demo']['metrics'].values():
            summary.pop('noise')
        result = perf.compare_reports(current, old)
        self.assertEqual(result['status'], 'passed')
        check = self.checks_for(result, 'cpu_us')[0]
        self.assertIsNone(check['baseline_noise'])
        self.assertIsNotNone(check['current_noise'])
        current['games']['demo']['metrics']['cpu_us']['noise']['sample_stdev'] = 1e10
        current['games']['demo']['metrics']['cpu_us']['p99'] = 999
        self.assertEqual(perf.compare_reports(current, old)['status'], 'regression')

    def test_mixed_outcomes_keep_regression_and_insufficient_checks(self):
        old = self.fixture()
        current = copy.deepcopy(old)
        current['games']['demo']['metrics']['cpu_us']['samples'] = 1
        current['games']['demo']['sizes']['native_bytes'] = 20000
        result = perf.compare_reports(current, old)
        self.assertEqual(result['status'], 'regression')
        self.assertIn('insufficient_samples', {check['status'] for check in result['checks']})
        artifact = next(check for check in result['checks'] if 'artifact' in check)
        self.assertEqual(artifact['delta'], 10000)
        self.assertEqual(artifact['limit'], 11024)
        current['games']['demo']['render_quality'] = 3
        result = perf.compare_reports(current, old)
        self.assertEqual(result['status'], 'incompatible')
        self.assertNotIn('regression', {check['status'] for check in result['checks']})

    def test_no_baseline_unsupported_metrics_and_missing_artifacts(self):
        old = self.fixture()
        self.assertEqual(perf.compare_reports(old, None)['status'], 'not_compared')
        old['games']['demo']['metrics']['gpu_us'] = None
        current = copy.deepcopy(old)
        result = perf.compare_reports(current, old)
        self.assertEqual(result['status'], 'passed')
        self.assertEqual(self.checks_for(result, 'gpu_us')[0]['status'], 'not_measured')
        current['games']['demo']['sizes'] = {}
        self.assertEqual(perf.compare_reports(current, old)['status'], 'incompatible')
        old['games'] = {}
        current['games'] = {}
        self.assertEqual(perf.compare_reports(current, old)['status'], 'incompatible')

    def test_empty_measurements_cannot_pass(self):
        report = self.fixture()
        report['games']['demo']['metrics'] = dict.fromkeys((*perf.TIMINGS, *perf.WORK))
        report['games']['demo']['sizes'] = {}
        result = perf.compare_reports(report, report)
        self.assertEqual(result['status'], 'insufficient_samples')
        self.assertIn('no measured metrics', result['failures'][0])

    def test_schema_mismatch_is_incompatible_without_comparing_metrics(self):
        old = self.fixture()
        current = copy.deepcopy(old)
        current['schema'] = 1
        current['games']['demo']['metrics']['cpu_us']['p99'] = 99999
        result = perf.compare_reports(current, old)
        self.assertEqual(result['status'], 'incompatible')
        self.assertEqual(result['checks'][0]['field'], 'schema')
        self.assertEqual(perf.compare(current, old), result['failures'])

    def test_invalid_comparison_inputs_are_not_silent_passes(self):
        old = self.fixture()
        for key in ('time_tolerance', 'time_floor', 'size_tolerance', 'gpu_floor'):
            for value in (float('nan'), float('inf'), -1, True):
                with self.subTest(key=key, value=value), self.assertRaises(ValueError):
                    perf.compare_reports(old, old, **{key: value})
        for value in (0, 1, 2.5, True, 163841):
            with self.subTest(min_samples=value), self.assertRaises(ValueError):
                perf.compare_reports(old, old, min_samples=value)
        for field, value in (('samples', -1), ('samples', True), ('coverage', float('nan')),
                             ('coverage', 1.1), ('mean', float('inf')), ('p99', -1),
                             ('mean', 10 ** 1000)):
            current = copy.deepcopy(old)
            current['games']['demo']['metrics']['cpu_us'][field] = value
            with self.subTest(field=field, value=value), self.assertRaises(ValueError):
                perf.compare_reports(current, old)
        for invalid in (None, [], {}, {'schema': 2}):
            with self.subTest(report=invalid), self.assertRaises(ValueError):
                perf.compare_reports(invalid, old)
        for field, value in (('metrics', None), ('metrics', {}), ('sizes', []),
                             ('sizes', {'native_bytes': float('nan')})):
            current = copy.deepcopy(old)
            current['games']['demo'][field] = value
            with self.subTest(field=field, value=value), self.assertRaises(ValueError):
                perf.compare_reports(current, old)


class PerformanceRunner(unittest.TestCase):
    """Exercise CLI/report plumbing with deterministic recordings, not real games."""

    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        (self.root / 'examples').mkdir()
        (self.root / 'examples/catalog.json').write_text('[{"id":"demo"}]')
        self.binary = self.root / 'target/release/demo'
        self.binary.parent.mkdir(parents=True)
        self.binary.write_bytes(b'fixture binary')
        self.cpu = 10
        self.recordings = {}
        self.calls = []
        patcher = mock.patch.object(perf, 'ROOT', self.root)
        patcher.start()
        self.addCleanup(patcher.stop)

    def run_process(self, command, **kwargs):
        self.calls.append(command)
        if command[:2] == ['cargo', 'build']:
            return subprocess.CompletedProcess(command, 0)
        self.assertEqual(command[0], str(self.binary))
        env = kwargs['env']
        count = int(command[command.index('--smoke-frames') + 1])
        warmup = int(env['FERRAVEX_PERF_WARMUP'])
        raw = Path(env['FERRAVEX_PERF_OUTPUT'])
        data = {
            'schema': 2, 'dropped': 0, 'warmup': warmup,
            'width': 640, 'height': 480, 'backend': 'test',
            'gpu_source': 'unsupported', 'render_quality': 1,
            'frames': [{'cpu_us': self.cpu, 'gpu_us': None,
                        'frame': index} for index in range(count - warmup)],
        }
        payload = json.dumps(data, separators=(',', ':')).encode() + b'\n'
        raw.write_bytes(payload)
        self.recordings[raw] = payload
        return subprocess.CompletedProcess(command, 0, '', '')

    def check_output(self, command, **kwargs):
        self.calls.append(command)
        if command[:2] == ['cargo', 'metadata']:
            return json.dumps({'target_directory': str(self.root / 'target')})
        if command[0] == 'git':
            return 'fixture-revision\n'
        if command[0] == 'rustc':
            return 'rustc fixture\n'
        self.fail(f'unexpected subprocess: {command}')

    def invoke(self, output, *options, frames=100):
        stdout, stderr = io.StringIO(), io.StringIO()
        argv = ['perf-games.py', '--game', 'demo', '--frames', str(frames),
                '--runs', '2', '--warmup', '1', '--output', str(output), *options]
        with (mock.patch.object(perf.sys, 'argv', argv),
              mock.patch.object(perf.subprocess, 'run', side_effect=self.run_process),
              mock.patch.object(perf.subprocess, 'check_output', side_effect=self.check_output),
              redirect_stdout(stdout), redirect_stderr(stderr)):
            status = perf.main()
        return status, stdout.getvalue(), stderr.getvalue()

    def test_report_retains_raw_data_and_records_baseline_identity(self):
        baseline = self.root / 'baseline.json'
        output = self.root / 'before'
        status, stdout, stderr = self.invoke(output, '--write-baseline', str(baseline))
        self.assertFalse(status)
        self.assertIn('Comparison: not_compared', stdout)
        self.assertEqual(stderr, '')
        report = json.loads(baseline.read_bytes())
        self.assertEqual(report['schema'], 2)
        self.assertEqual(report['analysis']['schema'], 1)
        game = report['games']['demo']
        self.assertEqual(len(game['repetitions']), 2)
        self.assertEqual(game['metrics']['cpu_us']['samples'], 200)
        self.assertEqual(game['repetition_noise']['cpu_us']['measured_runs'], 2)
        for repetition in game['repetitions']:
            raw = Path(repetition['raw_file'])
            self.assertEqual(raw.read_bytes(), self.recordings[raw])
            self.assertEqual(repetition['raw_sha256'], hashlib.sha256(raw.read_bytes()).hexdigest())
            self.assertEqual(repetition['metrics']['cpu_us']['samples'], 100)
        before = baseline.read_bytes()
        self.cpu = 1000
        status, _, stderr = self.invoke(self.root / 'after', '--baseline', str(baseline))
        self.assertTrue(status)
        comparison = json.loads((self.root / 'after/report.json').read_bytes())['comparison']
        self.assertEqual(comparison['status'], 'regression')
        self.assertEqual(comparison['baseline_sha256'], hashlib.sha256(before).hexdigest())
        self.assertEqual(comparison['baseline_revision'], 'fixture-revision')
        self.assertEqual(baseline.read_bytes(), before)
        self.assertIn('REGRESSION demo/cpu_us', stderr)
        self.assertNotIn('INSUFFICIENT_SAMPLES', stderr)
        for command in self.calls:
            if command[0] == 'cargo':
                self.assertIn('--offline', command)
                self.assertIn('--locked', command)
                if command[1] == 'build':
                    self.assertEqual(command[command.index('-j') + 1], '2')

    def test_cli_labels_insufficient_samples_and_incompatible_runs(self):
        baseline = self.root / 'baseline.json'
        self.invoke(self.root / 'before', '--write-baseline', str(baseline), frames=1)
        self.cpu = 1000
        status, _, stderr = self.invoke(self.root / 'after', '--baseline', str(baseline), frames=1)
        self.assertTrue(status)
        comparison = json.loads((self.root / 'after/report.json').read_bytes())['comparison']
        self.assertEqual(comparison['status'], 'insufficient_samples')
        self.assertIn('INSUFFICIENT_SAMPLES demo/cpu_us', stderr)
        self.assertNotIn('REGRESSION', stderr)
        status, _, stderr = self.invoke(self.root / 'override', '--baseline', str(baseline),
                                       '--min-samples', '2', frames=1)
        self.assertTrue(status)
        self.assertIn('REGRESSION demo/cpu_us', stderr)
        self.assertNotIn('INSUFFICIENT_SAMPLES', stderr)
        status, _, stderr = self.invoke(self.root / 'incompatible', '--baseline', str(baseline),
                                       frames=2)
        self.assertTrue(status)
        self.assertIn('INCOMPATIBLE baseline config differs', stderr)
        self.assertNotIn('REGRESSION', stderr)

    def test_reused_output_preserves_baseline_raw_files_and_no_build(self):
        baseline = self.root / 'baseline.json'
        output = self.root / 'current'
        self.invoke(output, '--write-baseline', str(baseline))
        original = dict(self.recordings)
        self.calls.clear()
        self.cpu = 20
        status, _, _ = self.invoke(output, '--baseline', str(baseline), '--no-build')
        self.assertFalse(status)
        self.assertEqual(len(self.recordings), 4)
        for path, payload in original.items():
            self.assertEqual(path.read_bytes(), payload)
        self.assertFalse(any(command[:2] == ['cargo', 'build'] for command in self.calls))
        report = json.loads((output / 'report.json').read_bytes())
        for repetition in report['games']['demo']['repetitions']:
            self.assertNotIn(Path(repetition['raw_file']), original)

    def test_cli_rejects_bad_policy_and_baseline_overwrite_before_running(self):
        for options in (('--min-samples', '1'), ('--time-tolerance', 'nan'),
                        ('--baseline', str(self.root / 'after/report.json'))):
            with self.subTest(options=options), self.assertRaises(SystemExit) as caught:
                self.invoke(self.root / 'after', *options)
            self.assertEqual(caught.exception.code, 2)
        self.assertEqual(self.calls, [])

if __name__=='__main__':unittest.main()
