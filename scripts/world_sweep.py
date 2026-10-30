"""Run every campaign world headless through the port's start harness and
report each one's exit, last status and notable (stop/boundary/omission) lines.

    python scripts/world_sweep.py [--frames N] [--tag TAG]
        [--exe PATH] [--renderer opengl|software] [--baseline TAG] [WORLD ...]

Each world (default 0..24) starts the buggy at its first start
(`--start {"world": N, "form": 1}`), hidden at 320x240 with a 16 ms fixed
step, for N frames (18750 is five game minutes). Output goes to
`.tmp/allmaps/TAG/`: `outNN.txt`, `logsNN/` and `summary.json`, rewritten
after each world. The exe is copied there first, so cargo can rebuild
during the sweep. `--baseline TAG` prints, per world, the notable lines that
are new (+) or gone (-) against an earlier sweep; with no worlds run
(`--diff-only`) it only compares.
"""
import argparse
import json
import os
import re
import shutil
import subprocess
import time
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
SWEEPS = REPO / '.tmp' / 'allmaps'
ROUTINE = re.compile(
    r'^(World (projectile collision|weapon firing|dynamic object|object disposal|projectile damage|actor dying)'
    r'|\[status\]|\[phase\]|Loading: LoadingPresentationStats|Menu: )')
NOTABLE = re.compile(r'(?i)panic|error|stop|fail|boundary|not ported|unsupported|crash|thread|omitted')
# Numbers that differ between runs without meaning anything.
NOISE = re.compile(r'\b(clock|frame|tick) \d+|\d{3,}|ObjectId \{[^}]*\}')


def notable(lines):
    return [line for line in lines if NOTABLE.search(line) and not ROUTINE.match(line)]


def sweep(args, out_dir):
    out_dir.mkdir(parents=True, exist_ok=True)
    exe = out_dir / 'inf-game.exe'
    shutil.copy2(args.exe, exe)
    env_tmp = str(REPO / '.tmp')
    env = dict(os.environ, TMP=env_tmp, TEMP=env_tmp)
    summary = []
    for world in args.worlds or range(25):
        state = out_dir / f'w{world:02}.json'
        state.write_bytes(json.dumps({'world': world, 'form': 1}).encode('utf-8'))
        out = out_dir / f'out{world:02}.txt'
        command = [
            str(exe), '--data-root', str(REPO / 'retail'),
            '--settings', str(out_dir / 'settings.json'),
            '--profiles', str(out_dir / 'profiles.json'),
            '--start', str(state), '--hidden', '--renderer', args.renderer,
            '--width', '320', '--height', '240',
            '--fixed-step-micros', '16000', '--frames', str(args.frames),
            '--session-log', str(out_dir / f'logs{world:02}'),
        ]
        began = time.time()
        try:
            with out.open('wb') as sink:
                code = subprocess.run(command, stdout=sink, stderr=subprocess.STDOUT, env=env,
                                      timeout=1800).returncode
        except subprocess.TimeoutExpired:
            code = 'timeout'
        lines = out.read_bytes().decode('utf-8', 'replace').splitlines()
        status = [line for line in lines if line.startswith('[status]')]
        found = notable(lines)
        summary.append(dict(
            world=world, exit=code, seconds=round(time.time() - began, 1),
            gameplay=any('effective: Gameplay' in line for line in lines),
            presented=any(line.startswith('Presented') for line in lines),
            last_status=status[-1] if status else None,
            dying=sum(line.startswith('World actor dying') for line in lines),
            notable=found[:40], notable_count=len(found)))
        print(f'world {world:2}: exit {code} {summary[-1]["seconds"]:6.1f}s notable={len(found)}', flush=True)
        (out_dir / 'summary.json').write_bytes(json.dumps(summary, indent=1).encode('utf-8'))


def compare(out_dir, baseline):
    old_rows = {row['world']: row for row in json.loads((SWEEPS / baseline / 'summary.json').read_bytes())}
    for row in json.loads((out_dir / 'summary.json').read_bytes()):
        new = {NOISE.sub('#', line) for line in row['notable']}
        old = {NOISE.sub('#', line) for line in old_rows.get(row['world'], {}).get('notable', [])}
        print(f'world {row["world"]}: exit {row["exit"]}, {(row["last_status"] or "no status")[:70]}')
        for line in sorted(new - old):
            print('  + ' + line[:220])
        for line in sorted(old - new):
            print('  - ' + line[:220])


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('worlds', nargs='*', type=int)
    parser.add_argument('--frames', type=int, default=3600)
    parser.add_argument('--tag', default=time.strftime('%Y%m%d-%H%M%S'))
    parser.add_argument('--exe', default=str(REPO / 'target/release/inf-game.exe'))
    parser.add_argument('--renderer', default='opengl')
    parser.add_argument('--baseline')
    parser.add_argument('--diff-only', action='store_true')
    args = parser.parse_args()
    out_dir = SWEEPS / args.tag
    if not args.diff_only:
        sweep(args, out_dir)
    if args.baseline:
        compare(out_dir, args.baseline)


if __name__ == '__main__':
    main()
