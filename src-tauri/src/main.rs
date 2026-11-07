#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod store;

use store::{AppDb, AttendanceRowInput, AlertInput, StudentInput};
use tauri::{Manager, State};

type ApiResult<T> = Result<T, String>;

#[tauri::command]
fn get_groups(db: State<'_, AppDb>) -> ApiResult<Vec<store::Group>> { db.groups() }

#[tauri::command]
fn create_class(db: State<'_, AppDb>, name: String) -> ApiResult<i64> { db.create_class(&name) }

#[tauri::command]
fn rename_class(db: State<'_, AppDb>, id: i64, name: String) -> ApiResult<()> { db.rename_class(id, &name) }

#[tauri::command]
fn delete_class(db: State<'_, AppDb>, id: i64) -> ApiResult<()> { db.delete_class(id) }

#[tauri::command]
fn create_section(db: State<'_, AppDb>, class_id: i64, name: String) -> ApiResult<i64> { db.create_section(class_id, &name) }

#[tauri::command]
fn rename_section(db: State<'_, AppDb>, id: i64, name: String) -> ApiResult<()> { db.rename_section(id, &name) }

#[tauri::command]
fn delete_section(db: State<'_, AppDb>, id: i64) -> ApiResult<()> { db.delete_section(id) }

#[tauri::command]
fn get_students(db: State<'_, AppDb>, section_id: i64) -> ApiResult<Vec<store::Student>> { db.students(section_id) }

#[tauri::command]
fn upsert_student(db: State<'_, AppDb>, student: StudentInput) -> ApiResult<i64> { db.upsert_student(student) }

#[tauri::command]
fn delete_student(db: State<'_, AppDb>, id: i64) -> ApiResult<()> { db.delete_student(id) }

#[tauri::command]
fn import_students(db: State<'_, AppDb>, section_id: i64, names: String, enrollment: String) -> ApiResult<u32> { db.import_students(section_id, &names, &enrollment) }

#[tauri::command]
fn get_sheet(db: State<'_, AppDb>, section_id: i64, date: String) -> ApiResult<Vec<store::SheetStudent>> { db.sheet(section_id, &date) }

#[tauri::command]
fn save_sheet(db: State<'_, AppDb>, section_id: i64, date: String, lessons: u32, rows: Vec<AttendanceRowInput>) -> ApiResult<()> {
    db.save_sheet(section_id, &date, lessons, rows)
}

#[tauri::command]
fn clear_sheet(db: State<'_, AppDb>, section_id: i64, date: String) -> ApiResult<()> { db.clear_sheet(section_id, &date) }

#[tauri::command]
fn get_config(db: State<'_, AppDb>) -> ApiResult<store::AppConfig> { db.config() }

#[tauri::command]
fn save_config(db: State<'_, AppDb>, school_name: String, lessons_per_day: u32, year_start: String, year_end: String) -> ApiResult<()> {
    db.save_config(&school_name, lessons_per_day, &year_start, &year_end)
}

#[tauri::command]
fn save_alert(db: State<'_, AppDb>, alert: AlertInput) -> ApiResult<i64> { db.save_alert(alert) }

#[tauri::command]
fn delete_alert(db: State<'_, AppDb>, id: i64) -> ApiResult<()> { db.delete_alert(id) }

#[tauri::command]
fn get_holidays(db: State<'_, AppDb>) -> ApiResult<Vec<store::Holiday>> { db.holidays() }

#[tauri::command]
fn set_holiday(db: State<'_, AppDb>, date: String, name: String) -> ApiResult<()> { db.set_holiday(&date, &name) }

#[tauri::command]
fn delete_holiday(db: State<'_, AppDb>, date: String) -> ApiResult<()> { db.delete_holiday(&date) }

#[tauri::command]
fn get_report(db: State<'_, AppDb>, section_id: Option<i64>) -> ApiResult<Vec<store::StudentReport>> { db.report(section_id) }

#[tauri::command]
fn get_history(db: State<'_, AppDb>, student_id: i64) -> ApiResult<Vec<store::HistoryRow>> { db.history(student_id) }

#[tauri::command]
fn get_dashboard(db: State<'_, AppDb>) -> ApiResult<store::Dashboard> { db.dashboard() }

#[tauri::command]
fn export_report(db: State<'_, AppDb>, path: String, section_id: Option<i64>) -> ApiResult<()> { db.export_report(&path, section_id) }

#[tauri::command]
fn save_export(db: State<'_, AppDb>, path: String, content: String) -> ApiResult<()> { db.save_export(&path, &content) }

#[tauri::command]
fn backup_db(db: State<'_, AppDb>, path: String) -> ApiResult<()> { db.backup(&path) }

#[tauri::command]
fn restore_db(db: State<'_, AppDb>, path: String) -> ApiResult<()> { db.restore(&path) }

#[cfg(target_os = "macos")]
struct LivePrint {
    window: tauri::Window,
    operation: usize,
    delegate: usize,
}

#[cfg(target_os = "macos")]
fn live_print() -> &'static std::sync::Mutex<Option<LivePrint>> {
    static LIVE: std::sync::OnceLock<std::sync::Mutex<Option<LivePrint>>> = std::sync::OnceLock::new();
    LIVE.get_or_init(|| std::sync::Mutex::new(None))
}

#[tauri::command]
fn print_page(window: tauri::Window) -> ApiResult<()> {
    #[cfg(target_os = "macos")]
    {
        let failed = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let flag = failed.clone();
        let window_for_print = window.clone();
        window
            .with_webview(move |webview| {
                if !macos_print(webview.inner(), webview.ns_window(), window_for_print) {
                    flag.store(true, std::sync::atomic::Ordering::Relaxed);
                }
            })
            .map_err(|error| error.to_string())?;
        if failed.load(std::sync::atomic::Ordering::Relaxed) {
            return Err("تعذر فتح نافذة الطباعة".into());
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = window;
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn print_delegate_class() -> &'static objc::runtime::Class {
    use objc::{class, sel, sel_impl};
    use std::sync::OnceLock;
    static CLASS: OnceLock<&'static objc::runtime::Class> = OnceLock::new();
    CLASS.get_or_init(|| unsafe {
        let mut decl = objc::declare::ClassDecl::new("AttendancePrintDelegate", class!(NSObject))
            .expect("تعذر تسجيل مفوض الطباعة");
        decl.add_method(
            sel!(printOperationDidRun:success:contextInfo:),
            print_operation_did_run as extern "C" fn(
                &mut objc::runtime::Object,
                objc::runtime::Sel,
                cocoa::base::id,
                cocoa::base::BOOL,
                *mut std::ffi::c_void,
            ),
        );
        decl.register()
    })
}

#[cfg(target_os = "macos")]
extern "C" fn print_operation_did_run(
    _: &mut objc::runtime::Object,
    _: objc::runtime::Sel,
    _: cocoa::base::id,
    _: cocoa::base::BOOL,
    _: *mut std::ffi::c_void,
) {
    let live = live_print().lock().expect("print state").take();
    if let Some(live) = live {
        let _ = live.window.eval("window.__restoreAfterPrint&&window.__restoreAfterPrint()");
        unsafe {
            use cocoa::base::id;
            use objc::{msg_send, sel, sel_impl};
            let operation = live.operation as id;
            let delegate = live.delegate as id;
            // Release on the next turn. Releasing inside AppKit's callback can free the panel still on the stack.
            let () = msg_send![operation, performSelector: sel!(release) withObject: cocoa::base::nil afterDelay: 0.0_f64];
            let () = msg_send![delegate, performSelector: sel!(release) withObject: cocoa::base::nil afterDelay: 0.0_f64];
        }
    }
}

#[cfg(target_os = "macos")]
fn macos_print(webview: cocoa::base::id, ns_window: cocoa::base::id, window: tauri::Window) -> bool {
    unsafe {
        use cocoa::base::{id, nil, BOOL, YES};
        use objc::{class, msg_send, sel, sel_impl};

        if live_print().lock().expect("print state").is_some() {
            return false;
        }
        let can_print: BOOL = msg_send![webview, respondsToSelector: sel!(printOperationWithPrintInfo:)];
        if can_print != YES {
            return false;
        }
        // sharedPrintInfo is already initialized. Calling init again makes the print panel fail to appear.
        let print_info: id = msg_send![class!(NSPrintInfo), sharedPrintInfo];
        if print_info == nil {
            return false;
        }
        let operation: id = msg_send![webview, printOperationWithPrintInfo: print_info];
        if operation == nil {
            return false;
        }
        let delegate: id = msg_send![print_delegate_class(), new];
        let () = msg_send![operation, retain];
        let () = msg_send![operation, setShowsPrintPanel: YES];
        *live_print().lock().expect("print state") = Some(LivePrint {
            window,
            operation: operation as usize,
            delegate: delegate as usize,
        });
        // Return immediately. A nested run loop here freezes the print panel because its events
        // are not delivered until this command returns to the main loop.
        let () = msg_send![
            operation,
            runOperationModalForWindow: ns_window
            delegate: delegate
            didRunSelector: sel!(printOperationDidRun:success:contextInfo:)
            contextInfo: nil
        ];
        true
    }
}

fn main() {
    tauri::Builder::default()
        .setup(|app| {
            let dir = app.path_resolver().app_data_dir().ok_or("تعذر تحديد مجلد البيانات")?;
            std::fs::create_dir_all(&dir)?;
            app.manage(AppDb::open(dir.join("attendance.sqlite3")).map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_groups, create_class, rename_class, delete_class,
            create_section, rename_section, delete_section,
            get_students, upsert_student, delete_student, import_students,
            get_sheet, save_sheet, clear_sheet, get_config, save_config,
            save_alert, delete_alert, get_holidays, set_holiday, delete_holiday,
            get_report, get_history, get_dashboard, export_report, save_export, backup_db, restore_db, print_page
        ])
        .run(tauri::generate_context!())
        .expect("application initialization failed");
}
