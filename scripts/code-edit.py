#!/usr/bin/env python3
"""CAS edits for the compiled game example, with recoverable receipts and isolated checks.

Only SDK src/*.rs files can change. The request is data; it cannot choose commands.
--check uses Apple's sandbox with network denied and a 90-second process-group deadline.
Other platforms can prepare/apply data edits; checking requires an external isolated runner.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import signal
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[1]
LIMIT = 1_048_576

def digest(text):
    return hashlib.sha256(text.encode()).hexdigest()

def bounded(path, limit=LIMIT):
    if path.is_symlink() or not path.is_file():
        raise ValueError(f'expected a regular file: {path}')
    with path.open('rb') as source:
        data = source.read(limit + 1)
    if len(data) > limit:
        raise ValueError('file exceeds size limit')
    return data.decode('utf-8')

def write_new(path, text):
    with path.open('x', encoding='utf-8') as out:
        out.write(text)
        out.flush()
        os.fsync(out.fileno())

def replace(path, text):
    temp = path.with_name(path.name + '.tomo-tmp')
    write_new(temp, text)
    os.replace(temp, path)

def checked(candidate, relative):
    if sys.platform != 'darwin' or not Path('/usr/bin/sandbox-exec').is_file():
        raise ValueError('isolated compiler check unavailable on this host; use a configured sandbox runner')
    with tempfile.TemporaryDirectory(prefix='sdk-check-', dir=ROOT / 'target') as name:
        trial = Path(name)
        for root in ['crates', 'tools', 'examples']:
            for path in (ROOT / root).rglob('*'):
                if path.is_file() and not path.is_symlink() and (path.suffix in {'.rs', '.toml', '.json', '.wgsl'}):
                    if path.name == 'progress.tomo.json' or '.tomo-journal' in path.parts:
                        continue
                    dest = trial / path.relative_to(ROOT)
                    dest.parent.mkdir(parents=True, exist_ok=True)
                    dest.write_text(bounded(path), encoding='utf-8')
        for name in ['Cargo.toml', 'Cargo.lock', 'rust-toolchain.toml']:
            shutil.copy2(ROOT / name, trial / name)
        (trial / relative).write_text(candidate, encoding='utf-8')
        cache = ROOT / 'target/code-check'
        cache.mkdir(exist_ok=True)
        home = Path.home()
        read_paths = ['/usr', '/bin', '/sbin', '/System', '/Library', '/Applications/Xcode.app', '/opt/homebrew', str(home / '.rustup'), str(home / '.cargo'), str(ROOT), '/private/tmp', '/private/var/folders', '/private/preboot', '/private/etc/ssl', '/dev']
        quote = lambda p: json.dumps(str(p))
        profile = '(version 1) (deny default) (allow process*) (allow sysctl-read) (allow mach-lookup) (allow signal) (allow file-read-metadata) (allow file-read-data (literal \"/\"))\n'
        profile += '(allow file-read* ' + ' '.join('(subpath ' + quote(p) + ')' for p in read_paths) + ')\n'
        profile += '(deny file-read* (literal ' + quote(home / '.cargo/credentials.toml') + '))\n'
        profile += '(allow file-write* ' + ' '.join('(subpath ' + quote(p) + ')' for p in [trial, cache, home / '.cargo', '/private/tmp', '/private/var/folders', '/private/preboot', '/private/etc/ssl', '/dev']) + ')\n'
        env = {k: v for k, v in os.environ.items() if k in {'PATH', 'HOME', 'TMPDIR', 'DEVELOPER_DIR', 'SDKROOT'}}
        env.update(CARGO_TARGET_DIR=str(cache), CARGO_INCREMENTAL='0', CARGO_PROFILE_DEV_DEBUG='0')
        command = ['/usr/bin/sandbox-exec', '-p', profile, 'cargo', '+1.85.0', 'check', '-p', 'tomo-sdk-demo', '--locked', '--offline']
        log = trial / 'check.log'
        with log.open('w') as output:
            child = subprocess.Popen(command, cwd=trial, env=env, stdout=output, stderr=subprocess.STDOUT, start_new_session=True)
            try:
                code = child.wait(timeout=90)
            except subprocess.TimeoutExpired:
                os.killpg(child.pid, signal.SIGKILL)
                child.wait()
                raise ValueError('compiler check exceeded 90 seconds')
        with log.open() as source:
            diagnostic = source.read(16384)
        if code:
            raise ValueError(f'compiler check exited {code}; source preserved: ' + diagnostic)
        return {'passed': True, 'command': command[3:], 'sandbox': 'macOS network-denied', 'log': diagnostic}

class JsonParser(argparse.ArgumentParser):
    def error(self, message):
        raise ValueError(message)


def main():
    parser = JsonParser()
    parser.add_argument('request', type=Path, nargs='?')
    parser.add_argument('--rollback', help='original applied request ID')
    parser.add_argument('--request-id', help='new ID for rollback receipt')
    parser.add_argument('--check', action='store_true')
    args = parser.parse_args()
    if args.rollback:
        if args.request or not args.request_id or not re.fullmatch(r'[A-Za-z0-9_-]{1,64}', args.rollback):
            raise ValueError('rollback requires an original ID and --request-id NEW')
        old = json.loads(bounded(ROOT / '.tomo-code-journal' / (args.rollback + '.json'), 2 * LIMIT))
        if old['schema_version'] != 1 or old['phase'] != 'applied':
            raise ValueError('only applied edits can be rolled back')
        request = dict(schema_version=1, request_id=args.request_id, path=old['request']['path'], expected_sha256=digest(old['request']['content']), content=old['before'])
    elif args.request:
        if args.request_id:
            raise ValueError('--request-id is only for rollback')
        request = json.loads(bounded(args.request))
    else:
        raise ValueError('provide a request file or --rollback ID --request-id NEW')
    if set(request) != {'schema_version', 'request_id', 'path', 'expected_sha256', 'content'} or type(request['schema_version']) is not int or request['schema_version'] != 1:
        raise ValueError('expected a strict code edit v1 request')
    ident = request['request_id']
    if not isinstance(ident, str) or not re.fullmatch(r'[A-Za-z0-9_-]{1,64}', ident):
        raise ValueError('invalid request ID')
    if not isinstance(request['expected_sha256'], str) or not re.fullmatch(r'[0-9a-f]{64}', request['expected_sha256']):
        raise ValueError('expected_sha256 must contain 64 lowercase hex digits')
    relative = Path(request['path'])
    source_root = ROOT / 'examples/rust_game/src'
    path = ROOT / relative
    if relative.is_absolute() or '..' in relative.parts or path.suffix != '.rs' or path.is_symlink() or not path.resolve().is_relative_to(source_root.resolve()):
        raise ValueError('only existing examples/rust_game/src/*.rs sources can be edited')
    if not isinstance(request['content'], str) or len(request['content'].encode()) > 262144:
        raise ValueError('source must be UTF-8 text <=256 KiB')
    journal = ROOT / '.tomo-code-journal'
    journal.mkdir(exist_ok=True)
    if journal.is_symlink():
        raise ValueError('journal cannot be a symlink')
    lock = journal / 'writer.lock'
    write_new(lock, ident)
    try:
        receipt = journal / (ident + '.json')
        current = bounded(path)
        replayed = receipt.exists()
        if replayed:
            record = json.loads(bounded(receipt, 2 * LIMIT))
            if record['request'] != request:
                raise ValueError('request ID already has different content')
            if record['phase'] == 'applied' or current == request['content']:
                if record['phase'] != 'applied':
                    record['phase'] = 'applied'
                    replace(receipt, json.dumps(record, indent=2))
                print(json.dumps({'schema_version': 1, 'ok': True, 'replayed': True, 'sha256': digest(current), 'receipt': str(receipt)}))
                return
        else:
            if len(list(journal.glob('*.json'))) >= 32:
                raise ValueError('journal full; archive completed receipts first')
            record = {'schema_version': 1, 'phase': 'prepared', 'request': request, 'before': current}
        if digest(current) != request['expected_sha256']:
            raise ValueError('source hash conflict; reread the current file')
        check = checked(request['content'], relative) if args.check else {'passed': None, 'reason': 'data edit only; no code executed'}
        record['check'] = check
        if not replayed:
            write_new(receipt, json.dumps(record, indent=2))
        if bounded(path) != current:
            raise ValueError('source changed during validation')
        replace(path, request['content'])
        record['phase'] = 'applied'
        replace(receipt, json.dumps(record, indent=2))
        print(json.dumps({'schema_version': 1, 'ok': True, 'replayed': replayed, 'sha256': digest(request['content']), 'receipt': str(receipt), 'check': check}))
    finally:
        lock.unlink()

if __name__ == '__main__':
    try:
        main()
    except (ValueError, OSError, KeyError, TypeError, subprocess.SubprocessError) as error:
        print(json.dumps({'schema_version': 1, 'ok': False, 'error': str(error)}))
        sys.exit(2)
