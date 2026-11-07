#!/usr/bin/env python3
"""Install the real packaged release through HTTP, then exercise its runtimes."""
import argparse
import functools
import hashlib
import http.server
import io
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import tarfile
import threading
import tomllib

ROOT = Path(__file__).resolve().parents[1]


class QuietHandler(http.server.SimpleHTTPRequestHandler):
    def log_message(self, *_):
        pass


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--directory', type=Path, required=True)
    args = parser.parse_args()
    archives = list(args.directory.glob('ronchi-*.tar.gz'))
    if len(archives) != 1:
        raise SystemExit('Test exactly one native release archive per job')
    archive = archives[0]
    version = tomllib.loads((ROOT / 'Cargo.toml').read_text())['package']['version']
    with tempfile.TemporaryDirectory(prefix='ronchi-native-install-') as temporary:
        temporary = Path(temporary)
        mirror = temporary / 'mirror'
        endpoint = mirror / 'download' / f'v{version}'
        endpoint.mkdir(parents=True)
        for file in [archive, archive.with_name(archive.name + '.sha256')]:
            shutil.copyfile(file, endpoint / file.name)
        latest = mirror / 'latest/download'
        latest.mkdir(parents=True)
        (latest / 'release-version.txt').write_text(version + '\n')
        server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), functools.partial(QuietHandler, directory=mirror))
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()
        prefix = temporary / 'installation'
        env = {k: v for k, v in os.environ.items() if not k.startswith('RONCHI_')}
        env['RONCHI_RELEASE_BASE_URL'] = f'http://127.0.0.1:{server.server_port}'
        # Reject accidental use of a system Surfpool, including a different version.
        sentinel = temporary / 'unrelated-bin'
        sentinel.mkdir()
        (sentinel / 'surfpool').write_text('#!/bin/sh\nexit 99\n')
        (sentinel / 'surfpool').chmod(0o755)
        env['PATH'] = str(sentinel) + os.pathsep + env['PATH']
        try:
            command = ['sh', '-s', '--', '--prefix', str(prefix)]
            subprocess.run(command, input=(ROOT / 'install.sh').read_text(), env=env, text=True, check=True, timeout=120)
            binary = prefix / 'bin/ronchi'
            first = binary.resolve()
            subprocess.run(['python3', str(ROOT / 'scripts/smoke-installed.py'), str(binary)], env=env, check=True, timeout=180)
            subprocess.run(command, input=(ROOT / 'install.sh').read_text(), env=env, text=True, check=True, timeout=120)
            assert binary.resolve() == first
            # Model a rebuilt release with the same version while a real Surfpool
            # Lab owns the old executable. Only test-mirror package content changes.
            active = temporary / 'active-update-project'
            active.mkdir()
            def cli(*argv):
                value = subprocess.run([binary, '--project', active, '--json', *argv], env=env, text=True, capture_output=True, timeout=150)
                if value.returncode:
                    raise RuntimeError(value.stdout + value.stderr)
                return json.loads(value.stdout)['result']
            live = cli('up', '--detach', '--mode', 'surfpool', '--rpc-listen', '127.0.0.1:0', '--control-listen', '127.0.0.1:0')
            try:
                updated = temporary / 'updated.tar.gz'
                with tarfile.open(endpoint / archive.name) as source, tarfile.open(updated, 'w:gz', format=tarfile.USTAR_FORMAT) as dest:
                    for member in source:
                        if member.name.endswith('/LICENSE'):
                            data = source.extractfile(member).read() + b'\nRebuilt-package fixture.\n'
                            member.size = len(data)
                            dest.addfile(member, io.BytesIO(data))
                        elif member.isfile():
                            with source.extractfile(member) as stream:
                                dest.addfile(member, stream)
                        else:
                            dest.addfile(member)
                updated.replace(endpoint / archive.name)
                with (endpoint / archive.name).open('rb') as stream:
                    changed = hashlib.file_digest(stream, 'sha256').hexdigest()
                (endpoint / (archive.name + '.sha256')).write_text(changed + '\n')
                subprocess.run(command, input=(ROOT / 'install.sh').read_text(), env=env, text=True, check=True, timeout=120)
                assert binary.resolve() != first and first.exists()
                status = cli('--lab-id', live['lab_id'], 'status')
                assert status['state'] == 'ready'
                assert status['owned_children'] == live['owned_children']
            finally:
                cli('--lab-id', live['lab_id'], 'down')
            assert (active / 'ronchi/labs' / live['lab_id'] / 'journal.sqlite').is_file()
            assert (sentinel / 'surfpool').read_text() == '#!/bin/sh\nexit 99\n'
            with binary.open('rb') as stream:
                digest = hashlib.file_digest(stream, 'sha256').hexdigest()
            print(json.dumps({'native_release': archive.name, 'installer': 'passed', 'reinstall': 'passed', 'live_surfpool_update': 'passed', 'system_surfpool_ignored': True, 'ronchi_sha256': digest}))
        finally:
            server.shutdown()
            server.server_close()
            thread.join()


if __name__ == '__main__':
    main()
