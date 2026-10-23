#!/usr/bin/env python3
"""Local performance runs with raw samples and explicit, machine-local comparisons.

Use --write-baseline PATH to record a baseline, then --baseline PATH to compare.
Report schema 2 and raw recordings remain compatible. Additive analysis and
comparison metadata have their own schema version (1). Noise is descriptive:
frames can be correlated, and variation never silently increases a budget.
Each invocation keeps raw files in a fresh recording-* directory under --output;
reusing an output directory does not overwrite the baseline's raw measurements.
"""
import argparse
import gzip
import hashlib
import json
import math
import os
from pathlib import Path
import platform
import statistics
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parent.parent
TIMINGS = ('cpu_us', 'submit_us', 'software_render_us', 'gpu_us')
WORK = ('draw_calls', 'vertices', 'upload_bytes', 'allocation_calls', 'allocation_bytes',
        'physics_steps', 'physics_pairs', 'contact_points', 'ccd_impacts')
DEFAULT_MIN_SAMPLES = 100


def _nonnegative_number(value):
    if not isinstance(value, (int, float)) or isinstance(value, bool):
        return False
    try:
        return math.isfinite(value) and value >= 0
    except OverflowError:
        return False


def _noise(values):
    median = statistics.median(values)
    return {
        'sample_stdev': statistics.stdev(values) if len(values) > 1 else None,
        'median_absolute_deviation': statistics.median(
            abs(value - median) for value in values),
    }


def summarize(frames):
    """Summarize every measured value, without trimming or mutating raw frames.

    Nearest-rank percentiles and existing fields retain their meaning. ``noise``
    adds sample standard deviation (n - 1) and median absolute deviation in the
    metric's units; one observation has no estimable sample standard deviation.

    >>> summarize([{'cpu_us': 2}, {'cpu_us': 4}])['cpu_us']['mean']
    3
    """
    if not frames:
        raise ValueError('recording has no measured frames')
    out = {}
    for key in (*TIMINGS, *WORK):
        values = [f[key] for f in frames if f.get(key) is not None]
        if not values:
            out[key] = None
            continue
        if ((key != 'gpu_us' and len(values) != len(frames))
                or any(not _nonnegative_number(v) for v in values)):
            raise ValueError(f'invalid or incomplete metric {key}')
        values.sort()
        out[key] = {
            'samples': len(values), 'coverage': len(values) / len(frames),
            'mean': statistics.mean(values),
            'p50': values[math.ceil(len(values) * .5) - 1],
            'p95': values[math.ceil(len(values) * .95) - 1],
            'p99': values[math.ceil(len(values) * .99) - 1],
            'max': values[-1], 'noise': _noise(values),
        }
    return out


def _repetition_noise(repetitions):
    out = {}
    for metric in (*TIMINGS, *WORK):
        means = [run['metrics'][metric]['mean'] for run in repetitions
                 if run['metrics'][metric] is not None]
        out[metric] = {
            'measured_runs': len(means), 'total_runs': len(repetitions),
            'mean_min': min(means) if means else None,
            'mean_max': max(means) if means else None,
            'noise_of_run_means': _noise(means) if means else None,
        }
    return out


def _comparison_policy(time_tolerance, time_floor, size_tolerance, gpu_floor,
                       min_samples):
    if any(not _nonnegative_number(n) for n in
           (time_tolerance, time_floor, size_tolerance, gpu_floor)):
        raise ValueError('budget tolerances must be finite and nonnegative')
    if type(min_samples) is not int or not 2 <= min_samples <= 8192 * 20:
        raise ValueError('min_samples must be an integer in 2..163840')
    return {
        'time_tolerance': time_tolerance, 'time_floor_us': time_floor,
        'gpu_time_floor_us': gpu_floor, 'size_tolerance': size_tolerance,
        'work_tolerance': .05, 'work_floor': 1., 'size_floor_bytes': 1024,
        'min_samples': min_samples, 'min_gpu_coverage': .5,
        'max_gpu_coverage_drop': .1,
    }


def _finish_comparison(checks, policy, baseline_present=True):
    # Mixed outcomes remain visible in checks. A valid regression does not hide
    # insufficient samples elsewhere, and an incomplete run never passes.
    statuses = ('incompatible', 'regression', 'insufficient_samples')
    status = next((s for s in statuses if any(c['status'] == s for c in checks)),
                  'passed' if baseline_present else 'not_compared')
    return {
        'schema': 1, 'status': status, **policy, 'checks': checks,
        'failures': [c['reason'] for c in checks if c['status'] in statuses],
    }


def _validate_metric(metric, label):
    if not isinstance(metric, dict):
        raise ValueError(f'{label}: expected a metric summary')
    if (type(metric.get('samples')) is not int or metric['samples'] < 0
            or not _nonnegative_number(metric.get('coverage'))
            or metric['coverage'] > 1
            or any(not _nonnegative_number(metric.get(q))
                   for q in ('mean', 'p95', 'p99'))):
        raise ValueError(f'{label}: invalid samples, coverage or statistics')


def _validate_report(report, label):
    if (not isinstance(report, dict) or report.get('schema') != 2
            or not isinstance(report.get('machine'), dict)
            or not isinstance(report.get('config'), dict)
            or not isinstance(report.get('games'), dict)):
        raise ValueError(f'{label}: expected a schema-2 performance report')
    for game, row in report['games'].items():
        if (not isinstance(row, dict) or not isinstance(row.get('metrics'), dict)
                or not isinstance(row.get('sizes'), dict)
                or any(key not in row['metrics'] for key in (*TIMINGS, *WORK))):
            raise ValueError(f'{label}/{game}: expected metrics and sizes')
        for metric in (*TIMINGS, *WORK):
            if row['metrics'][metric] is not None:
                _validate_metric(row['metrics'][metric], f'{label}/{game}/{metric}')
        if any(type(size) is not int or not _nonnegative_number(size)
               for size in row['sizes'].values()):
            raise ValueError(f'{label}/{game}: invalid artifact size')


def _measurement_check(current, baseline, tolerance, **context):
    limit = baseline + tolerance
    if not math.isfinite(limit):
        raise ValueError('comparison limit must be finite')
    delta = current - baseline
    relative = delta / baseline * 100 if baseline else None
    return {
        **context, 'current': current, 'baseline': baseline, 'delta': delta,
        'delta_percent': relative if relative is None or math.isfinite(relative) else None,
        'tolerance': tolerance, 'limit': limit,
        'status': 'regression' if current > limit else 'passed',
    }


def compare_reports(current, baseline, time_tolerance=.2, time_floor=200.,
                    size_tolerance=.05, gpu_floor=20., *,
                    min_samples=DEFAULT_MIN_SAMPLES):
    """Return versioned checks with values, deltas, limits and outcome reasons.

    ``baseline=None`` explicitly yields ``not_compared``. Otherwise outcomes are
    ``passed``, ``regression``, ``insufficient_samples`` or ``incompatible``;
    unsupported metrics on both sides are ``not_measured``. All nonpassing
    outcomes remain in the legacy ``failures`` list. Overall status precedence
    is incompatible, regression, insufficient_samples, passed.

    Both sides need ``min_samples`` observations per measured metric. The default
    100 offers one observation per percentile point for p99, not statistical
    confidence. GPU coverage checks apply to both sides before timing gates.
    Old schema-2 reports without noise fields work; absent noise remains None.
    Inputs are never mutated. Existing local budget defaults are unchanged.

    >>> compare_reports({}, None)['status']
    'not_compared'
    """
    policy = _comparison_policy(time_tolerance, time_floor, size_tolerance,
                                gpu_floor, min_samples)
    checks = []
    if baseline is None:
        return _finish_comparison(checks, policy, baseline_present=False)
    for label, report in (('current', current), ('baseline', baseline)):
        if not isinstance(report, dict) or type(report.get('schema')) is not int:
            raise ValueError(f'{label}: expected a versioned performance report')
    if current['schema'] != 2 or baseline['schema'] != 2:
        checks.append({'status': 'incompatible', 'field': 'schema',
                       'reason': 'baseline schema differs or is unsupported; schema 2 required'})
        return _finish_comparison(checks, policy)
    for label, report in (('current', current), ('baseline', baseline)):
        _validate_report(report, label)
    for key in ('schema', 'machine', 'config', 'rustc'):
        if current.get(key) != baseline.get(key):
            checks.append({
                'status': 'incompatible', 'field': key,
                'reason': f'baseline {key} differs; record a baseline for this machine/configuration',
            })
    if set(current['games']) != set(baseline['games']):
        checks.append({'status': 'incompatible', 'field': 'games',
                       'reason': 'baseline game coverage differs'})
    elif not current['games']:
        checks.append({'status': 'incompatible', 'field': 'games',
                       'reason': 'no games to compare'})
    if checks:
        return _finish_comparison(checks, policy)
    for game, row in current['games'].items():
        old = baseline['games'][game]
        if any(row.get(key) != old.get(key) for key in
               ('backend', 'dimensions', 'gpu_source', 'render_quality')):
            checks.append({'status': 'incompatible', 'game': game,
                           'reason': f'{game}: renderer, dimensions, GPU timer or quality differ'})
            continue
        for metric in (*TIMINGS, *WORK):
            a, b = row['metrics'][metric], old['metrics'][metric]
            context = {'game': game, 'metric': metric}
            if (a is None) != (b is None):
                checks.append({**context, 'status': 'incompatible',
                               'reason': f'{game}/{metric}: measurement availability changed'})
                continue
            if a is None:
                checks.append({**context, 'status': 'not_measured',
                               'reason': f'{game}/{metric}: not measured in either report'})
                continue
            context.update({
                'current_samples': a['samples'], 'baseline_samples': b['samples'],
                'current_coverage': a['coverage'], 'baseline_coverage': b['coverage'],
                'current_noise': a.get('noise'), 'baseline_noise': b.get('noise'),
                'current_repetition_noise': row.get('repetition_noise', {}).get(metric),
                'baseline_repetition_noise': old.get('repetition_noise', {}).get(metric),
            })
            insufficient = []
            for label, summary in (('current', a), ('baseline', b)):
                if summary['samples'] < min_samples:
                    insufficient.append(f'{label} samples {summary["samples"]} < {min_samples}')
                required_coverage = .5 if metric == 'gpu_us' else 1.
                if summary['coverage'] < required_coverage:
                    insufficient.append(f'{label} coverage {summary["coverage"]:.3f} < {required_coverage}')
            if metric == 'gpu_us' and a['coverage'] < b['coverage'] - .1:
                insufficient.append('GPU query coverage dropped by more than 0.1')
            for quantile in ('mean', 'p95', 'p99'):
                if insufficient:
                    checks.append({
                        **context, 'statistic': quantile, 'status': 'insufficient_samples',
                        'current': a[quantile], 'baseline': b[quantile],
                        'reason': f'{game}/{metric}/{quantile}: ' + '; '.join(insufficient),
                    })
                    continue
                # Existing local allowances; measured noise never widens them.
                floor = gpu_floor if metric == 'gpu_us' else time_floor
                tolerance = (max(floor, b[quantile] * time_tolerance)
                             if metric in TIMINGS else max(1., b[quantile] * .05))
                check = _measurement_check(a[quantile], b[quantile], tolerance,
                                           **context, statistic=quantile)
                if check['status'] == 'regression':
                    check['reason'] = (f'{game}/{metric}/{quantile}: {a[quantile]:.2f} > '
                                       f'{check["limit"]:.2f} (baseline {b[quantile]:.2f})')
                checks.append(check)
        if set(old['sizes']) != set(row['sizes']):
            checks.append({'status': 'incompatible', 'game': game, 'field': 'sizes',
                           'reason': f'{game}: artifact coverage differs'})
        for artifact in sorted(row['sizes'].keys() & old['sizes'].keys()):
            value, previous = row['sizes'][artifact], old['sizes'][artifact]
            check = _measurement_check(value, previous,
                                       max(1024, previous * size_tolerance),
                                       game=game, artifact=artifact)
            if check['status'] == 'regression':
                check['reason'] = f'{game}/{artifact}: {value} bytes exceeds size budget'
            checks.append(check)
    if all(check['status'] == 'not_measured' for check in checks):
        checks.append({'status': 'insufficient_samples',
                       'reason': 'no measured metrics or artifacts to compare'})
    return _finish_comparison(checks, policy)


def compare(current, baseline, time_tolerance=.2, time_floor=200.,
            size_tolerance=.05, gpu_floor=20., *, min_samples=DEFAULT_MIN_SAMPLES):
    """Compatibility wrapper returning nonpassing reasons, including low samples.

    Use :func:`compare_reports` to distinguish regressions from insufficient
    evidence or incompatible workloads and to inspect passing comparisons too.

    >>> compare({}, None)
    []
    """
    return compare_reports(current, baseline, time_tolerance, time_floor,
                           size_tolerance, gpu_floor, min_samples=min_samples)['failures']


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--game', action='append', help='catalog id; repeat for a subset')
    p.add_argument('--profile', choices=('release','small','debug'), default='release')
    p.add_argument('--frames', type=int, default=180, help='measured frames per repetition')
    p.add_argument('--warmup', type=int, default=60)
    p.add_argument('--runs', type=int, default=3)
    p.add_argument('--quality', choices=('default','low','medium','high'), default='default')
    p.add_argument('--timeout', type=int, default=180)
    p.add_argument('--output', type=Path, default=ROOT/'target/perf/current')
    mode=p.add_mutually_exclusive_group()
    mode.add_argument('--baseline', type=Path,
                      help='compare; nonzero for regressions, insufficient samples or incompatibility')
    mode.add_argument('--write-baseline', type=Path, help='explicitly create/replace an approved baseline')
    p.add_argument('--no-build', action='store_true', help='use existing binaries; caller is responsible for freshness')
    p.add_argument('--time-tolerance', type=float, default=.2)
    p.add_argument('--time-floor-us', type=float, default=200.)
    p.add_argument('--gpu-time-floor-us', type=float, default=20.)
    p.add_argument('--size-tolerance', type=float, default=.05)
    p.add_argument('--min-samples', type=int, default=DEFAULT_MIN_SAMPLES,
                   help='minimum observations per metric in BOTH reports (default: 100; '
                        'p99 resolution floor, not a confidence guarantee)')
    a=p.parse_args()
    if not 1 <= a.frames <= 8192 or not 0 <= a.warmup <= 10000 or not 1 <= a.runs <= 20 or a.timeout < 1:
        p.error('frames 1..8192, warmup 0..10000, runs 1..20, timeout > 0 required')
    try:
        _comparison_policy(a.time_tolerance, a.time_floor_us, a.size_tolerance,
                           a.gpu_time_floor_us, a.min_samples)
    except ValueError as error:
        p.error(str(error))
    baseline = None
    baseline_sha256 = None
    if a.baseline:
        if a.baseline.resolve() == (a.output / 'report.json').resolve():
            p.error('--baseline must differ from the output report.json; preserve the baseline')
        baseline_bytes = a.baseline.read_bytes()
        baseline = json.loads(baseline_bytes)
        baseline_sha256 = hashlib.sha256(baseline_bytes).hexdigest()
        try:
            _validate_report(baseline, 'baseline')
        except ValueError as error:
            p.error(str(error))
    catalog=json.loads((ROOT/'examples/catalog.json').read_text())
    known={g['id'] for g in catalog}
    if a.game and set(a.game)-known:
        p.error('unknown games: '+', '.join(sorted(set(a.game)-known)))
    games=[g['id'] for g in catalog if not a.game or g['id'] in a.game]
    if not a.no_build:
        cmd=['cargo','build','--offline','--locked','-j','2','--bins']
        if a.profile != 'debug': cmd += ['--profile',a.profile]
        for game in games: cmd += ['-p',game]
        subprocess.run(cmd,cwd=ROOT,check=True)
    metadata=json.loads(subprocess.check_output(['cargo','metadata','--no-deps','--format-version','1','--offline','--locked'],cwd=ROOT,text=True))
    target=Path(metadata['target_directory'])
    a.output.mkdir(parents=True,exist_ok=True)
    raw_directory = Path(tempfile.mkdtemp(prefix='recording-', dir=a.output)).resolve()
    machine={'system':platform.system(),'release':platform.release(),'arch':platform.machine(),'node':platform.node()}
    report={'schema':2,'machine':machine,'config':{'profile':a.profile,'frames':a.frames,'warmup':a.warmup,'runs':a.runs,'workload':'catalog-autoplay-v1','quality':a.quality},
            'revision':subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip(),
            'rustc':subprocess.check_output(['rustc','--version'],text=True).strip(),'games':{}}
    report['analysis'] = {
        'schema': 1, 'quantiles': 'nearest_rank',
        'frame_noise': 'sample standard deviation (n - 1) and median absolute deviation',
        'repetition_noise': 'same estimators applied to measured repetition means',
        'interpretation': 'Descriptive variation, not confidence intervals; frames may be '
                          'correlated. Noise does not change comparison budgets.',
    }
    for game in games:
        binary=target/a.profile/game
        if not binary.is_file(): binary=binary.with_suffix('.exe')
        frames=[]; backend=None; dimensions=None; gpu_source=None; render_quality=None
        repetitions = []
        for repetition in range(a.runs):
            raw = raw_directory / f'{game}-{repetition}.json'
            env={**os.environ,'FERRAVEX_PERF_OUTPUT':str(raw),'FERRAVEX_PERF_WARMUP':str(a.warmup)}
            # Do not inherit an unrecorded quality override from the invoking shell.
            env.pop('FERRAVEX_QUALITY',None)
            env.pop('FERRAVEX_LOOK',None)
            # Preserve the historical 1:1 physical framebuffer workload after
            # native DPI support; desktop scaling must not alter this baseline.
            env['FERRAVEX_DPI_SCALE']='1'
            if a.quality != 'default': env['FERRAVEX_QUALITY']=a.quality
            count=str(a.frames+a.warmup)
            if game=='owned-lab': options=['--frames',count,'--output',str((a.output/'courtyard.png').resolve())]
            elif game=='owned-orbit': options=['--frames',count,'--autoplay']
            elif game=='strike': options=['--play','--smoke-frames',count]
            else: options=['--autoplay','--smoke-frames',count]
            run=subprocess.run([str(binary),*options],cwd=ROOT,env=env,capture_output=True,text=True,timeout=a.timeout)
            if run.returncode: raise ValueError(f'{game}: exit {run.returncode}: {run.stderr[-3000:]}')
            raw_bytes = raw.read_bytes()
            data=json.loads(raw_bytes)
            if data['schema']!=2 or data['dropped'] or len(data['frames'])!=a.frames or data['warmup']!=a.warmup:
                raise ValueError(f'{game}: incomplete performance recording')
            current_dimensions=[data['width'],data['height']]
            if backend is not None and (backend!=data['backend'] or dimensions!=current_dimensions or gpu_source!=data['gpu_source'] or render_quality!=data['render_quality']):
                raise ValueError(f'{game}: renderer/dimensions changed between repetitions')
            repetitions.append({
                'index': repetition, 'raw_file': str(raw),
                'raw_sha256': hashlib.sha256(raw_bytes).hexdigest(),
                'frames': len(data['frames']), 'metrics': summarize(data['frames']),
            })
            frames.extend(data['frames']);backend=data['backend'];dimensions=current_dimensions;gpu_source=data['gpu_source'];render_quality=data['render_quality']
        sizes={}
        artifacts={'native':binary,'wasm':target/'wasm32-unknown-unknown'/a.profile/(game.replace('-','_')+'.wasm')}
        for name,path in artifacts.items():
            if path.is_file():
                payload=path.read_bytes();sizes[name+'_bytes']=len(payload);sizes[name+'_gzip9_bytes']=len(gzip.compress(payload,mtime=0))
        summary=summarize(frames)
        report['games'][game]={'backend':backend,'dimensions':dimensions,'metrics':summary,'sizes':sizes,
            'binary_sha256':hashlib.sha256(binary.read_bytes()).hexdigest(),'gpu_source':gpu_source,'render_quality':render_quality,
            'repetitions': repetitions, 'repetition_noise': _repetition_noise(repetitions)}
        print(f'{game}: CPU p95 {summary["cpu_us"]["p95"]:.1f} µs; {len(frames)} samples',flush=True)
    comparison = compare_reports(report, baseline, a.time_tolerance, a.time_floor_us,
                                 a.size_tolerance, a.gpu_time_floor_us,
                                 min_samples=a.min_samples)
    comparison.update({
        'baseline': str(a.baseline) if a.baseline else None,
        'baseline_sha256': baseline_sha256,
        'baseline_revision': baseline.get('revision') if baseline is not None else None,
    })
    report['comparison'] = comparison
    serialized = json.dumps(report, indent=2, allow_nan=False) + '\n'
    (a.output/'report.json').write_text(serialized)
    if a.write_baseline:
        a.write_baseline.parent.mkdir(parents=True,exist_ok=True)
        a.write_baseline.write_text(serialized)
    for check in comparison['checks']:
        if check['status'] in ('regression', 'insufficient_samples', 'incompatible'):
            print(check['status'].upper() + ' ' + check['reason'], file=sys.stderr)
    print('Comparison: ' + comparison['status'])
    print(f'Report: {a.output/"report.json"}; GPU timings are driver queries where supported (see gpu_source and coverage).')
    return bool(comparison['failures'])


if __name__=='__main__':
    try: sys.exit(main())
    except (OSError,ValueError,subprocess.SubprocessError,KeyError) as e: sys.exit(str(e))
