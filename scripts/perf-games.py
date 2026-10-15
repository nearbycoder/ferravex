#!/usr/bin/env python3
"""Local performance CI: deterministic native gameplay, raw samples and explicit baseline budgets."""
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

ROOT = Path(__file__).resolve().parent.parent
TIMINGS = ('cpu_us', 'submit_us', 'software_render_us', 'gpu_us')
WORK = ('draw_calls', 'vertices', 'upload_bytes', 'allocation_calls', 'allocation_bytes',
        'physics_steps', 'physics_pairs', 'contact_points', 'ccd_impacts')


def summarize(frames):
    if not frames:
        raise ValueError('recording has no measured frames')
    out = {}
    for key in (*TIMINGS, *WORK):
        values = [f[key] for f in frames if f.get(key) is not None]
        if not values:
            out[key] = None
            continue
        if (key != 'gpu_us' and len(values) != len(frames)) or any(not isinstance(v, (float, int)) or not math.isfinite(v) or v < 0 for v in values):
            raise ValueError(f'invalid or incomplete metric {key}')
        values.sort()
        out[key] = {'samples':len(values), 'coverage':len(values)/len(frames), 'mean': statistics.mean(values), 'p50': values[math.ceil(len(values)*.5)-1],
                    'p95': values[math.ceil(len(values)*.95)-1], 'p99': values[math.ceil(len(values)*.99)-1], 'max': values[-1]}
    return out


def compare(current, baseline, time_tolerance=.2, time_floor=200., size_tolerance=.05, gpu_floor=20.):
    failures = []
    for key in ('schema', 'machine', 'config', 'rustc'):
        if current.get(key) != baseline.get(key):
            return [f'baseline {key} differs; record a baseline for this machine/configuration']
    if set(current['games']) != set(baseline['games']):
        return ['baseline game coverage differs']
    for game, row in current['games'].items():
        old = baseline['games'][game]
        if any(row.get(key) != old.get(key) for key in ('backend','dimensions','gpu_source','render_quality')):
            failures.append(f'{game}: renderer, dimensions, GPU timer or quality differ')
            continue
        for metric in (*TIMINGS, *WORK):
            a, b = row['metrics'][metric], old['metrics'][metric]
            if (a is None) != (b is None):
                failures.append(f'{game}/{metric}: measurement availability changed')
                continue
            if a is None:
                continue
            if metric == 'gpu_us' and a['coverage'] < max(.5, b['coverage']-.1):
                failures.append(f'{game}/{metric}: insufficient GPU query coverage')
            # Compare both typical cost and tails. Integer work gets a small allowance for
            # allocator/driver bookkeeping, not the much larger CPU timer noise allowance.
            for quantile in ('mean', 'p95', 'p99'):
                floor = gpu_floor if metric == 'gpu_us' else time_floor
                tolerance = max(floor, b[quantile]*time_tolerance) if metric in TIMINGS else max(1., b[quantile]*.05)
                if a[quantile] > b[quantile]+tolerance:
                    failures.append(f'{game}/{metric}/{quantile}: {a[quantile]:.2f} > {b[quantile]+tolerance:.2f} (baseline {b[quantile]:.2f})')
        for artifact, value in row['sizes'].items():
            if artifact not in old['sizes']:
                failures.append(f'{game}/{artifact}: artifact coverage changed')
            elif value > old['sizes'][artifact]+max(1024,old['sizes'][artifact]*size_tolerance):
                failures.append(f'{game}/{artifact}: {value} bytes exceeds size budget')
        if set(old['sizes']) != set(row['sizes']):
            failures.append(f'{game}: artifact coverage differs')
    return failures


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
    mode.add_argument('--baseline', type=Path, help='compare and exit nonzero on regressions')
    mode.add_argument('--write-baseline', type=Path, help='explicitly create/replace an approved baseline')
    p.add_argument('--no-build', action='store_true', help='use existing binaries; caller is responsible for freshness')
    p.add_argument('--time-tolerance', type=float, default=.2)
    p.add_argument('--time-floor-us', type=float, default=200.)
    p.add_argument('--gpu-time-floor-us', type=float, default=20.)
    p.add_argument('--size-tolerance', type=float, default=.05)
    a=p.parse_args()
    if not 1 <= a.frames <= 8192 or not 0 <= a.warmup <= 10000 or not 1 <= a.runs <= 20 or a.timeout < 1:
        p.error('frames 1..8192, warmup 0..10000, runs 1..20, timeout > 0 required')
    if any(not math.isfinite(n) or n < 0 for n in (a.time_tolerance,a.time_floor_us,a.size_tolerance,a.gpu_time_floor_us)):
        p.error('budget tolerances must be finite and nonnegative')
    catalog=json.loads((ROOT/'examples/catalog.json').read_text())
    known={g['id'] for g in catalog}
    if a.game and set(a.game)-known:
        p.error('unknown games: '+', '.join(sorted(set(a.game)-known)))
    games=[g['id'] for g in catalog if not a.game or g['id'] in a.game]
    if not a.no_build:
        cmd=['cargo','build','--offline','--locked','--bins']
        if a.profile != 'debug': cmd += ['--profile',a.profile]
        for game in games: cmd += ['-p',game]
        subprocess.run(cmd,cwd=ROOT,check=True)
    metadata=json.loads(subprocess.check_output(['cargo','metadata','--no-deps','--format-version','1','--locked'],cwd=ROOT,text=True))
    target=Path(metadata['target_directory'])
    a.output.mkdir(parents=True,exist_ok=True)
    machine={'system':platform.system(),'release':platform.release(),'arch':platform.machine(),'node':platform.node()}
    report={'schema':2,'machine':machine,'config':{'profile':a.profile,'frames':a.frames,'warmup':a.warmup,'runs':a.runs,'workload':'catalog-autoplay-v1','quality':a.quality},
            'revision':subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip(),
            'rustc':subprocess.check_output(['rustc','--version'],text=True).strip(),'games':{}}
    for game in games:
        binary=target/a.profile/game
        if not binary.is_file(): binary=binary.with_suffix('.exe')
        frames=[]; backend=None; dimensions=None; gpu_source=None; render_quality=None
        for repetition in range(a.runs):
            raw=(a.output/f'{game}-{repetition}.json').resolve(); raw.unlink(missing_ok=True)
            env={**os.environ,'FERRAVEX_PERF_OUTPUT':str(raw),'FERRAVEX_PERF_WARMUP':str(a.warmup)}
            # Do not inherit an unrecorded quality override from the invoking shell.
            env.pop('FERRAVEX_QUALITY',None)
            if a.quality != 'default': env['FERRAVEX_QUALITY']=a.quality
            count=str(a.frames+a.warmup)
            if game=='owned-lab': options=['--frames',count,'--output',str((a.output/'courtyard.png').resolve())]
            elif game=='owned-orbit': options=['--frames',count,'--autoplay']
            elif game=='strike': options=['--play','--smoke-frames',count]
            else: options=['--autoplay','--smoke-frames',count]
            run=subprocess.run([str(binary),*options],cwd=ROOT,env=env,capture_output=True,text=True,timeout=a.timeout)
            if run.returncode: raise ValueError(f'{game}: exit {run.returncode}: {run.stderr[-3000:]}')
            data=json.loads(raw.read_text())
            if data['schema']!=2 or data['dropped'] or len(data['frames'])!=a.frames or data['warmup']!=a.warmup:
                raise ValueError(f'{game}: incomplete performance recording')
            current_dimensions=[data['width'],data['height']]
            if backend is not None and (backend!=data['backend'] or dimensions!=current_dimensions or gpu_source!=data['gpu_source'] or render_quality!=data['render_quality']):
                raise ValueError(f'{game}: renderer/dimensions changed between repetitions')
            frames.extend(data['frames']);backend=data['backend'];dimensions=current_dimensions;gpu_source=data['gpu_source'];render_quality=data['render_quality']
        sizes={}
        artifacts={'native':binary,'wasm':target/'wasm32-unknown-unknown'/a.profile/(game.replace('-','_')+'.wasm')}
        for name,path in artifacts.items():
            if path.is_file():
                payload=path.read_bytes();sizes[name+'_bytes']=len(payload);sizes[name+'_gzip9_bytes']=len(gzip.compress(payload,mtime=0))
        summary=summarize(frames)
        report['games'][game]={'backend':backend,'dimensions':dimensions,'metrics':summary,'sizes':sizes,
            'binary_sha256':hashlib.sha256(binary.read_bytes()).hexdigest(),'gpu_source':gpu_source,'render_quality':render_quality}
        print(f'{game}: CPU p95 {summary["cpu_us"]["p95"]:.1f} µs; {len(frames)} samples',flush=True)
    failures=compare(report,json.loads(a.baseline.read_text()),a.time_tolerance,a.time_floor_us,a.size_tolerance,a.gpu_time_floor_us) if a.baseline else []
    report['comparison']={'baseline':str(a.baseline) if a.baseline else None,'time_tolerance':a.time_tolerance,'time_floor_us':a.time_floor_us,'gpu_time_floor_us':a.gpu_time_floor_us,'size_tolerance':a.size_tolerance,'failures':failures}
    (a.output/'report.json').write_text(json.dumps(report,indent=2)+'\n')
    if a.write_baseline:
        a.write_baseline.parent.mkdir(parents=True,exist_ok=True)
        a.write_baseline.write_text(json.dumps(report,indent=2)+'\n')
    for failure in failures: print('REGRESSION '+failure,file=sys.stderr)
    print(f'Report: {a.output/"report.json"}; GPU timings are driver queries where supported (see gpu_source and coverage).')
    return bool(failures)


if __name__=='__main__':
    try: sys.exit(main())
    except (OSError,ValueError,subprocess.SubprocessError,KeyError) as e: sys.exit(str(e))
