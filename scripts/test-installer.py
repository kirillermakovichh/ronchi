#!/usr/bin/env python3
"""Exercise the actual POSIX installer against disposable local HTTP releases."""
import functools
import hashlib
import http.server
import io
import os
from pathlib import Path
import platform
import subprocess
import tarfile
import tempfile
import threading
import unittest

ROOT = Path(__file__).resolve().parents[1]
TARGETS = {('Darwin', 'arm64'): 'aarch64-apple-darwin', ('Darwin', 'x86_64'): 'x86_64-apple-darwin', ('Linux', 'x86_64'): 'x86_64-unknown-linux-gnu'}
TARGET = TARGETS.get((platform.system(), platform.machine()))


class QuietHandler(http.server.SimpleHTTPRequestHandler):
    def log_message(self, *_):
        pass


class Installer(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix='ronchi-installer-')
        self.root = Path(self.temp.name)
        self.mirror = self.root / 'mirror'
        self.mirror.mkdir()
        self.prefix = self.root / "install with ' spaces"
        self.server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), functools.partial(QuietHandler, directory=self.mirror))
        self.thread = threading.Thread(target=self.server.serve_forever, daemon=True)
        self.thread.start()
        self.env = {k: v for k, v in os.environ.items() if not k.startswith('RONCHI_')}
        self.env['RONCHI_RELEASE_BASE_URL'] = f'http://127.0.0.1:{self.server.server_port}'
        self.env['RONCHI_INSTALL_PREFIX'] = str(self.prefix)

    def tearDown(self):
        self.server.shutdown()
        self.server.server_close()
        self.thread.join()
        self.temp.cleanup()

    def release(self, version='1.2.3', mutate=None):
        name = f'ronchi-{version}-{TARGET}'
        files = {
            'bin/ronchi': f'#!/bin/sh\nprintf "ronchi {version}\\n"\n'.encode(),
            'libexec/surfpool/1.7.0/surfpool': b'#!/bin/sh\nprintf "surfpool 1.7.0\\n"\n',
            'libexec/surfpool/1.7.0/artifact.tar.gz': b'fixture upstream archive',
            'LICENSE': b'MIT fixture', 'release.json': b'{}',
            'licenses/SURFPOOL-LICENSE': b'Apache-2.0 fixture',
            'licenses/THIRD-PARTY-LICENSES.html': b'fixture notices',
            'licenses/SURFPOOL-THIRD-PARTY-LICENSES.html': b'fixture notices',
            'licenses/SURFPOOL-SOURCES.tar.gz': b'fixture source archive',
        }
        output = self.mirror / 'download' / f'v{version}'
        output.mkdir(parents=True, exist_ok=True)
        archive = output / f'{name}.tar.gz'
        with tarfile.open(archive, 'w:gz', format=tarfile.USTAR_FORMAT) as tar:
            for path, data in files.items():
                info = tarfile.TarInfo(f'{name}/{path}')
                info.size = len(data)
                info.mode = 0o755 if path.endswith(('ronchi', 'surfpool')) else 0o644
                tar.addfile(info, io.BytesIO(data))
            if mutate:
                mutate(tar, name)
        checksum = archive.with_name(archive.name + '.sha256')
        checksum.write_text(hashlib.sha256(archive.read_bytes()).hexdigest() + '\n')
        latest = self.mirror / 'latest/download'
        latest.mkdir(parents=True, exist_ok=True)
        (latest / 'release-version.txt').write_text(version + '\n')
        return archive

    def run_install(self, *args, success=True):
        result = subprocess.run(['sh', str(ROOT / 'install.sh'), *args], env=self.env, text=True, capture_output=True, timeout=20)
        self.assertEqual(result.returncode == 0, success, result.stdout + result.stderr)
        self.assertFalse((self.prefix / '.ronchi-install-lock').exists())
        return result

    def test_latest_install_and_same_version_are_idempotent(self):
        self.release()
        self.run_install()
        link = self.prefix / 'bin/ronchi'
        first, inode = link.resolve(), link.stat().st_ino
        self.run_install('--version', 'v1.2.3')
        self.assertEqual(link.resolve(), first)
        self.assertEqual(link.stat().st_ino, inode)
        self.assertTrue((first.parent.parent / 'libexec/surfpool/1.7.0/surfpool').exists())
        self.assertFalse((self.prefix / 'ronchi').exists())

    def test_update_keeps_previous_executable(self):
        self.release()
        self.run_install()
        old = (self.prefix / 'bin/ronchi').resolve()
        self.release('1.2.4')
        self.run_install()
        new = (self.prefix / 'bin/ronchi').resolve()
        self.assertNotEqual(old, new)
        self.assertEqual(subprocess.check_output([old], text=True).strip(), 'ronchi 1.2.3')
        self.assertEqual(subprocess.check_output([new], text=True).strip(), 'ronchi 1.2.4')

    def test_corrupt_download_preserves_working_install(self):
        self.release()
        self.run_install()
        link = (self.prefix / 'bin/ronchi').resolve()
        self.release('1.2.4').write_bytes(b'broken archive')
        result = self.run_install(success=False)
        self.assertIn('SHA-256 mismatch', result.stderr)
        self.assertEqual(link, (self.prefix / 'bin/ronchi').resolve())

    def test_missing_download_does_not_activate(self):
        self.release()
        self.run_install('--version', '9.9.9', success=False)
        self.assertFalse((self.prefix / 'bin/ronchi').exists())

    def test_traversal_is_rejected_before_extraction(self):
        def bad(tar, _):
            info = tarfile.TarInfo('../escaped')
            info.size = 1
            tar.addfile(info, io.BytesIO(b'x'))
        self.release(mutate=bad)
        self.run_install(success=False)
        self.assertFalse((self.prefix / 'escaped').exists())
        self.assertFalse((self.prefix / 'bin/ronchi').exists())

    def test_symlink_is_rejected(self):
        def bad(tar, name):
            info = tarfile.TarInfo(name + '/bin/')
            info.type, info.linkname = tarfile.SYMTYPE, '/tmp'
            tar.addfile(info)
        self.release(mutate=bad)
        self.assertIn('links and special files', self.run_install(success=False).stderr)

    def test_duplicate_entries_are_rejected(self):
        def bad(tar, name):
            info = tarfile.TarInfo(name + '/LICENSE')
            info.size = 1
            tar.addfile(info, io.BytesIO(b'x'))
        self.release(mutate=bad)
        self.assertIn('Duplicate', self.run_install(success=False).stderr)

    def test_wrong_executable_version_does_not_activate(self):
        self.release('1.2.3', lambda tar, name: None)
        # A valid package checksum cannot establish that its executable will run.
        archive = self.mirror / 'download/v1.2.3' / f'ronchi-1.2.3-{TARGET}.tar.gz'
        with tarfile.open(archive) as tar:
            contents = [(m, tar.extractfile(m).read()) for m in tar.getmembers()]
        with tarfile.open(archive, 'w:gz') as tar:
            for info, data in contents:
                if info.name.endswith('/bin/ronchi'):
                    data = b'#!/bin/sh\necho "ronchi wrong"\n'; info.size = len(data)
                tar.addfile(info, io.BytesIO(data))
        archive.with_name(archive.name + '.sha256').write_text(hashlib.sha256(archive.read_bytes()).hexdigest() + '\n')
        self.assertIn('incorrect version', self.run_install(success=False).stderr)
        self.assertFalse((self.prefix / 'bin/ronchi').exists())

    def test_unmanaged_binary_is_preserved(self):
        self.release()
        (self.prefix / 'bin').mkdir(parents=True)
        original = self.prefix / 'bin/ronchi'
        original.write_text('unmanaged')
        self.run_install(success=False)
        self.assertEqual(original.read_text(), 'unmanaged')

    def test_damaged_installed_version_is_not_silently_replaced(self):
        self.release()
        self.run_install()
        binary = (self.prefix / 'bin/ronchi').resolve()
        binary.write_text('damaged')
        self.assertIn('damaged', self.run_install(success=False).stderr)
        self.assertEqual(binary.read_text(), 'damaged')

    def test_existing_executable_permissions_are_checked(self):
        self.release()
        self.run_install()
        binary = (self.prefix / 'bin/ronchi').resolve()
        binary.chmod(0o644)
        self.assertIn('executables are damaged', self.run_install(success=False).stderr)
        self.assertFalse(os.access(binary, os.X_OK))

    def test_existing_release_directory_symlinks_are_rejected(self):
        self.release()
        self.run_install()
        release = (self.prefix / 'bin/ronchi').resolve().parent.parent
        external = self.root / 'external-licenses'
        (release / 'licenses').rename(external)
        (release / 'licenses').symlink_to(external, target_is_directory=True)
        self.assertIn('directories are damaged', self.run_install(success=False).stderr)
        self.assertTrue((release / 'licenses').is_symlink())

    def test_another_install_lock_is_not_removed(self):
        self.release()
        self.prefix.mkdir()
        lock = self.prefix / '.ronchi-install-lock'
        lock.mkdir()
        result = subprocess.run(['sh', str(ROOT / 'install.sh')], env=self.env, text=True, capture_output=True, timeout=10)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('Another installation', result.stderr)
        self.assertTrue(lock.exists())

    def test_unrelated_symlink_is_preserved(self):
        self.release()
        (self.prefix / 'bin').mkdir(parents=True)
        other = self.root / 'other-tool'
        other.write_text('do not replace')
        link = self.prefix / 'bin/ronchi'
        link.symlink_to(other)
        self.run_install(success=False)
        self.assertEqual(link.resolve(), other.resolve())
        self.assertEqual(other.read_text(), 'do not replace')

    def test_private_install_directory_symlink_is_rejected(self):
        self.release()
        self.prefix.mkdir()
        external = self.root / 'external'
        external.mkdir()
        (self.prefix / 'lib').symlink_to(external, target_is_directory=True)
        self.run_install(success=False)
        self.assertEqual(list(external.iterdir()), [])

    def test_path_instruction_handles_single_quotes(self):
        self.release()
        output = self.run_install().stdout
        instruction = next(line.strip() for line in output.splitlines() if line.strip().startswith('export PATH='))
        resolved = subprocess.check_output(['sh', '-c', instruction + '\ncommand -v ronchi'], text=True).strip()
        self.assertEqual(resolved, str(self.prefix.resolve() / 'bin/ronchi'))


if __name__ == '__main__':
    if not TARGET:
        raise SystemExit('Installer tests require a supported native platform')
    unittest.main()
