#!/usr/bin/env python3
"""Convert Cargo's newly created v4 lockfile to v3 for Rust/Cargo 1.77.

The stable Cargo resolver first chooses dependency versions compatible with
rust-version=1.77.2, but it writes v4 lockfiles that older Cargo cannot parse.
"""
from pathlib import Path
import re

p = Path(__file__).resolve().parent.parent / 'src-tauri' / 'Cargo.lock'
if not p.is_file():
    raise SystemExit(f'Cargo.lock missing: {p}')
raw = p.read_text(encoding='utf-8')
changed, count = re.subn(r'(?m)^version = 4$', 'version = 3', raw, count=1)
if not count and not re.search(r'(?m)^version = 3$', raw):
    raise SystemExit('Unexpected Cargo.lock version, did not modify')
if count:
    p.write_text(changed, encoding='utf-8')
print('Cargo.lock is in v3 format (Cargo 1.77 compatible)')
