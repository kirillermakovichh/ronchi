#!/usr/bin/env python3
"""Generate license notices for Ronchi and the pinned upstream Surfpool source."""
import argparse
import gzip
import tomllib
import hashlib
import html
import json
from pathlib import Path
import subprocess
import tarfile
import tempfile
import urllib.request

ROOT = Path(__file__).resolve().parents[1]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--target', required=True)
    parser.add_argument('--cargo-about', default='cargo-about')
    parser.add_argument('--output', type=Path, default=ROOT / 'target/distribution')
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    catalog = json.loads((ROOT / 'release/surfpool.json').read_text())
    common = [args.cargo_about, 'generate', '--locked', '--fail', '--target', args.target, '--config', str(ROOT / 'release/about.toml')]
    subprocess.run([*common, '--manifest-path', str(ROOT / 'Cargo.toml'), '--output-file', str(args.output / 'THIRD-PARTY-LICENSES.html'), str(ROOT / 'release/about.hbs')], check=True)
    append_dashboard_notices(args.output / 'THIRD-PARTY-LICENSES.html')
    with tempfile.TemporaryDirectory(prefix='ronchi-surfpool-licenses-') as temporary:
        temporary = Path(temporary)
        archive = temporary / 'source.tar.gz'
        url = f"https://codeload.github.com/{catalog['repository']}/tar.gz/refs/tags/v{catalog['version']}"
        with urllib.request.urlopen(url, timeout=120) as response, archive.open('wb') as dest:
            while data := response.read(65536):
                dest.write(data)
        with archive.open('rb') as stream:
            if hashlib.file_digest(stream, 'sha256').hexdigest() != catalog['source_sha256']:
                raise SystemExit('Pinned Surfpool source archive checksum mismatch')
        with tarfile.open(archive) as source:
            source.extractall(temporary, filter='data')
        manifest = temporary / f"surfpool-{catalog['version']}/crates/cli/Cargo.toml"
        subprocess.run([*common, '--manifest-path', str(manifest), '--output-file', str(args.output / 'SURFPOOL-THIRD-PARTY-LICENSES.html'), str(ROOT / 'release/about.hbs')], check=True)
        bundle_sources(temporary, manifest, archive, args.output / 'SURFPOOL-SOURCES.tar.gz')


def append_dashboard_notices(output):
    packages = json.loads((ROOT / 'dashboard/package-lock.json').read_text())['packages']
    sections = []
    for name, package in sorted(packages.items()):
        if not name or package.get('dev'):
            continue
        directory = ROOT / 'dashboard' / name
        installed = json.loads((directory / 'package.json').read_text())
        if installed['version'] != package['version']:
            raise SystemExit(f'Dashboard dependency differs from lockfile: {name}')
        licenses = [file for file in directory.iterdir() if file.is_file() and file.name.lower().startswith(('license', 'copying', 'notice'))]
        if not licenses:
            raise SystemExit(f'Missing dashboard dependency license: {name}')
        sections.append('<section><h2>' + html.escape(installed['name'] + ' ' + installed['version']) + '</h2>')
        for file in sorted(licenses):
            sections.append('<pre>' + html.escape(file.read_text()) + '</pre>')
        sections.append('</section>')
    text = output.read_text()
    output.write_text(text.replace('</body>', '<h1>Ronchi dashboard dependencies</h1>' + ''.join(sections) + '</body>'))


def bundle_sources(directory, manifest, archive, output):
    # Ship covered source files as well as license text for MPL dependencies.
    # Include exact Cargo.lock-verified crate archives, not moving repository links.
    metadata = json.loads(subprocess.check_output(['cargo', 'metadata', '--locked', '--offline', '--format-version', '1', '--manifest-path', str(manifest)], text=True))
    lock = tomllib.loads((manifest.parents[2] / 'Cargo.lock').read_text())
    checksums = {(p['name'], p['version']): p.get('checksum') for p in lock['package']}
    sources = [('surfpool-source.tar.gz', archive)]
    for package in metadata['packages']:
        if 'MPL-2.0' not in (package.get('license') or ''):
            continue
        source = Path(package['manifest_path']).parent
        crate = source.parent.parent.parent / 'cache' / source.parent.name / f"{package['name']}-{package['version']}.crate"
        with crate.open('rb') as stream:
            if hashlib.file_digest(stream, 'sha256').hexdigest() != checksums[(package['name'], package['version'])]:
                raise SystemExit(f'Covered dependency source checksum mismatch: {crate.name}')
        sources.append((crate.name, crate))
    with output.open('wb') as raw, gzip.GzipFile(filename='', mode='wb', fileobj=raw, mtime=0) as compressed, tarfile.open(fileobj=compressed, mode='w', format=tarfile.USTAR_FORMAT) as tar:
        for name, source in sources:
            info = tar.gettarinfo(str(source), name)
            info.uid = info.gid = info.mtime = 0
            info.uname = info.gname = ''
            info.mode = 0o644
            with source.open('rb') as stream:
                tar.addfile(info, stream)


if __name__ == '__main__':
    main()
