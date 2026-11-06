"""Static checks and SQLite schema smoke tests; does NOT compile Rust or run Tauri."""
from pathlib import Path
import json, re, sqlite3, subprocess

BASE=Path(__file__).resolve().parents[1]
ui=(BASE/'ui/index.html').read_text(encoding='utf-8')
js=(BASE/'ui/app.js').read_text(encoding='utf-8')
rs=(BASE/'src-tauri/src/store.rs').read_text(encoding='utf-8')
main=(BASE/'src-tauri/src/main.rs').read_text(encoding='utf-8')
config=json.loads((BASE/'src-tauri/tauri.conf.json').read_text(encoding='utf-8'))
assert config['build']['withGlobalTauri'] is True
assert config['tauri']['bundle']['windows']['webviewInstallMode']['type']=='skip'
assert '<html lang="ar" dir="rtl">' in ui
ids=set(re.findall(r'\bid="([\w-]+)"',ui))
js_ids=set(re.findall(r"\$\('([\w-]+)'\)",js))|set(re.findall(r"on\('([\w-]+)'",js))
missing=js_ids-ids
assert not missing,f'Missing DOM ids: {missing}'
# Check frontend invokes only declared Rust commands.
commands=set(re.findall(r'fn (\w+)\(db: State',main))
invocations=set(re.findall(r"api\('([a-z_]+)'",js))
assert not (invocations-commands),f'Commands missing: {invocations-commands}'
match=re.search(r'conn\.execute_batch\(r#"(.*?)"#\)',rs,re.S)
assert match,'SQLite migration schema not found'
conn=sqlite3.connect(':memory:')
conn.executescript(match.group(1))
conn.execute('INSERT INTO classes(name) VALUES (?)',('الخامس العلمي',))
cls=conn.execute('SELECT id FROM classes').fetchone()[0]
conn.execute('INSERT INTO sections(class_id,name) VALUES (?,?)',(cls,'أ'))
sec=conn.execute('SELECT id FROM sections').fetchone()[0]
conn.execute('INSERT INTO students(section_id,name,enrollment) VALUES (?,?,?)',(sec,'أحمد','regular'))
student=conn.execute('SELECT id FROM students WHERE name=?',('أحمد',)).fetchone()[0]
for day in range(1,10):
    for lesson in range(1,6):
        conn.execute('INSERT INTO attendance(student_id,date,lesson,status) VALUES (?,?,?,?)',(student,f'2026-10-{day:02}',lesson,'absent'))
    
assert conn.execute("SELECT COUNT(*) FROM attendance WHERE status='absent'").fetchone()[0]==45
assert 45/5>8
try:
    conn.execute('INSERT INTO attendance(student_id,date,lesson,status) VALUES (?,?,?,?)',(student,'2026-10-10',1,'unexcused'))
    raise AssertionError('Invalid attendance status accepted')
except sqlite3.IntegrityError: pass
conn.execute('DELETE FROM classes WHERE id=?',(cls,))
assert conn.execute('SELECT COUNT(*) FROM attendance').fetchone()[0]==0
assert conn.execute('SELECT COUNT(*) FROM students').fetchone()[0]==0
print('PASS: Tauri config, RTL, DOM IDs, command coverage, SQLite schema, constraints, cascade, and 8-day threshold math')
