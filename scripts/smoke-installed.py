#!/usr/bin/env python3
"""Verify installed native binaries without the source checkout or global Surfpool."""
import argparse
import json
import os
from pathlib import Path
import subprocess
import tempfile
import urllib.request


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('binary', type=Path)
    args = parser.parse_args()
    binary = args.binary.absolute()
    env = {k: v for k, v in os.environ.items() if not k.startswith('RONCHI_')}
    assert subprocess.check_output([binary, '--version'], text=True, env=env).startswith('ronchi ')
    installation = binary.resolve().parent.parent
    projects = []
    with tempfile.TemporaryDirectory(prefix='ronchi-installed-smoke-') as temporary:
        root = Path(temporary)
        for index, mode in enumerate(['simulator', 'surfpool', 'surfpool']):
            project = root / f'project-{index}'
            project.mkdir()
            def cli(*argv, check=True):
                process = subprocess.run([binary, '--project', project, '--json', *argv], cwd=project, text=True, capture_output=True, env=env, timeout=150)
                value = json.loads(process.stdout)
                if check and process.returncode:
                    raise RuntimeError(value)
                return process, value
            lab_id = None
            try:
                _, value = cli('up', '--detach', '--mode', mode, '--rpc-listen', '127.0.0.1:0', '--control-listen', '127.0.0.1:0')
                lab = value['result']; lab_id = lab['lab_id']
                assert lab['configuration']['mode'] == mode
                if mode == 'surfpool':
                    child = lab['owned_children'][0]
                    assert Path(child['executable']).resolve().is_relative_to(installation), child
                    assert not (project / 'ronchi/tools').exists()
                else:
                    assert not lab['owned_children']
                launch = cli('--lab-id', lab_id, 'dashboard')[1]['result']['launch_url']
                with urllib.request.urlopen(launch.split('#')[0], timeout=10) as response:
                    assert b'<html' in response.read().lower()
                request = urllib.request.Request('http://' + lab['rpc_address'], data=json.dumps({'jsonrpc': '2.0', 'id': 1, 'method': 'getLatestBlockhash', 'params': []}).encode(), headers={'Content-Type': 'application/json'})
                with urllib.request.urlopen(request, timeout=10) as response:
                    assert json.load(response)['result']['value']['blockhash']
            finally:
                if lab_id:
                    cli('--lab-id', lab_id, 'down')
                    journal = project / 'ronchi/labs' / lab_id / 'journal.sqlite'
                    assert journal.is_file()
                    projects.append(str(journal.relative_to(root)))
        # A corrupt bundled tool must fail before allocating a new Lab, even
        # though source-build/project-local fallback is still supported.
        tool = next(installation.glob('libexec/surfpool/*/surfpool'))
        original = tool.read_bytes()
        try:
            tool.write_bytes(b'not a valid Surfpool')
            project = root / 'tamper'
            project.mkdir()
            result = subprocess.run([binary, '--project', project, '--json', 'up', '--detach', '--mode', 'surfpool', '--rpc-listen', '127.0.0.1:0', '--control-listen', '127.0.0.1:0'], cwd=project, env=env, text=True, capture_output=True, timeout=20)
            value = json.loads(result.stdout)
            assert result.returncode != 0 and value['error']['code'] == 'state_conflict', value
            assert not list((project / 'ronchi/labs').glob('*/manifest.json'))
        finally:
            tool.write_bytes(original)
    print(json.dumps({'installed_modes': ['simulator', 'surfpool', 'surfpool'], 'independent_projects': 3, 'dashboard': 'embedded', 'tamper_rejected': True, 'journals': projects}))


if __name__ == '__main__':
    main()
