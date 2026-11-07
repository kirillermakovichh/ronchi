#!/usr/bin/env python3
"""Create a verified native Ronchi distribution. Python is a release-only tool."""
import argparse
from contextlib import nullcontext
import gzip
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import struct
import subprocess
import tarfile
import tempfile
import tomllib
import urllib.request

ROOT = Path(__file__).resolve().parents[1]
CATALOG = json.loads((ROOT / 'release/surfpool.json').read_text())


def sha256(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def fetch(url, path):
    request = urllib.request.Request(url, headers={'User-Agent': 'ronchi-release'})
    with urllib.request.urlopen(request, timeout=120) as source, path.open('xb') as dest:
        shutil.copyfileobj(source, dest)


def verify_architecture(path, target):
    with path.open('rb') as stream:
        header = stream.read(32)
    if target.endswith('apple-darwin'):
        cpu = 0x0100000c if target.startswith('aarch64') else 0x01000007
        valid = len(header) >= 8 and header[:4] == b'\xcf\xfa\xed\xfe' and struct.unpack_from('<I', header, 4)[0] == cpu
    else:
        valid = len(header) >= 20 and header[:6] == b'\x7fELF\x02\x01' and struct.unpack_from('<H', header, 18)[0] == 62
    if not valid:
        raise SystemExit(f'Executable architecture does not match {target}: {path.name}')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--target', required=True, choices=[a['target'] for a in CATALOG['artifacts']])
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--surfpool-archive', type=Path)
    parser.add_argument('--notices', type=Path, required=True)
    parser.add_argument('--surfpool-notices', type=Path, required=True)
    parser.add_argument('--output', type=Path, default=ROOT / 'target/distribution')
    args = parser.parse_args()
    version = tomllib.loads((ROOT / 'Cargo.toml').read_text())['package']['version']
    if not re.fullmatch(r'\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?', version):
        raise SystemExit('Unsupported release version')
    artifact = next(a for a in CATALOG['artifacts'] if a['target'] == args.target)
    installer = (ROOT / 'install.sh').read_text()
    if f"surfpool_version={CATALOG['version']}\n" not in installer:
        raise SystemExit('Installer Surfpool pin must match release/surfpool.json')
    name = f'ronchi-{version}-{args.target}'
    args.output.mkdir(parents=True, exist_ok=True)
    archive = args.output / f'{name}.tar.gz'
    with tempfile.TemporaryDirectory(prefix='ronchi-package-') as temporary:
        temporary = Path(temporary)
        upstream = args.surfpool_archive
        if upstream is None:
            upstream = temporary / artifact['archive']
            fetch(f"https://github.com/{CATALOG['repository']}/releases/download/v{CATALOG['version']}/{artifact['archive']}", upstream)
        if sha256(upstream) != artifact['sha256']:
            raise SystemExit('Surfpool archive does not match the pinned official SHA-256')
        bundle = temporary / name
        tool = bundle / 'libexec/surfpool' / CATALOG['version']
        tool.mkdir(parents=True)
        with tarfile.open(upstream, 'r:gz') as source:
            entries = source.getmembers()
            files = [m for m in entries if m.name == 'surfpool']
            if len(files) != 1 or not files[0].isfile() or files[0].size > 128 * 1024 * 1024:
                raise SystemExit('Official archive must contain one bounded regular surfpool executable')
            with source.extractfile(files[0]) as stream, (tool / 'surfpool').open('xb') as output:
                shutil.copyfileobj(stream, output)
        shutil.copyfile(upstream, tool / 'artifact.tar.gz')
        (bundle / 'bin').mkdir()
        shutil.copyfile(args.binary, bundle / 'bin/ronchi')
        for executable in [bundle / 'bin/ronchi', tool / 'surfpool']:
            verify_architecture(executable, args.target)
            executable.chmod(0o755)
        # Packaging is native: neither cross-compilation nor file names prove startup.
        actual = subprocess.check_output([str(bundle / 'bin/ronchi'), '--version'], text=True, timeout=10).strip()
        surfpool_version = subprocess.check_output([str(tool / 'surfpool'), '--version'], text=True, timeout=10).split()
        if actual != f'ronchi {version}' or CATALOG['version'] not in surfpool_version:
            raise SystemExit('Executable version mismatch')
        shutil.copyfile(ROOT / 'LICENSE', bundle / 'LICENSE')
        (bundle / 'licenses').mkdir()
        shutil.copyfile(ROOT / 'release/SURFPOOL-LICENSE', bundle / 'licenses/SURFPOOL-LICENSE')
        for source, name_in_bundle in [(args.notices, 'THIRD-PARTY-LICENSES.html'), (args.surfpool_notices, 'SURFPOOL-THIRD-PARTY-LICENSES.html')]:
            if not source.is_file() or source.stat().st_size == 0:
                raise SystemExit(f'Missing dependency notices: {source}')
            shutil.copyfile(source, bundle / 'licenses' / name_in_bundle)
        covered_sources = args.surfpool_notices.parent / 'SURFPOOL-SOURCES.tar.gz'
        if not covered_sources.is_file() or covered_sources.stat().st_size == 0:
            raise SystemExit('Missing Surfpool covered dependency sources; run release-notices.py')
        shutil.copyfile(covered_sources, bundle / 'licenses/SURFPOOL-SOURCES.tar.gz')
        (bundle / 'release.json').write_text(json.dumps({'version': version, 'target': args.target, 'surfpool_version': CATALOG['version'], 'surfpool_archive_sha256': artifact['sha256']}, indent=2) + '\n')
        # Fixed headers and ordering make re-packaging identical inputs reproducible.
        pending = args.output / f'.{archive.name}.{os.getpid()}'
        try:
            with pending.open('xb') as raw, gzip.GzipFile(filename='', mode='wb', fileobj=raw, mtime=0) as compressed, tarfile.open(fileobj=compressed, mode='w', format=tarfile.USTAR_FORMAT) as tar:
                for path in [bundle, *sorted(bundle.rglob('*'))]:
                    info = tar.gettarinfo(str(path), str(path.relative_to(temporary)))
                    info.uid = info.gid = info.mtime = 0
                    info.uname = info.gname = ''
                    info.mode = 0o755 if path.is_dir() or path.name in ['ronchi', 'surfpool'] else 0o644
                    with path.open('rb') if path.is_file() else nullcontext() as stream:
                        tar.addfile(info, stream)
            pending.replace(archive)
        finally:
            pending.unlink(missing_ok=True)
    (args.output / f'{archive.name}.sha256').write_text(sha256(archive) + '\n')
    (args.output / 'release-version.txt').write_text(version + '\n')
    shutil.copyfile(ROOT / 'install.sh', args.output / 'install.sh')
    print(archive)


if __name__ == '__main__':
    main()
