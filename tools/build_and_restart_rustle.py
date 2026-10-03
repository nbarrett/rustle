import hashlib
import os
from pathlib import Path
import signal
import subprocess
import sys
import tempfile
import time


REPOSITORY = Path(__file__).resolve().parents[1]
INSTALLED_APP = Path('/Applications/Rustle.app')
BUILT_APP = REPOSITORY / 'target/release/bundle/macos/Rustle.app'
EXECUTABLE_PATH = Path('Contents/MacOS/rustle-app')


def run_command_in_repository(arguments):
    subprocess.run(arguments, cwd=REPOSITORY, check=True)


def find_installed_rustle_processes():
    result = subprocess.run(
        ['pgrep', '-f', '^/Applications/Rustle.app/Contents/MacOS/rustle-app$'],
        capture_output=True,
        text=True,
    )
    if result.returncode not in (0, 1):
        raise RuntimeError('Could not inspect running Rustle processes')
    return [int(pid) for pid in result.stdout.split()]


def stop_installed_rustle_processes():
    for pid in find_installed_rustle_processes():
        try:
            os.kill(pid, signal.SIGTERM)
        except ProcessLookupError:
            pass
    for attempt in range(50):
        if not find_installed_rustle_processes():
            return
        time.sleep(0.1)
    raise RuntimeError('Rustle did not stop; the installed app was not replaced')


def hash_app_executable(app):
    with (app / EXECUTABLE_PATH).open('rb') as executable:
        return hashlib.file_digest(executable, 'sha256').hexdigest()


def open_installed_rustle_and_verify_process():
    run_command_in_repository(['open', '-n', str(INSTALLED_APP)])
    for attempt in range(50):
        if find_installed_rustle_processes():
            return
        time.sleep(0.1)
    raise RuntimeError('Rustle did not start')


def install_built_rustle_and_restart():
    run_command_in_repository(['codesign', '--verify', '--deep', '--strict', str(BUILT_APP)])
    backup = Path(tempfile.mkdtemp(prefix='rustle-before-update-')) / 'Rustle.app'
    run_command_in_repository(['ditto', str(INSTALLED_APP), str(backup)])
    print(f'Previous app saved at {backup}', flush=True)
    stop_installed_rustle_processes()
    try:
        run_command_in_repository(['ditto', str(BUILT_APP), str(INSTALLED_APP)])
        run_command_in_repository(['codesign', '--verify', '--deep', '--strict', str(INSTALLED_APP)])
        if hash_app_executable(INSTALLED_APP) != hash_app_executable(BUILT_APP):
            raise RuntimeError('The installed executable does not match the new build')
        open_installed_rustle_and_verify_process()
    except Exception:
        stop_installed_rustle_processes()
        run_command_in_repository(['ditto', str(backup), str(INSTALLED_APP)])
        open_installed_rustle_and_verify_process()
        raise
    print('Latest build is installed and running. Real dictation checks remain pending.', flush=True)


def test_build_install_and_restart_rustle():
    if sys.platform != 'darwin':
        raise RuntimeError('This build-and-restart command is for macOS')
    run_command_in_repository(['cargo', 'test', '--workspace', '--lib'])
    run_command_in_repository(['pnpm', 'typecheck'])
    run_command_in_repository(['pnpm', 'typecheck:web'])
    run_command_in_repository(['pnpm', 'exec', 'tauri', 'build', '--bundles', 'app'])
    install_built_rustle_and_restart()


if __name__ == '__main__':
    test_build_install_and_restart_rustle()
