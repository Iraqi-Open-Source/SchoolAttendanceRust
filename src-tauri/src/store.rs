use chrono::{Datelike, Local, NaiveDate};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::{collections::{HashMap, HashSet}, fs, path::PathBuf, sync::Mutex, time::Duration};

type R<T> = Result<T, String>;
fn msg(e: impl std::fmt::Display) -> String { e.to_string() }
fn clean(input: &str, field: &str, max: usize) -> R<String> {
    let t = input.trim();
    if t.is_empty() || t.chars().count() > max { return Err(format!("{} مطلوب وطوله لا يتجاوز {} حرفاً", field, max)); }
    Ok(t.to_owned())
}
fn valid_day(day: &str) -> R<()> {
    if day.len() != 10 || NaiveDate::parse_from_str(day, "%Y-%m-%d").is_err() { Err("صيغة التاريخ غير صحيحة (YYYY-MM-DD)".into()) } else { Ok(()) }
}
fn valid_enrollment(v: &str) -> R<()> {
    if v == "regular" || v == "external" { Ok(()) } else { Err("نوع الدوام غير صحيح".into()) }
}

pub struct AppDb { conn: Mutex<Connection> }

#[derive(Serialize)]
pub struct Group { pub id: i64, pub name: String, pub sections: Vec<Section> }
#[derive(Serialize)]
pub struct Section { pub id: i64, pub name: String }
#[derive(Serialize)]
pub struct Student { pub id: i64, pub section_id: i64, pub name: String, pub code: String, pub enrollment: String, pub notes: String }
#[derive(Deserialize)]
pub struct StudentInput { pub id: Option<i64>, pub section_id: i64, pub name: String, pub code: String, pub enrollment: String, pub notes: String }
#[derive(Serialize)]
pub struct SheetStudent { pub student: Student, pub statuses: HashMap<i64, String> }
#[derive(Deserialize)]
pub struct AttendanceRowInput { pub student_id: i64, pub statuses: Vec<String> }
#[derive(Serialize, Clone)]
pub struct Alert { pub id: i64, pub name: String, pub threshold: f64, pub metric: String, pub operator: String, pub color: String, pub enabled: bool }
#[derive(Deserialize)]
pub struct AlertInput { pub id: Option<i64>, pub name: String, pub threshold: f64, pub metric: String, pub operator: String, pub color: String, pub enabled: bool }
#[derive(Serialize)]
pub struct AppConfig { pub school_name: String, pub lessons_per_day: i64, pub year_start: String, pub year_end: String, pub weekly_off_days: Vec<u8>, pub alerts: Vec<Alert> }
#[derive(Serialize)]
pub struct Holiday { pub date: String, pub name: String }
#[derive(Serialize)]
pub struct StudentReport {
    pub student_id: i64, pub student_name: String, pub class_name: String, pub section_name: String,
    pub enrollment: String, pub absent_lessons: i64, pub excused_lessons: i64,
    pub present_lessons: i64, pub absent_days: f64, pub full_absent_days: i64,
    pub partial_absent_lessons: i64, pub absence_text: String,
    pub full_excused_days: i64, pub partial_excused_lessons: i64, pub excused_text: String,
    pub alerts: Vec<Alert>,
}
#[derive(Serialize)]
pub struct HistoryRow { pub date: String, pub lesson: i64, pub status: String, pub note: String }
#[derive(Serialize)]
pub struct Dashboard { pub classes: i64, pub sections: i64, pub students: i64, pub regular: i64, pub external: i64, pub attendance_days: i64, pub absent_today: i64, pub school_name: String }

fn defaults(conn: &Connection) -> R<()> {
    let first_start: bool = conn.query_row("SELECT NOT EXISTS(SELECT 1 FROM settings)", [], |r| r.get(0)).map_err(msg)?;
    let today = Local::now().date_naive();
    let school_year = if today.month() >= 8 { today.year() } else { today.year() - 1 };
    let data = [
        ("school_name", "مدرستي".to_string()),
        ("lessons_per_day", "5".to_string()),
        ("year_start", format!("{}-09-01", school_year)),
        ("year_end", format!("{}-08-31", school_year + 1)),
        ("weekly_off_days", "5,6".to_string()),
    ];
    for (key, value) in data {
        conn.execute("INSERT OR IGNORE INTO settings(key,value) VALUES(?1,?2)", params![key,value]).map_err(msg)?;
    }
    if first_start {
        conn.execute("INSERT INTO alert_rules(name,threshold,metric,operator,color,enabled) VALUES('تجاوز ٨ أيام',8,'days','>','#dc454f',1)",[]).map_err(msg)?;
    }
    Ok(())
}

fn migrate(conn: &Connection) -> R<()> {
    conn.execute_batch(r#"
    PRAGMA foreign_keys=ON;
    PRAGMA busy_timeout=5000;
    CREATE TABLE IF NOT EXISTS classes (
      id INTEGER PRIMARY KEY, name TEXT NOT NULL UNIQUE COLLATE NOCASE
    );
    CREATE TABLE IF NOT EXISTS sections (
      id INTEGER PRIMARY KEY, class_id INTEGER NOT NULL REFERENCES classes(id) ON DELETE CASCADE,
      name TEXT NOT NULL, UNIQUE(class_id,name)
    );
    CREATE TABLE IF NOT EXISTS students (
      id INTEGER PRIMARY KEY, section_id INTEGER NOT NULL REFERENCES sections(id) ON DELETE CASCADE,
      name TEXT NOT NULL, code TEXT NOT NULL DEFAULT '',
      enrollment TEXT NOT NULL DEFAULT 'regular' CHECK(enrollment IN ('regular','external')),
      notes TEXT NOT NULL DEFAULT ''
    );
    CREATE INDEX IF NOT EXISTS ix_students_section ON students(section_id);
    CREATE TABLE IF NOT EXISTS attendance (
      student_id INTEGER NOT NULL REFERENCES students(id) ON DELETE CASCADE,
      date TEXT NOT NULL, lesson INTEGER NOT NULL CHECK(lesson BETWEEN 1 AND 20),
      status TEXT NOT NULL CHECK(status IN ('present','absent','excused')),
      note TEXT NOT NULL DEFAULT '',
      PRIMARY KEY(student_id,date,lesson)
    );
    CREATE INDEX IF NOT EXISTS ix_attendance_date ON attendance(date);
    CREATE TABLE IF NOT EXISTS settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);
    CREATE TABLE IF NOT EXISTS alert_rules (
      id INTEGER PRIMARY KEY, name TEXT NOT NULL, threshold REAL NOT NULL,
      metric TEXT NOT NULL CHECK(metric IN ('days','lessons')),
      operator TEXT NOT NULL CHECK(operator IN ('>','>=')),
      color TEXT NOT NULL, enabled INTEGER NOT NULL DEFAULT 1
    );
    CREATE TABLE IF NOT EXISTS holidays (date TEXT PRIMARY KEY, name TEXT NOT NULL);
    "#).map_err(msg)?;
    defaults(conn)
}

fn setting(conn: &Connection, key: &str) -> R<String> {
    conn.query_row("SELECT value FROM settings WHERE key=?1", [key], |r| r.get(0)).map_err(msg)
}
fn js_weekday_from_date(date: &str) -> R<u8> {
    let d = NaiveDate::parse_from_str(date, "%Y-%m-%d").map_err(|_| "صيغة التاريخ غير صحيحة (YYYY-MM-DD)".to_string())?;
    Ok(((d.weekday().num_days_from_monday() as u8).wrapping_add(1)) % 7)
}
fn parse_weekly_off_days(raw: &str) -> Vec<u8> {
    let mut days: Vec<u8> = raw
        .split(',')
        .filter_map(|part| part.trim().parse::<u8>().ok())
        .filter(|day| *day <= 6)
        .collect();
    days.sort_unstable();
    days.dedup();
    days
}
fn weekly_off_days(conn: &Connection) -> R<Vec<u8>> {
    let raw = setting(conn, "weekly_off_days").unwrap_or_else(|_| "5,6".into());
    Ok(parse_weekly_off_days(&raw))
}
fn is_weekly_off(conn: &Connection, date: &str) -> R<bool> {
    let weekday = js_weekday_from_date(date)?;
    Ok(weekly_off_days(conn)?.contains(&weekday))
}
fn format_weekly_off_days(days: &[u8]) -> String {
    parse_weekly_off_days(
        &days
            .iter()
            .map(|d| d.to_string())
            .collect::<Vec<_>>()
            .join(","),
    )
    .iter()
    .map(|d| d.to_string())
    .collect::<Vec<_>>()
    .join(",")
}
fn config_with(conn: &Connection) -> R<AppConfig> {
    let mut stmt = conn.prepare("SELECT id,name,threshold,metric,operator,color,enabled FROM alert_rules ORDER BY threshold DESC,id").map_err(msg)?;
    let alerts = stmt.query_map([], |r| Ok(Alert {
        id:r.get(0)?, name:r.get(1)?, threshold:r.get(2)?, metric:r.get(3)?, operator:r.get(4)?, color:r.get(5)?, enabled:r.get::<_,i64>(6)? == 1
    })).map_err(msg)?.collect::<Result<Vec<_>,_>>().map_err(msg)?;
    let count = setting(conn, "lessons_per_day")?.parse::<i64>().unwrap_or(5).clamp(1, 20);
    Ok(AppConfig { school_name:setting(conn,"school_name")?, lessons_per_day:count,
        year_start:setting(conn,"year_start")?, year_end:setting(conn,"year_end")?,
        weekly_off_days:weekly_off_days(conn)?, alerts })
}
fn count(conn: &Connection, sql: &str) -> R<i64> {
    conn.query_row(sql,[],|r|r.get(0)).map_err(msg)
}
impl AppDb {
    pub fn open(path: PathBuf) -> R<Self> {
        let conn = Connection::open(path).map_err(msg)?;
        migrate(&conn)?;
        Ok(Self {conn: Mutex::new(conn)})
    }
    #[cfg(test)]
    fn memory() -> Self {
        let conn=Connection::open_in_memory().unwrap(); migrate(&conn).unwrap();
        Self{conn:Mutex::new(conn)}
    }
    fn db(&self) -> R<std::sync::MutexGuard<'_, Connection>> {
        self.conn.lock().map_err(|_|"تعذر الوصول إلى قاعدة البيانات".to_owned())
    }
    pub fn groups(&self) -> R<Vec<Group>> {
        let conn = self.db()?;
        let mut cs = conn.prepare("SELECT id,name FROM classes ORDER BY name").map_err(msg)?;
        let mut classes = cs.query_map([],|r|Ok(Group{id:r.get(0)?,name:r.get(1)?,sections:vec![]})).map_err(msg)?
            .collect::<Result<Vec<_>,_>>().map_err(msg)?;
        let mut ss = conn.prepare("SELECT id,name FROM sections WHERE class_id=?1 ORDER BY name").map_err(msg)?;
        for c in &mut classes {
            c.sections=ss.query_map([c.id],|r|Ok(Section{id:r.get(0)?,name:r.get(1)?})).map_err(msg)?
                .collect::<Result<Vec<_>,_>>().map_err(msg)?;
        }
        Ok(classes)
    }
    pub fn create_class(&self, name:&str) -> R<i64> {
        let name=clean(name,"اسم الصف",100)?; let conn=self.db()?;
        conn.execute("INSERT INTO classes(name) VALUES(?1)",[name]).map_err(msg)?; Ok(conn.last_insert_rowid())
    }
    pub fn rename_class(&self,id:i64,name:&str)->R<()> {
        let name=clean(name,"اسم الصف",100)?;let conn=self.db()?;
        if conn.execute("UPDATE classes SET name=?1 WHERE id=?2",params![name,id]).map_err(msg)?==0 {return Err("الصف غير موجود".into())} Ok(())
    }
    pub fn delete_class(&self,id:i64)->R<()> {
        let conn=self.db()?;
        conn.execute("DELETE FROM classes WHERE id=?1",[id]).map_err(msg)?; Ok(())
    }
    pub fn create_section(&self,class_id:i64,name:&str)->R<i64> {
        let name=clean(name,"اسم الشعبة",100)?;let conn=self.db()?;
        conn.execute("INSERT INTO sections(class_id,name) VALUES(?1,?2)",params![class_id,name]).map_err(msg)?;Ok(conn.last_insert_rowid())
    }
    pub fn rename_section(&self,id:i64,name:&str)->R<()> {
        let name=clean(name,"اسم الشعبة",100)?;let conn=self.db()?;
        if conn.execute("UPDATE sections SET name=?1 WHERE id=?2",params![name,id]).map_err(msg)?==0{return Err("الشعبة غير موجودة".into())} Ok(())
    }
    pub fn delete_section(&self,id:i64)->R<()> {
        let conn=self.db()?;conn.execute("DELETE FROM sections WHERE id=?1",[id]).map_err(msg)?;Ok(())
    }
    pub fn students(&self,section_id:i64)->R<Vec<Student>> {
        let conn=self.db()?;
        let mut stmt=conn.prepare("SELECT id,section_id,name,code,enrollment,notes FROM students WHERE section_id=?1 ORDER BY name,id").map_err(msg)?;
        let data=stmt.query_map([section_id], |r| Ok(Student{id:r.get(0)?,section_id:r.get(1)?,name:r.get(2)?,code:r.get(3)?,enrollment:r.get(4)?,notes:r.get(5)?}))
            .map_err(msg)?.collect::<Result<Vec<_>,_>>().map_err(msg)?;
        Ok(data)
    }
    pub fn upsert_student(&self,s:StudentInput)->R<i64> {
        let name=clean(&s.name,"اسم الطالب",150)?;
        valid_enrollment(&s.enrollment)?;
        let code=s.code.trim(); let notes=s.notes.trim();
        if code.chars().count()>80 || notes.chars().count()>600{return Err("الرمز أو الملاحظات أطول من الحد المسموح".into())}
        let conn=self.db()?;
        if let Some(id)=s.id {
            if conn.execute("UPDATE students SET section_id=?1,name=?2,code=?3,enrollment=?4,notes=?5 WHERE id=?6",params![s.section_id,name,code,s.enrollment,notes,id]).map_err(msg)?==0 {
                return Err("الطالب غير موجود".into());
            }
            Ok(id)
        }else{
            conn.execute("INSERT INTO students(section_id,name,code,enrollment,notes) VALUES(?1,?2,?3,?4,?5)",params![s.section_id,name,code,s.enrollment,notes]).map_err(msg)?;
            Ok(conn.last_insert_rowid())
        }
    }
    pub fn delete_student(&self,id:i64)->R<()> {
        let conn=self.db()?;conn.execute("DELETE FROM students WHERE id=?1",[id]).map_err(msg)?;Ok(())
    }
    pub fn move_student(&self,id:i64,section_id:i64)->R<()> {
        let conn=self.db()?;
        let exists:bool=conn.query_row("SELECT EXISTS(SELECT 1 FROM sections WHERE id=?1)",[section_id],|r|r.get(0)).map_err(msg)?;
        if !exists{return Err("الشعبة غير موجودة".into())}
        if conn.execute("UPDATE students SET section_id=?1 WHERE id=?2",params![section_id,id]).map_err(msg)?==0{
            return Err("الطالب غير موجود".into());
        }
        Ok(())
    }
    pub fn import_students(&self,section_id:i64,names:&str,enrollment:&str)->R<u32> {
        valid_enrollment(enrollment)?;
        let names=names.lines().map(str::trim).filter(|v|!v.is_empty()).map(|v|clean(v,"اسم الطالب",150)).collect::<R<Vec<_>>>()?;
        if names.is_empty() || names.len()>500 {return Err("أدخل بين 1 و500 طالب في كل عملية".into())}
        let mut conn=self.db()?;let tx=conn.transaction().map_err(msg)?;
        for n in &names { tx.execute("INSERT INTO students(section_id,name,enrollment) VALUES(?1,?2,?3)",params![section_id,n,enrollment]).map_err(msg)?; }
        tx.commit().map_err(msg)?;Ok(names.len() as u32)
    }
    pub fn sheet(&self,section_id:i64,date:&str)->R<Vec<SheetStudent>> {
        valid_day(date)?;
        let students=self.students(section_id)?;
        let conn=self.db()?;
        let mut stmt=conn.prepare("SELECT a.student_id,a.lesson,a.status FROM attendance a JOIN students s ON s.id=a.student_id WHERE s.section_id=?1 AND a.date=?2").map_err(msg)?;
        let recs=stmt.query_map(params![section_id,date],|r|Ok((r.get::<_,i64>(0)?,r.get::<_,i64>(1)?,r.get::<_,String>(2)?))).map_err(msg)?;
        let mut map:HashMap<i64,HashMap<i64,String>>=HashMap::new();
        for item in recs { let (id,lesson,status)=item.map_err(msg)?;map.entry(id).or_default().insert(lesson,status); }
        Ok(students.into_iter().map(|student|SheetStudent{statuses:map.remove(&student.id).unwrap_or_default(),student}).collect())
    }
    pub fn save_sheet(&self,section_id:i64,date:&str,lessons:u32,rows:Vec<AttendanceRowInput>)->R<()> {
        valid_day(date)?;
        if !(1..=20).contains(&lessons) {return Err("عدد الدروس يجب أن يكون بين 1 و20".into())}
        let mut conn=self.db()?;
        let blocked=conn.query_row("SELECT EXISTS(SELECT 1 FROM holidays WHERE date=?1)",[date],|r|r.get::<_,bool>(0)).map_err(msg)?;
        if blocked {return Err("هذا اليوم مسجل عطلة؛ أزل العطلة أولاً إن أردت تسجيل الدوام".into())}
        if is_weekly_off(&conn, date)? {return Err("هذا اليوم ضمن العطل الأسبوعية؛ لا يمكن تسجيل الدوام".into())}
        let expected_ids:HashSet<i64>={
            let mut stmt=conn.prepare("SELECT id FROM students WHERE section_id=?1 AND enrollment='regular'").map_err(msg)?;
            let result=stmt.query_map([section_id],|r|r.get(0)).map_err(msg)?.collect::<Result<HashSet<i64>,_>>().map_err(msg)?;
            result
        };
        let given_ids:HashSet<i64>=rows.iter().map(|r|r.student_id).collect();
        if expected_ids.is_empty(){return Err("لا يوجد طلاب دوام اعتيادي في هذه الشعبة".into())}
        if given_ids!=expected_ids || given_ids.len()!=rows.len(){return Err("قائمة الطلاب تغيّرت، حدّث الصفحة ثم أعد الحفظ".into())}
        for row in &rows {
            if row.statuses.len()!=lessons as usize{return Err("عدد الدروس غير مطابق".into())}
            if row.statuses.iter().any(|s|!matches!(s.as_str(),"present"|"absent"|"excused")) {return Err("حالة حضور غير صحيحة".into())}
        }
        let tx=conn.transaction().map_err(msg)?;
        for row in &rows {
            tx.execute("DELETE FROM attendance WHERE student_id=?1 AND date=?2",params![row.student_id,date]).map_err(msg)?;
            for (i,status) in row.statuses.iter().enumerate() {
                tx.execute("INSERT INTO attendance(student_id,date,lesson,status) VALUES(?1,?2,?3,?4)",params![row.student_id,date,(i+1) as i64,status]).map_err(msg)?;
            }
        }
        tx.commit().map_err(msg)
    }
    pub fn clear_sheet(&self,section_id:i64,date:&str)->R<()> {
        valid_day(date)?;
        let conn=self.db()?;
        conn.execute("DELETE FROM attendance WHERE date=?1 AND student_id IN (SELECT id FROM students WHERE section_id=?2)",params![date,section_id]).map_err(msg)?;
        Ok(())
    }
    pub fn config(&self)->R<AppConfig>{let conn=self.db()?;config_with(&conn)}
    pub fn save_config(&self,school_name:&str,lessons_per_day:u32,year_start:&str,year_end:&str,weekly_off_days:Vec<u8>)->R<()> {
        let school_name=clean(school_name,"اسم المدرسة",150)?;
        if !(1..=20).contains(&lessons_per_day){return Err("عدد الدروس يجب أن يكون بين 1 و20".into())}
        valid_day(year_start)?;valid_day(year_end)?;
        if year_start > year_end{return Err("نهاية السنة يجب أن تأتي بعد بدايتها".into())}
        if weekly_off_days.iter().any(|day| *day > 6){return Err("أيام العطل الأسبوعية غير صحيحة".into())}
        let weekly_off=format_weekly_off_days(&weekly_off_days);
        let mut conn=self.db()?;let tx=conn.transaction().map_err(msg)?;
        for (key,value) in [
            ("school_name",school_name),
            ("lessons_per_day",lessons_per_day.to_string()),
            ("year_start",year_start.to_owned()),
            ("year_end",year_end.to_owned()),
            ("weekly_off_days",weekly_off),
        ] {
            tx.execute("INSERT INTO settings(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",params![key,value]).map_err(msg)?;
        }
        tx.commit().map_err(msg)
    }
    pub fn save_alert(&self,a:AlertInput)->R<i64> {
        let name=clean(&a.name,"اسم التنبيه",100)?;
        if !a.threshold.is_finite() || a.threshold<0.0 || a.threshold>10000.0{return Err("حد التنبيه غير صحيح".into())}
        if !matches!(a.metric.as_str(),"days"|"lessons") || !matches!(a.operator.as_str(),">"|">="){return Err("إعداد التنبيه غير صحيح".into())}
        if a.color.len()!=7 || !a.color.starts_with('#') || !a.color.chars().skip(1).all(|c|c.is_ascii_hexdigit()){return Err("لون التنبيه يجب أن يكون رمز HEX مثل #dc454f".into())}
        let conn=self.db()?;
        if let Some(id)=a.id {
            if conn.execute("UPDATE alert_rules SET name=?1,threshold=?2,metric=?3,operator=?4,color=?5,enabled=?6 WHERE id=?7",params![name,a.threshold,a.metric,a.operator,a.color,a.enabled as i64,id]).map_err(msg)?==0{return Err("التنبيه غير موجود".into())}
            Ok(id)
        }else{
            conn.execute("INSERT INTO alert_rules(name,threshold,metric,operator,color,enabled) VALUES(?1,?2,?3,?4,?5,?6)",params![name,a.threshold,a.metric,a.operator,a.color,a.enabled as i64]).map_err(msg)?;
            Ok(conn.last_insert_rowid())
        }
    }
    pub fn delete_alert(&self,id:i64)->R<()> {let conn=self.db()?;conn.execute("DELETE FROM alert_rules WHERE id=?1",[id]).map_err(msg)?;Ok(())}
    pub fn holidays(&self)->R<Vec<Holiday>> {
        let conn=self.db()?;let mut stmt=conn.prepare("SELECT date,name FROM holidays ORDER BY date DESC").map_err(msg)?;
        let data=stmt.query_map([],|r|Ok(Holiday{date:r.get(0)?,name:r.get(1)?})).map_err(msg)?.collect::<Result<Vec<_>,_>>().map_err(msg)?;
        Ok(data)
    }
    pub fn set_holiday(&self,date:&str,name:&str)->R<()> {
        valid_day(date)?;let name=clean(name,"اسم العطلة",150)?;let conn=self.db()?;
        let existing:i64=conn.query_row("SELECT COUNT(*) FROM attendance WHERE date=?1",[date],|r|r.get(0)).map_err(msg)?;
        if existing>0 {return Err("توجد حالات دوام مسجلة لهذا اليوم؛ لا يمكن تحويله إلى عطلة قبل حذف تلك السجلات".into())}
        conn.execute("INSERT INTO holidays(date,name) VALUES(?1,?2) ON CONFLICT(date) DO UPDATE SET name=excluded.name",params![date,name]).map_err(msg)?;Ok(())
    }
    pub fn delete_holiday(&self,date:&str)->R<()> {valid_day(date)?;let conn=self.db()?;conn.execute("DELETE FROM holidays WHERE date=?1",[date]).map_err(msg)?;Ok(())}
    pub fn report(&self,section_id:Option<i64>)->R<Vec<StudentReport>> {
        let conn=self.db()?;report_with(&conn,section_id)
    }
    pub fn history(&self,student_id:i64)->R<Vec<HistoryRow>> {
        let conn=self.db()?;let conf=config_with(&conn)?;
        let mut stmt=conn.prepare("SELECT date,lesson,status,note FROM attendance WHERE student_id=?1 AND date BETWEEN ?2 AND ?3 ORDER BY date DESC,lesson ASC").map_err(msg)?;
        let data=stmt.query_map(params![student_id,conf.year_start,conf.year_end],|r|Ok(HistoryRow{date:r.get(0)?,lesson:r.get(1)?,status:r.get(2)?,note:r.get(3)?})).map_err(msg)?
          .collect::<Result<Vec<_>,_>>().map_err(msg)?;
        Ok(data)
    }
    pub fn dashboard(&self)->R<Dashboard> {
        let conn=self.db()?;
        let conf=config_with(&conn)?;
        let date=Local::now().date_naive().format("%Y-%m-%d").to_string();
        let absent_today=conn.query_row("SELECT COUNT(*) FROM attendance a JOIN students s ON s.id=a.student_id WHERE a.date=?1 AND a.status='absent' AND s.enrollment='regular'",[date],|r|r.get(0)).map_err(msg)?;
        Ok(Dashboard{
            classes:count(&conn,"SELECT COUNT(*) FROM classes")?,sections:count(&conn,"SELECT COUNT(*) FROM sections")?,
            students:count(&conn,"SELECT COUNT(*) FROM students")?,regular:count(&conn,"SELECT COUNT(*) FROM students WHERE enrollment='regular'")?,
            external:count(&conn,"SELECT COUNT(*) FROM students WHERE enrollment='external'")?,
            attendance_days:count(&conn,"SELECT COUNT(DISTINCT date) FROM attendance")?, absent_today, school_name:conf.school_name
        })
    }
    pub fn export_report(&self,path:&str,section_id:Option<i64>)->R<()> {
        let path=clean(path,"مسار التصدير",2000)?;
        let report=self.report(section_id)?;
        // UTF-8 BOM enables Arabic text display in common spreadsheet applications.
        let mut csv=String::from("\u{feff}الطالب,الصف,الشعبة,نوع الدوام,الغياب,أيام الغياب المكافئة,دروس الغياب,الإجازة,دروس الحضور,التنبيهات\r\n");
        fn csv_cell(s:&str)->String {
            let potentially_formula=s.trim_start().chars().next().map(|c|matches!(c,'='|'+'|'-'|'@'|'\t'|'\r')).unwrap_or(false);
            let safe=if potentially_formula {format!("'{}",s)} else {s.to_owned()};
            format!("\"{}\"",safe.replace('"',"\"\""))
        }
        for r in report {
            let fields=[r.student_name,r.class_name,r.section_name,if r.enrollment=="external"{"انتساب".into()}else{"اعتيادي".into()},
                r.absence_text,format!("{:.2}",r.absent_days),r.absent_lessons.to_string(),r.excused_text,r.present_lessons.to_string(),
                r.alerts.iter().map(|a|a.name.clone()).collect::<Vec<_>>().join(" | ")];
            csv.push_str(&fields.iter().map(|x|csv_cell(x)).collect::<Vec<_>>().join(",")); csv.push_str("\r\n");
        }
        fs::write(path,csv).map_err(msg)
    }
    pub fn save_export(&self, path:&str, content:&str)->R<()> {
        let path=clean(path,"مسار التصدير",2000)?;
        if !path.to_lowercase().ends_with(".csv") { return Err("احفظ الملف بامتداد csv لفتحه في إكسل".into()); }
        if content.len()>2_000_000 { return Err("التقرير أكبر من الحد المسموح".into()); }
        let body=if content.starts_with('\u{feff}') { content.to_owned() } else { format!("\u{feff}{content}") };
        fs::write(path, body).map_err(msg)
    }
    pub fn backup(&self,path:&str)->R<()> {
        let path=clean(path,"مسار النسخة",2000)?;
        let source=self.db()?;
        let mut dest=Connection::open(&path).map_err(msg)?;
        let backup=rusqlite::backup::Backup::new(&source,&mut dest).map_err(msg)?;
        backup.run_to_completion(100,Duration::from_millis(100),None).map_err(msg)
    }
    pub fn restore(&self,path:&str)->R<()> {
        let path=clean(path,"مسار النسخة",2000)?;
        let source=Connection::open_with_flags(path,rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).map_err(msg)?;
        let integrity:String=source.query_row("PRAGMA quick_check",[],|r|r.get(0)).map_err(msg)?;
        if integrity!="ok" {return Err("النسخة الاحتياطية تالفة".into())}
        let schema:i64=source.query_row("SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name IN ('classes','sections','students','attendance','settings','alert_rules','holidays')",[],|r|r.get(0)).map_err(msg)?;
        if schema!=7{return Err("الملف ليس نسخة احتياطية متوافقة مع دفتر الغياب".into())}
        let mut target=self.db()?;
        {
            let backup=rusqlite::backup::Backup::new(&source,&mut target).map_err(msg)?;
            backup.run_to_completion(100,Duration::from_millis(100),None).map_err(msg)?;
        }
        target.pragma_update(None,"foreign_keys","ON").map_err(msg)?;
        Ok(())
    }
}

fn report_with(conn:&Connection,section_id:Option<i64>)->R<Vec<StudentReport>> {
    let conf=config_with(conn)?;
    let mut stmt=conn.prepare(r#"
       SELECT s.id,s.name,c.name,g.name,s.enrollment,
          COALESCE(SUM(CASE WHEN a.status='absent' THEN 1 ELSE 0 END),0),
          COALESCE(SUM(CASE WHEN a.status='excused' THEN 1 ELSE 0 END),0),
          COALESCE(SUM(CASE WHEN a.status='present' THEN 1 ELSE 0 END),0)
       FROM students s JOIN sections g ON g.id=s.section_id JOIN classes c ON c.id=g.class_id
       LEFT JOIN attendance a ON a.student_id=s.id AND a.date>=?1 AND a.date<=?2
       WHERE (?3 IS NULL OR s.section_id=?3)
       GROUP BY s.id ORDER BY c.name,g.name,s.name,s.id
    "#).map_err(msg)?;
    let results=stmt.query_map(params![conf.year_start,conf.year_end,section_id],|r|Ok((
        r.get::<_,i64>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?,r.get::<_,String>(3)?,r.get::<_,String>(4)?,
        r.get::<_,i64>(5)?,r.get::<_,i64>(6)?,r.get::<_,i64>(7)?
    ))).map_err(msg)?;
    let shape=absence_shape(conn,&conf.year_start,&conf.year_end)?;
    let mut report=Vec::new();
    for entry in results {
        let (student_id,student_name,class_name,section_name,enrollment,absent,excused,present)=entry.map_err(msg)?;
        let external=enrollment=="external";
        let counted_absent=if external{0}else{absent};
        let (full_days,partial_lessons,full_excused,partial_excused)=if external{(0,0,0,0)}else{*shape.get(&student_id).unwrap_or(&(0,0,0,0))};
        let days=counted_absent as f64 / conf.lessons_per_day as f64;
        let alerts=if external {vec![]}else{
            conf.alerts.iter().filter(|a|a.enabled && {
                let value=if a.metric=="days" {days} else {counted_absent as f64};
                if a.operator==">" {value>a.threshold} else {value>=a.threshold}
            }).cloned().collect()
        };
        report.push(StudentReport{student_id,student_name,class_name,section_name,enrollment,
            absent_lessons:counted_absent,excused_lessons:if external{0}else{excused},
            present_lessons:if external{0}else{present},absent_days:days,
            full_absent_days:full_days,partial_absent_lessons:partial_lessons,
            absence_text:if external{"معفي".into()}else{status_text("غائب",full_days,partial_lessons)},
            full_excused_days:full_excused,partial_excused_lessons:partial_excused,
            excused_text:if external{"معفي".into()}else{status_text("مجاز",full_excused,partial_excused)},alerts});
    }
    Ok(report)
}

fn absence_shape(conn:&Connection,start:&str,end:&str)->R<HashMap<i64,(i64,i64,i64,i64)>> {
    let mut stmt=conn.prepare("SELECT student_id,COUNT(*),SUM(CASE WHEN status='absent' THEN 1 ELSE 0 END),SUM(CASE WHEN status='excused' THEN 1 ELSE 0 END) FROM attendance WHERE date>=?1 AND date<=?2 GROUP BY student_id,date").map_err(msg)?;
    let rows=stmt.query_map(params![start,end],|r|Ok((r.get::<_,i64>(0)?,r.get::<_,i64>(1)?,r.get::<_,i64>(2)?,r.get::<_,i64>(3)?))).map_err(msg)?;
    let mut map=HashMap::new();
    for row in rows {
        let (id,total,absent,excused)=row.map_err(msg)?;
        if absent==0 && excused==0 {continue;}
        let entry=map.entry(id).or_insert((0,0,0,0));
        if absent>0 { if total>0 && absent==total {entry.0+=1;} else {entry.1+=absent;} }
        if excused>0 { if total>0 && excused==total {entry.2+=1;} else {entry.3+=excused;} }
    }
    Ok(map)
}

fn count_phrase(n:i64,day:bool)->String {
    let (one,two,few,many)=if day {("يوم واحد","يومان","أيام","يوم")} else {("درس واحد","درسان","دروس","درس")};
    match n {1=>one.into(),2=>two.into(),3..=10=>format!("{n} {few}"),_=>format!("{n} {many}")}
}

fn status_text(verb:&str,full_days:i64,partial_lessons:i64)->String {
    if full_days==0 && partial_lessons==0 {return if verb=="مجاز"{"لا إجازة".into()}else{"لا غياب".into()}}
    let mut parts=Vec::new();
    if full_days>0 {parts.push(count_phrase(full_days,true));}
    if partial_lessons>0 {parts.push(count_phrase(partial_lessons,false));}
    format!("{verb} {}",parts.join(" و"))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn setup()->(AppDb,i64,i64) {
        let db=AppDb::memory();let class=db.create_class("الخامس العلمي").unwrap();let section=db.create_section(class,"أ").unwrap();
        (db,class,section)
    }
    fn add(db:&AppDb,section:i64,enrollment:&str)->i64 {
        db.upsert_student(StudentInput{id:None,section_id:section,name:format!("طالب {}",enrollment),code:"".into(),enrollment:enrollment.into(),notes:"".into()}).unwrap()
    }
    #[test]
    fn conversion_and_alert_after_eight_days(){
        let (db,_,section)=setup();let id=add(&db,section,"regular");
        for d in 1..=9 {db.save_sheet(section,&format!("2026-10-{:02}",d),5,vec![AttendanceRowInput{student_id:id,statuses:vec!["absent".into();5]}]).unwrap()}
        let r=db.report(Some(section)).unwrap();
        assert_eq!(r[0].absent_lessons,45);assert_eq!(r[0].absent_days,9.0);assert_eq!(r[0].full_absent_days,9);assert_eq!(r[0].partial_absent_lessons,0);assert_eq!(r[0].absence_text,"غائب 9 أيام");assert_eq!(r[0].alerts[0].name,"تجاوز ٨ أيام");
    }
    #[test]
    fn excused_not_counted_and_partial_absence(){
        let (db,_,section)=setup();let id=add(&db,section,"regular");
        db.save_sheet(section,"2026-10-01",5,vec![AttendanceRowInput{student_id:id,statuses:vec!["absent".into(),"present".into(),"excused".into(),"present".into(),"absent".into()]}]).unwrap();
        let r=db.report(None).unwrap();assert_eq!(r[0].absent_lessons,2);assert_eq!(r[0].excused_lessons,1);assert_eq!(r[0].full_absent_days,0);assert_eq!(r[0].partial_absent_lessons,2);assert_eq!(r[0].absence_text,"غائب درسان");assert_eq!(r[0].full_excused_days,0);assert_eq!(r[0].partial_excused_lessons,1);assert_eq!(r[0].excused_text,"مجاز درس واحد");assert!((r[0].absent_days-0.4).abs()<0.0001);
    }
    #[test]
    fn external_excluded(){
        let (db,_,section)=setup();let id=add(&db,section,"external");
        assert!(db.save_sheet(section,"2026-10-01",5,vec![AttendanceRowInput{student_id:id,statuses:vec!["absent".into();5]}]).is_err());
        assert_eq!(db.report(None).unwrap()[0].absent_days,0.0);
    }
    #[test]
    fn updating_existing_day_replaces_absence(){
        let (db,_,section)=setup();let id=add(&db,section,"regular");
        db.save_sheet(section,"2026-10-01",5,vec![AttendanceRowInput{student_id:id,statuses:vec!["absent".into();5]}]).unwrap();
        db.save_sheet(section,"2026-10-01",5,vec![AttendanceRowInput{student_id:id,statuses:vec!["present".into();5]}]).unwrap();
        assert_eq!(db.report(None).unwrap()[0].absent_lessons,0);
    }
    #[test]
    fn holiday_prevents_attendance(){
        let (db,_,section)=setup();let id=add(&db,section,"regular");
        db.set_holiday("2026-10-01","عطلة مدرسية").unwrap();
        assert!(db.save_sheet(section,"2026-10-01",5,vec![AttendanceRowInput{student_id:id,statuses:vec!["absent".into();5]}]).is_err());
    }
    #[test]
    fn weekly_off_defaults_friday_saturday(){
        let (db,_,section)=setup();let id=add(&db,section,"regular");
        let present=||vec![AttendanceRowInput{student_id:id,statuses:vec!["present".into();5]}];
        assert!(db.save_sheet(section,"2026-10-09",5,present()).is_err());
        db.save_sheet(section,"2026-10-08",5,present()).unwrap();
    }
    #[test]
    fn move_student_keeps_attendance(){
        let (db,class_a,section_a)=setup();
        let section_b=db.create_section(class_a,"ب").unwrap();
        let id=add(&db,section_a,"regular");
        db.save_sheet(section_a,"2026-10-05",5,vec![AttendanceRowInput{student_id:id,statuses:vec!["absent".into();5]}]).unwrap();
        db.move_student(id,section_b).unwrap();
        let student=db.students(section_b).unwrap().into_iter().find(|s|s.id==id).expect("moved");
        assert_eq!(student.section_id,section_b);
        assert_eq!(db.report(Some(section_b)).unwrap()[0].absent_lessons,5);
    }
}
