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
SEED = 128


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def run(binary, *args):
    result = subprocess.run([str(binary), *map(str, args)], check=True, capture_output=True, text=True, timeout=90)
    return json.loads(result.stdout)


def summary(values):
    values = sorted(values)
    return {'samples': len(values), 'p50_ms': values[len(values)//2] if len(values)%2 else (values[len(values)//2-1]+values[len(values)//2])/2,
            'p95_ms': values[math.ceil(len(values)*.95)-1]}


def tail_summary(values):
    result = summary(values)
    ordered = sorted(values)
    result.update({'p99_ms': ordered[math.ceil(len(ordered)*.99)-1], 'max_ms': ordered[-1]})
    return result


def wait_for(paths, children, message):
    deadline = time.monotonic()+30
    while not all(path.exists() for path in paths):
        assert all(p.poll() is None for p in children), f'{message}: child exited'
        assert time.monotonic()<deadline, f'{message}: timeout'
        time.sleep(.005)


def existing_quality_tail(path):
    source = json.loads(path.read_text())
    cases = []
    for row in source['results']:
        if row['database'] != 'quality':
            continue
        writes = [value for client in row['raw_clients'] for value in client['write_ms']]
        reads = [value for client in row['raw_clients'] for value in client['read_ms']]
        periodic = [
            {'client': client['client'], 'write_number': write_number,
             'duration_ms': client['write_ms'][write_number-1]}
            for client in row['raw_clients']
            for write_number in (16, 32)
        ]
        flushes = [client['durable_tail_ms'] for client in row['raw_clients']]
        cases.append({
            'clients': row['clients'],
            'write': tail_summary(writes),
            'read': tail_summary(reads),
            'periodic_full_write_samples': periodic,
            'durable_tail_flush_ms': flushes,
            'durable_tail_flush': tail_summary(flushes),
        })
    return {
        'source': str(path.relative_to(ROOT)),
        'source_sha256': sha(path),
        'samples_reused': True,
        'new_measurement': False,
        'periodic_full_cadence': 'write numbers 16 and 32 for each fresh writer',
        'cases': cases,
    }


def checkpoint_cases(args, binary):
    rows = []
    for clients in [1, 2]:
        case = args.scratch/f'kernel-checkpoint-{clients}'
        case.mkdir()
        store = case/'store'
        store.mkdir()
        prepared = run(binary, 'prepare', 'kernel', store)
        reader_out = (case/'reader.out').open('w')
        reader_err = (case/'reader.err').open('w')
        reader = subprocess.Popen(
            [str(binary), 'checkpoint-reader', 'kernel', str(store), str(case)],
            stdout=reader_out, stderr=reader_err,
        )
        writers = []
        handles = [reader_out, reader_err]
        try:
            wait_for([case/'reader-ready'], [reader], 'pinned reader start')
            for index in range(clients):
                out = (case/f'client-{index}.out').open('w')
                err = (case/f'client-{index}.err').open('w')
                handles.extend([out, err])
                writers.append(subprocess.Popen([
                    str(binary), 'checkpoint-client', 'kernel', str(store), str(index),
                    str(args.samples), str(args.checkpoint_payload_bytes), str(case),
                ], stdout=out, stderr=err))
            wait_for(
                [case/f'ready-{index}' for index in range(clients)],
                [reader, *writers],
                'checkpoint writers start',
            )
            (case/'go').touch()
            for process in writers:
                assert process.wait(timeout=90) == 0, 'checkpoint writer failed'
            passive = run(binary, 'checkpoint', 'kernel', store, 'PASSIVE')
            (case/'reader-release').touch()
            wait_for([case/'reader-released'], [reader], 'pinned reader release')
            full = run(binary, 'checkpoint', 'kernel', store, 'FULL')
            (case/'reader-done').touch()
            assert reader.wait(timeout=30) == 0, 'pinned reader failed'
        finally:
            for process in [reader, *writers]:
                if process.poll() is None:
                    process.kill()
                    process.wait()
            for handle in handles:
                handle.close()
        writer_rows = [json.loads((case/f'client-{index}.out').read_text()) for index in range(clients)]
        reader_row = json.loads((case/'reader.out').read_text())
        verification = run(binary, 'verify', 'kernel', store, clients, args.samples)
        page_size = passive['page_size_bytes']
        frame_bytes = page_size + 24
        threshold = passive['wal_autocheckpoint_pages']
        threshold_eligible = []
        for writer in writer_rows:
            writer['wal_frames_after_each_write'] = [
                0 if value < 32 else (value-32)//frame_bytes
                for value in writer['wal_bytes_after_each_write']
            ]
            threshold_eligible.extend(
                duration for duration, frames in zip(writer['write_ms'], writer['wal_frames_after_each_write'])
                if frames >= threshold
            )
        all_writes = [value for writer in writer_rows for value in writer['write_ms']]
        assert passive['wal_frames_before'] > threshold, 'workload did not cross autocheckpoint threshold'
        assert passive['log_frames'] == passive['wal_frames_before'], 'PASSIVE reports the observed WAL'
        assert passive['checkpointed_frames'] < passive['log_frames'], 'pinned reader did not constrain PASSIVE'
        assert full['log_frames'] == passive['log_frames'], 'FULL did not observe the pinned WAL frames'
        assert full['busy'] == 0 and full['checkpointed_frames'] == full['log_frames'], 'FULL did not finish after release'
        assert reader_row['old_snapshot_preserved']
        assert reader_row['snapshot_released_before_full']
        assert reader_row['rows_after_full'] == SEED + clients*args.samples
        assert verification['every_client_acknowledgement_preserved']
        case_bytes = sum(path.stat().st_size for path in store.rglob('*') if path.is_file())
        assert case_bytes < 30*1024*1024, 'checkpoint fixture exceeds 30 MiB'
        rows.append({
            'database': 'kernel',
            'clients': clients,
            'prepared': prepared,
            'raw_clients': writer_rows,
            'writer_latency': tail_summary(all_writes),
            'threshold_eligible_writer_latency': tail_summary(threshold_eligible),
            'automatic_checkpoint_inference': {
                'threshold_pages': threshold,
                'eligible_write_observations': len(threshold_eligible),
                'basis': 'post-commit WAL frames at or above the connection-local threshold; no hook directly observed an automatic checkpoint call',
            },
            'pinned_reader': reader_row,
            'passive_while_reader_pinned': passive,
            'full_after_reader_release': full,
            'verification_after_reopen': verification,
            'peak_rss_kib_per_writer': [writer['peak_rss_kib'] for writer in writer_rows],
            'max_observed_wal_bytes': max(
                value for writer in writer_rows for value in writer['wal_bytes_after_each_write']
            ),
            'fixture_bytes_after_full_checkpoint': case_bytes,
        })
    return rows


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--scratch', required=True, type=Path)
    parser.add_argument('--output', required=True, type=Path)
    parser.add_argument('--samples', type=int, default=40)
    parser.add_argument('--build-only', action='store_true')
    parser.add_argument('--runner-binary', type=Path)
    parser.add_argument('--checkpoint-only', action='store_true')
    parser.add_argument('--checkpoint-payload-bytes', type=int, default=131072)
    parser.add_argument('--clients-input', type=Path, default=ROOT/'artifacts/performance-772/clients.json')
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
        if args.checkpoint_only:
            rows = checkpoint_cases(args, binary)
            result = {
                'started_utc': started_utc,
                'completed_utc': time.strftime('%Y-%m-%dT%H:%M:%SZ',time.gmtime()),
                'commit': subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip(),
                'binary_sha256': sha(binary),
                'runner_sha256': sha(Path(__file__)),
                'rust_source_sha256': sha(SOURCE),
                'production_kernel_sqlite_sha256': sha(
                    ROOT/'crates/kmp-adapter-embedded/src/adapter/engine/sqlite.rs'
                ),
                'profile': 'dev/sqlite, inherited workspace Cargo configuration',
                'samples_per_writer': args.samples,
                'payload_bytes_per_write': args.checkpoint_payload_bytes,
                'platform': platform.platform(),
                'rustc': subprocess.check_output(['rustc','-Vv'],text=True),
                'lscpu': json.loads(subprocess.check_output(['lscpu','--json'],text=True)),
                'checkpoint_scope': 'one or two independent production-adapter writers with one independent production snapshot reader pinned before their common start',
                'automatic_checkpoint_scope': 'threshold eligibility is inferred from WAL frames after acknowledged commits; PASSIVE and FULL rows are explicit diagnostic checkpoint observations',
                'cache': 'prepared persistent store, fresh worker processes, OS cache not flushed; reader pins before writer processes open',
                'quality_existing_control': existing_quality_tail(args.clients_input.resolve()),
                'kernel_checkpoint_results': rows,
            }
            args.output.parent.mkdir(parents=True,exist_ok=True)
            args.output.write_text(json.dumps(result,indent=2)+'\n')
            print(json.dumps([{k:v for k,v in row.items() if k!='raw_clients'} for row in rows]))
            return
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
