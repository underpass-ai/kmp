#!/usr/bin/env python3
"""Supplement #772 with one/two native clients per SQLite database.

Build first with --build-only, then reserve a quiet slot. Seed/start barriers,
1-ms inter-operation pacing, RSS and WAL observations are outside latency.
The two clients use distinct logical roots in one database, exercising SQLite
writer serialization without changing expected-revision semantics.
"""
import argparse
import hashlib
import json
import math
from pathlib import Path
import platform
import shutil
import subprocess
import time

ROOT = Path(__file__).resolve().parents[2]
SOURCE = ROOT / 'scripts/performance/sqlite_clients.rs'


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def run(binary, *args):
    result = subprocess.run([str(binary), *map(str, args)], check=True, capture_output=True, text=True, timeout=90)
    return json.loads(result.stdout)


def summary(values):
    values = sorted(values)
    return {'samples': len(values), 'p50_ms': values[len(values)//2] if len(values)%2 else (values[len(values)//2-1]+values[len(values)//2])/2,
            'p95_ms': values[math.ceil(len(values)*.95)-1]}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--scratch', required=True, type=Path)
    parser.add_argument('--output', required=True, type=Path)
    parser.add_argument('--samples', type=int, default=40)
    parser.add_argument('--build-only', action='store_true')
    parser.add_argument('--runner-binary', type=Path)
    args = parser.parse_args()
    if args.samples < 32:
        parser.error('at least 32 samples exercise two periodic durable quality batches')
    binary = args.runner_binary.resolve() if args.runner_binary else ROOT/'target/debug/sqlite-clients-runner'
    if not args.runner_binary:
        destination = ROOT/'crates/kmp-adapter-embedded/src/bin/sqlite-clients-runner.rs'
        destination.parent.mkdir(parents=True, exist_ok=True)
        with destination.open('x') as file:
            file.write(SOURCE.read_text())
        try:
            subprocess.run(['cargo','build','--locked','-p','kmp-adapter-embedded','--bin','sqlite-clients-runner'],cwd=ROOT,check=True)
        finally:
            destination.unlink()
    if args.build_only:
        print(binary)
        return
    if args.output.exists():
        parser.error('output must be new')
    args.scratch.mkdir(parents=True,exist_ok=False)
    rows = []
    started_utc = time.strftime('%Y-%m-%dT%H:%M:%SZ',time.gmtime())
    try:
        for kind in ['kernel','quality']:
            for clients in [1,2]:
                case = args.scratch/f'{kind}-{clients}'
                case.mkdir()
                store = case/'store'
                store.mkdir()
                prepared = run(binary,'prepare',kind,store)
                children = []
                handles = []
                try:
                    for index in range(clients):
                        out = (case/f'client-{index}.out').open('w')
                        err = (case/f'client-{index}.err').open('w')
                        handles.extend([out,err])
                        children.append(subprocess.Popen([str(binary),'client',kind,str(store),str(index),str(args.samples),str(case)],stdout=out,stderr=err))
                    deadline = time.monotonic()+30
                    while not all((case/f'ready-{i}').exists() for i in range(clients)):
                        assert all(p.poll() is None for p in children), 'client failed before start'
                        assert time.monotonic()<deadline, 'start barrier timeout'
                        time.sleep(.005)
                    (case/'go').touch()
                    for p in children:
                        assert p.wait(timeout=90)==0, 'native client failed'
                finally:
                    for p in children:
                        if p.poll() is None:
                            p.kill()
                            p.wait()
                    for file in handles:
                        file.close()
                samples = [json.loads((case/f'client-{i}.out').read_text()) for i in range(clients)]
                verification = run(binary,'verify',kind,store,clients,args.samples)
                rows.append({'database':kind,'clients':clients,'prepared':prepared,'raw_clients':samples,'verification':verification,
                    'write':summary([v for c in samples for v in c['write_ms']]),
                    'read':summary([v for c in samples for v in c['read_ms']]),
                    'peak_rss_kib_per_client':[c['peak_rss_kib'] for c in samples],
                    'max_observed_wal_bytes':max(v for c in samples for v in c['wal_bytes_after_each_write'])})
        result = {'started_utc':started_utc,'completed_utc':time.strftime('%Y-%m-%dT%H:%M:%SZ',time.gmtime()),
            'commit':subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip(),
            'binary_sha256':sha(binary),'runner_sha256':sha(Path(__file__)),'rust_source_sha256':sha(SOURCE),
            'profile':'dev/sqlite, inherited workspace Cargo configuration','samples_per_client':args.samples,
            'platform':platform.platform(),'rustc':subprocess.check_output(['rustc','-Vv'],text=True),
            'lscpu':json.loads(subprocess.check_output(['lscpu','--json'],text=True)),
            'sample_scope':'40 operations per process by default; two client p50/p95 pool both processes. Latencies exclude fixture construction, barriers, pacing, WAL/RSS reads. Quality writes are one observation per batch; default FULL cadence is every 16 batches.',
            'memory_scope':'Linux native process lifetime VmHWM per client; not live heap or summed simultaneous peak',
            'concurrency':'one or two independent production-adapter client processes sharing a database; independent roots avoid semantic revision conflicts; unrelated campaign builds paused',
            'cache':'prepared persistent store, fresh worker processes, OS cache not flushed; each client opens before barrier',
            'results':rows}
        args.output.parent.mkdir(parents=True,exist_ok=True)
        args.output.write_text(json.dumps(result,indent=2)+'\n')
        print(json.dumps([{k:v for k,v in r.items() if k!='raw_clients'} for r in rows]))
    except BaseException:
        diagnostic = args.output.with_suffix('.diagnostics')
        diagnostic.mkdir(parents=True,exist_ok=False)
        for path in args.scratch.rglob('*'):
            if path.is_file() and path.suffix in ['.out','.err']:
                dest=diagnostic/path.relative_to(args.scratch)
                dest.parent.mkdir(parents=True,exist_ok=True)
                shutil.copy2(path,dest)
        raise
    finally:
        shutil.rmtree(args.scratch)


if __name__=='__main__':
    main()
