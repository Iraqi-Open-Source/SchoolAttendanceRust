# بناء ملف EXE و APP تلقائياً من السورس

> **مهم:** الحزمة المرفقة **لا تحتوي ملف EXE أو APP مبنياً**. تحتوي ملفات كود وإجراءات بناء متاحة للتشغيل على GitHub Actions. لم يجر تشغيل هذه الإجراءات على GitHub أو اختبار البرامج النهائية هنا.

## باستخدام GitHub Actions (أسهل طريقة)

1. أنشئ مستودع GitHub (يمكن جعله Private).
2. ارفع **كل ملفات مجلد SchoolAttendanceRust** إلى جذر المستودع، بما فيها مجلد `.github` المخفي، بحيث يكون الملف `.github/workflows/build-desktop.yml` في هذا المسار بالضبط.
3. من تبويب **Actions** اختر **Build School Attendance (Windows and macOS)** ثم **Run workflow**.
4. بعد نجاح **جميع مراحل البناء** افتح نتيجة التشغيل وضمن **Artifacts** تجد:
   - `SchoolAttendance-Windows-x64-EXE` وفيه ملف `SchoolAttendance-Windows-x64.exe`.
   - `SchoolAttendance-macOS-arm64-APP` وفيه `SchoolAttendance.app` داخل ZIP لمعالجات Apple Silicon.
   - `SchoolAttendance-macOS-intel-APP` وفيه `SchoolAttendance.app` داخل ZIP لمعالجات Intel.
5. نزّل الملفات على النظام المطلوب واختبرها قبل الاستخدام الفعلي.

**ماذا لو فشل البناء؟** المشروع غير مُجمّع أو مُختبَر فعلياً بعد، وقد تتطلب تبعيات Rust/Tauri القديمة تعديلات عند بناء نسخة اليوم. راجع رسالة الخطأ في Actions. لا تفترض أن ملفات البرامج موجودة إلا إذا اكتملت مهام البناء وظهرت Artifacts.

## ملاحظات تشغيل

- إصدار Windows المستهدف x64. لا يمكن ضمان Windows 7 إلا بعد اختباره على Windows 7 SP1 ومكتبة WebView2 Runtime 109 المناسبة؛ Windows 7 لم يعد مدعوماً من مايكروسوفت.
- ملف macOS ينتج على نظام Apple فعلي في GitHub Actions؛ محزّم `SchoolAttendance.app` موقّع بتوقيع محلي **ad-hoc** وليس موثقاً لدى Apple. قد يعترض Gatekeeper على فتح تطبيق منزّل من الإنترنت؛ التوزيع العام يحتاج Developer ID وتوثيق notarization.
- إذا كنت تريد إنشاء `.exe` مباشرة على Windows أو `.app` مباشرة على Mac، تتوفر الأوامر المحلية في `scripts/build-windows.ps1` و`scripts/build-macos.sh` (تستخدم CLI لـ Tauri).
- الملفات المضغوطة الخاصة بتطبيق Mac تحفظ صلاحيات التنفيذ وبنية التطبيق؛ لا تسحب الملف التنفيذي من داخل الحزمة لتشغيله خارج `.app`.
- قاعدة SQLite داخل مجلد بيانات التطبيق على الجهاز، ولن تُنشأ إلا عند تشغيل التطبيق.

## سبب عدم تقديم ملفات تنفيذية داخل هذا التسليم

بيئة تجهيز هذه الحزمة تعمل على Linux ولا تحتوي على Rust/Cargo أو أدوات بناء Windows/macOS، ولا يتوفر تنزيلها هنا. لذلك لم يكن ممكناً بناء ملفات تنفيذية حقيقية وفحصها. لا توجد ملفات تنفيذية وهمية أو مجلدات `.app` مزيفة في هذه الحزمة.
