#!/usr/bin/env python3
"""Construct the actual macOS .app from a Mach-O binary built ON macOS.

Usage: python3 scripts/package_macos.py
Does not compile Rust, sign for Developer ID, or notarize the result.
"""
from pathlib import Path
import os
import platform
import plistlib
import shutil
import subprocess
import sys

if platform.system() != 'Darwin':
    raise SystemExit('This packager MUST run on macOS with a compiled Mach-O binary.')
root = Path(__file__).resolve().parent.parent
binary = root / 'src-tauri' / 'target' / 'release' / 'school_attendance'
if not binary.is_file():
    raise SystemExit(f'Executable not found; compile first: {binary}')
magic = binary.open('rb').read(4)
if magic not in (b'\xfe\xed\xfa\xcf', b'\xcf\xfa\xed\xfe', b'\xca\xfe\xba\xbe', b'\xbe\xba\xfe\xca'):
    raise SystemExit('Binary is not a 64-bit Mach-O executable')
app = root / 'dist' / 'SchoolAttendance.app'
macos = app / 'Contents' / 'MacOS'
res = app / 'Contents' / 'Resources'
macos.mkdir(parents=True, exist_ok=True)
res.mkdir(parents=True, exist_ok=True)
shutil.copy2(binary, macos / 'school_attendance')
os.chmod(macos / 'school_attendance', 0o755)
shutil.copy2(root / 'src-tauri' / 'icons' / 'icon.icns', res / 'AppIcon.icns')
info = {
    'CFBundleDevelopmentRegion': 'ar',
    'CFBundleDisplayName': 'دفتر الغياب',
    'CFBundleExecutable': 'school_attendance',
    'CFBundleIconFile': 'AppIcon',
    'CFBundleIdentifier': 'com.school.attendance',
    'CFBundleInfoDictionaryVersion': '6.0',
    'CFBundleName': 'SchoolAttendance',
    'CFBundlePackageType': 'APPL',
    'CFBundleShortVersionString': '1.1.0',
    'CFBundleVersion': '1.1.0',
    'LSMinimumSystemVersion': '10.15',
    'NSHighResolutionCapable': True,
}
with (app / 'Contents' / 'Info.plist').open('wb') as handle:
    plistlib.dump(info, handle)
# Ad-hoc signing supports local test builds, it is NOT Apple Developer ID notarization.
subprocess.run(['codesign', '--force', '--deep', '--sign', '-', '--timestamp=none', str(app)], check=True)
arch = subprocess.check_output(['uname', '-m'], text=True).strip()
zip_path = app.parent / f'SchoolAttendance-macOS-{arch}.zip'
subprocess.run(['ditto', '-c', '-k', '--sequesterRsrc', '--keepParent', str(app), str(zip_path)], check=True)
print(f'Created {app}\nCreated {zip_path}')
