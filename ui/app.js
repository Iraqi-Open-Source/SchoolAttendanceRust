'use strict';
// Vanilla JavaScript keeps the app offline and compatible with the Windows 7 WebView2 109 engine.
const $ = id => document.getElementById(id);
const safe = value => String(value == null ? '' : value).replace(/[&<>"']/g, c => ({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]));
const localDate = () => { const d = new Date(); const p = n => String(n).padStart(2,'0'); return `${d.getFullYear()}-${p(d.getMonth()+1)}-${p(d.getDate())}`; };
const names = {present:'حاضر', absent:'غائب', excused:'مجاز'};
const signs = {present:'✓', absent:'✕', excused:'◷'};
const nextStatus = status => status === 'present' ? 'absent' : status === 'absent' ? 'excused' : 'present';
const attendanceMark = status => `<span class="pill ${status==='absent'?'external':'regular'}">${safe(names[status])}</span>`;
let groups = [], config = null, page = 'home', sheet = [], sheetDirty = false, sheetRequest = 0;
let students = [], reports = [], holidays = [];
const studentPick = new Set();
let studentReport = null;
let toastTimer = null;
const ATT_PREFS_KEY = 'attendance.prefs';
const WEEKDAY_AR = ['الأحد','الاثنين','الثلاثاء','الأربعاء','الخميس','الجمعة','السبت'];
const DEFAULT_WEEKLY_OFF = [5, 6];

async function api(command, args={}) {
  if (!window.__TAURI__ || !window.__TAURI__.tauri) throw new Error('شغّل هذا البرنامج من نافذة تطبيق Tauri وليس من المتصفح.');
  return window.__TAURI__.tauri.invoke(command, args);
}
function toast(message, error=false) {
  const t=$('toast'); t.textContent=message; t.className='toast' + (error?' error':'');
  clearTimeout(toastTimer); toastTimer=setTimeout(()=>t.classList.add('hidden'),4300);
}
async function run(fn) { try { await fn(); } catch(e) { console.error(e); toast(typeof e==='string'?e:(e.message||String(e)),true); } }
function empty(message, symbol='◌') {return `<div class="empty"><span class="symbol">${symbol}</span>${safe(message)}</div>`;}
function table(headers, body, className='') {
  return `<div class="sheet-wrap"><table class="data-table ${className}"><thead><tr>${headers.map(h=>`<th>${safe(h)}</th>`).join('')}</tr></thead><tbody>${body}</tbody></table></div>`;
}
function chosen(id) {return Number($(id).value)||0;}
function selectedGroup(classId, sectionId) {
  const c=groups.find(g=>g.id===classId), s=c&&c.sections.find(x=>x.id===sectionId);
  return {name:c?c.name:'',section:s?s.name:''};
}
function sectionOptions(cId, selectId, preferred) {
  const cls=groups.find(g=>g.id===Number($(cId).value));
  const s=$(selectId); const prev=preferred!==undefined?String(preferred):s.value;
  s.innerHTML=(cls?cls.sections:[]).map(x=>`<option value="${x.id}">${safe(x.name)}</option>`).join('');
  if([...s.options].some(o=>o.value===prev))s.value=prev;
}
function setClasses(selectId, preserve=true) {
  const el=$(selectId), selected=preserve?el.value:null;
  el.innerHTML=groups.map(x=>`<option value="${x.id}">${safe(x.name)}</option>`).join('');
  if(selected && [...el.options].some(o=>o.value===selected)) el.value=selected;
}
function setAllSelectors() {
  for(const id of ['att-class','students-class','section-parent'])setClasses(id);
  sectionOptions('att-class','att-section');sectionOptions('students-class','students-section');
  const prev=$('report-section').value;
  $('report-section').innerHTML=`<option value="">جميع الشعب</option>`+groups.flatMap(g=>g.sections.map(s=>`<option value="${s.id}">${safe(g.name)} / ${safe(s.name)}</option>`)).join('');
  if([...$('report-section').options].some(o=>o.value===prev))$('report-section').value=prev;
}
async function refreshGroups() { groups=await api('get_groups');setAllSelectors();renderClasses(); }
async function refreshConfig() {
  config=await api('get_config');
  $('school-label').textContent=config.school_name;
  $('school-name').value=config.school_name; $('settings-lessons').value=config.lessons_per_day;
  $('year-start').value=config.year_start; $('year-end').value=config.year_end;
  $('tip-lessons').textContent=config.lessons_per_day;
  $('att-lessons').value=config.lessons_per_day;
  $('report-period').textContent=`من ${config.year_start} إلى ${config.year_end} • ${config.lessons_per_day} دروس = يوم واحد`;
  const off=new Set((config.weekly_off_days&&config.weekly_off_days.length)?config.weekly_off_days:DEFAULT_WEEKLY_OFF);
  document.querySelectorAll('.week-off').forEach(el=>{el.checked=off.has(Number(el.value));});
  renderAlerts();
  if(page==='attendance')updateAttendanceDateNotice();
}
function weeklyOffDays(){return (config&&config.weekly_off_days&&config.weekly_off_days.length)?config.weekly_off_days:DEFAULT_WEEKLY_OFF;}
function weekdayFromDate(date){return new Date(`${date}T12:00:00`).getDay();}
function isWeeklyOffDate(date){if(!date)return false;return weeklyOffDays().includes(weekdayFromDate(date));}
const titles={home:'لوحة المتابعة',attendance:'تسجيل الغيابات',students:'إدارة الطلاب',classes:'الصفوف والشعب',reports:'التقارير والإحصائيات',alerts:'قواعد التنبيه',holidays:'العطل المدرسية',settings:'الإعدادات والنسخ الاحتياطي'};
async function showPage(next) {
  if(page==='attendance' && next!==page && sheetDirty && !confirm('توجد تعديلات غير محفوظة في دفتر الحضور. هل تريد مغادرة الصفحة؟'))return;
  page=next;document.querySelectorAll('.page').forEach(el=>el.classList.toggle('active',el.id==='page-'+next));
  document.querySelectorAll('.nav-item').forEach(el=>el.classList.toggle('active',el.dataset.page===next));
  $('page-title').textContent=titles[next];
  if(next==='home') await refreshHome();
  if(next==='attendance'){await refreshHolidaysCache();updateAttendanceDateNotice();await loadSheet();}
  if(next==='students') await loadStudents();
  if(next==='classes')renderClasses();
  if(next==='reports')await loadReports();
  if(next==='alerts')renderAlerts();
  if(next==='holidays')await loadHolidays();
}
function renderStat(label,value,description,icon) {
  return `<div class="stat"><div class="icon">${icon}</div><div class="label">${safe(label)}</div><div class="value">${safe(value)}</div><div class="description">${safe(description)}</div></div>`;
}
async function refreshHome() {
  const [dash, full]=await Promise.all([api('get_dashboard'),api('get_report', {sectionId:null})]);
  $('school-label').textContent=dash.school_name;
  const alerted=full.filter(s=>s.alerts.length);
  $('dashboard-cards').innerHTML=renderStat('مجموع الطلاب',dash.students,`${dash.regular} اعتيادي / ${dash.external} انتساب`,'♙')+
    renderStat('الصفوف الدراسية',dash.classes,`${dash.sections} شعبة مسجلة`,'▤')+
    renderStat('دروس الغياب اليوم',dash.absent_today,'عدد الحصص المسجلة غياباً','✕')+
    renderStat('تنبيهات فعّالة',alerted.length,'طلاب تجاوزوا أحد الحدود','⚑');
  $('home-alerts').innerHTML=alerted.length?table(['الطالب','الصف والشعبة','الغياب','التنبيهات'],
    alerted.slice(0,12).map(r=>`<tr><td class="student-cell">${safe(r.student_name)}</td><td>${safe(r.class_name)} / ${safe(r.section_name)}</td><td><span class="absence-phrase">${safe(r.absence_text)}</span><small class="absence-sub">${r.absent_days.toFixed(2)} يوم مكافئ • ${r.absent_lessons} درس</small></td><td>${alertTags(r.alerts)}</td></tr>`).join('')):
    empty('لا توجد تنبيهات متجاوزة حتى الآن — ممتاز!','✓');
}
function alertTags(alerts) { return alerts.map(a=>`<span class="alert-chip" style="color:${a.color};background:${a.color}18;border:1px solid ${a.color}50">${safe(a.name)}</span>`).join(''); }

async function refreshHolidaysCache(){holidays=await api('get_holidays');if(page==='attendance')updateAttendanceDateNotice();}
function holidayOnDate(date){return holidays.find(h=>h.date===date);}
function attendanceBlocked(date){return Boolean(holidayOnDate(date))||isWeeklyOffDate(date);}
function updateAttendanceDateNotice(){
  const date=$('att-date').value;
  const notice=$('att-date-notice');
  const save=$('att-save');
  const clearBtn=$('att-clear');
  if(!date||!notice){return;}
  const holiday=holidayOnDate(date);
  const weekday=weekdayFromDate(date);
  const weeklyOff=isWeeklyOffDate(date);
  const parts=[];
  if(holiday)parts.push(`عطلة رسمية: ${holiday.name} — لا يمكن حفظ الدوام لهذا اليوم.`);
  if(weeklyOff)parts.push(`عطلة أسبوعية (${WEEKDAY_AR[weekday]}) — لا يمكن حفظ الدوام لهذا اليوم.`);
  const blocked=Boolean(holiday||weeklyOff);
  if(parts.length){
    notice.textContent=parts.join(' ');
    notice.className='att-date-notice'+(blocked?' blocked':' warn');
  }else{
    notice.textContent='';
    notice.className='att-date-notice hidden';
  }
  if(save)save.disabled=blocked;
  if(clearBtn)clearBtn.disabled=blocked;
}
function saveAttPrefs(){
  try{
    localStorage.setItem(ATT_PREFS_KEY,JSON.stringify({
      classId:$('att-class').value,
      sectionId:$('att-section').value,
      date:$('att-date').value
    }));
  }catch(_){/* ignore */}
}
function restoreAttPrefs(){
  try{
    const raw=localStorage.getItem(ATT_PREFS_KEY);
    if(!raw)return;
    const prefs=JSON.parse(raw);
    if(prefs.classId && [...$('att-class').options].some(o=>o.value===prefs.classId)){
      $('att-class').value=prefs.classId;
      sectionOptions('att-class','att-section',prefs.sectionId);
    }
    if(prefs.date) $('att-date').value=prefs.date;
  }catch(_){/* ignore */}
}
function focusSheetCell(studentId,lesson){
  const button=$('att-table').querySelector(`.status-button[data-action="cycle"][data-id="${studentId}"][data-lesson="${lesson}"]`);
  if(button)button.focus();
}
function handleSheetKeydown(event){
  if(page!=='attendance')return;
  const button=event.target.closest('.status-button[data-action="cycle"]');
  if(!button)return;
  const id=Number(button.dataset.id);
  const lesson=Number(button.dataset.lesson);
  const regular=sheet.filter(row=>row.student.enrollment==='regular');
  const rowIndex=regular.findIndex(row=>row.student.id===id);
  const lessons=currentLessonCount();
  if(event.key===' '||event.key==='Enter'){
    event.preventDefault();
    const row=sheet.find(x=>x.student.id===id);
    if(row)modifyLesson(id,lesson,nextStatus(row.statuses[lesson]));
  }else if(event.key==='ArrowLeft'&&lesson>0){
    event.preventDefault();focusSheetCell(id,lesson-1);
  }else if(event.key==='ArrowRight'&&lesson<lessons-1){
    event.preventDefault();focusSheetCell(id,lesson+1);
  }else if(event.key==='ArrowDown'&&rowIndex>=0&&rowIndex<regular.length-1){
    event.preventDefault();focusSheetCell(regular[rowIndex+1].student.id,lesson);
  }else if(event.key==='ArrowUp'&&rowIndex>0){
    event.preventDefault();focusSheetCell(regular[rowIndex-1].student.id,lesson);
  }
}

function currentLessonCount() {const n=Number($('att-lessons').value);return Number.isInteger(n)&&n>=1&&n<=20?n:0;}
async function loadSheet() {
  if(sheetDirty && !confirm('ستفقد تغييرات الحضور غير المحفوظة. هل تتابع؟'))return;
  const sectionId=chosen('att-section'), date=$('att-date').value, lessons=currentLessonCount();
  if(!sectionId){sheet=[];$('att-table').innerHTML=empty('أنشئ صفاً وشعبة وأضف الطلاب أولاً');$('att-summary').textContent='';return;}
  if(!lessons){toast('حدد عدد دروس صحيحاً من 1 إلى 20',true);return;}
  const request=++sheetRequest;
  const rows=await api('get_sheet',{sectionId,date});
  if(request!==sheetRequest)return;
  sheet=rows.map(r=>({...r,statuses:Array.from({length:lessons},(_,i)=>r.statuses[String(i+1)]||'present')}));
  sheetDirty=false;updateAttendanceDateNotice();renderSheet();
}
function markDirty(){sheetDirty=true;$('att-dirty').textContent='● تغييرات لم تُحفظ';}
function renderSheet() {
  $('att-dirty').textContent=sheetDirty?'● تغييرات لم تُحفظ':'✓ كل التعديلات محفوظة';
  const regular=sheet.filter(x=>x.student.enrollment==='regular');
  const excluded=sheet.length-regular.length;
  const lessons=currentLessonCount();
  if(!regular.length){$('att-table').innerHTML=empty('لا يوجد طلاب دوام اعتيادي في هذه الشعبة');$('att-summary').textContent=`طلاب الانتساب المعفيون: ${excluded}`;return;}
  const headers=['الطالب',...Array.from({length:lessons},(_,i)=>`الدرس ${i+1}`),'طوال اليوم'];
  const body=regular.map((r,index)=>`<tr><td><span class="student-cell">${safe(r.student.name)}</span>${r.student.code?`<div class="muted small">${safe(r.student.code)}</div>`:''}</td>`+
    r.statuses.slice(0,lessons).map((status,lesson)=>`<td class="status-cell"><button type="button" class="status-button ${status}" data-action="cycle" data-id="${r.student.id}" data-lesson="${lesson}" title="اضغط أو Enter/مسافة للتبديل • الأسهم للتنقل">${signs[status]} ${safe(names[status])}</button></td>`).join('')+
    `<td><div class="small-action"><button title="حاضر طوال اليوم" data-action="row-status" data-id="${r.student.id}" data-status="present">✓</button><button title="غائب طوال اليوم" data-action="row-status" data-id="${r.student.id}" data-status="absent">✕</button><button title="مجاز طوال اليوم" data-action="row-status" data-id="${r.student.id}" data-status="excused">◷</button></div></td></tr>`).join('');
  $('att-table').innerHTML=table(headers,body,'sheet-table');
  const totals=regular.flatMap(r=>r.statuses.slice(0,lessons));
  $('att-summary').textContent=`${regular.length} طالب اعتيادي • ${excluded} طالب انتساب (معفي) • ${totals.filter(s=>s==='present').length} حضور درس • ${totals.filter(s=>s==='absent').length} غياب درس • ${totals.filter(s=>s==='excused').length} إجازة درس`;
}
async function saveSheet() {
  const sectionId=chosen('att-section'),date=$('att-date').value,lessons=currentLessonCount();
  if(!sectionId||!lessons){toast('اختر الشعبة وعدد الدروس',true);return;}
  if(holidayOnDate(date)){toast('هذا اليوم مسجل عطلة؛ لا يمكن حفظ الدوام',true);return;}
  if(isWeeklyOffDate(date)){toast('هذا اليوم ضمن العطل الأسبوعية؛ لا يمكن حفظ الدوام',true);return;}
  saveAttPrefs();
  const rows=sheet.filter(r=>r.student.enrollment==='regular').map(r=>({student_id:r.student.id,statuses:r.statuses.slice(0,lessons)}));
  if(!rows.length){toast('لا يوجد طلاب اعتياديون',true);return;}
  await api('save_sheet',{sectionId,date,lessons,rows});sheetDirty=false;renderSheet();toast('تم حفظ دفتر الحضور بنجاح');
}
function modifyLesson(id,lesson,status) {
  const student=sheet.find(r=>r.student.id===id);
  if(!student)return;
  if(lesson===null)student.statuses=Array.from({length:currentLessonCount()},()=>status);
  else student.statuses[lesson]=status||nextStatus(student.statuses[lesson]);
  markDirty();renderSheet();
}
function allStatuses(status){sheet.filter(r=>r.student.enrollment==='regular').forEach(r=>r.statuses=Array.from({length:currentLessonCount()},()=>status));markDirty();renderSheet();}
function lessonCountChanged(){
  const lessons=currentLessonCount();if(!lessons)return;
  sheet.forEach(r=>r.statuses=Array.from({length:lessons},(_,i)=>r.statuses[i]||'present'));
  if(sheet.length)markDirty();renderSheet();
}

function renderClasses() {
  $('classes-list').innerHTML=groups.length?groups.map(g=>`<div class="record"><div><strong>${safe(g.name)}</strong><small>${g.sections.length} شعبة</small></div><div class="buttons"><button data-action="rename-class" data-id="${g.id}">تعديل</button><button class="delete" data-action="delete-class" data-id="${g.id}">حذف</button></div></div>`).join(''):empty('لم تضف صفوفاً بعد');
  const classId=chosen('section-parent');const c=groups.find(g=>g.id===classId);
  $('sections-list').innerHTML=c&&c.sections.length?c.sections.map(s=>`<div class="record"><div><strong>الشعبة ${safe(s.name)}</strong></div><div class="buttons"><button data-action="rename-section" data-id="${s.id}">تعديل</button><button class="delete" data-action="delete-section" data-id="${s.id}">حذف</button></div></div>`).join(''):empty('اختر صفاً ثم أضف شعبه');
}
function filteredStudentList(){
  const query=$('students-search').value.trim();
  return students.filter(s=>!query||`${s.name} ${s.code}`.includes(query));
}
function updateStudentTransferUi(){
  const bulk=$('students-bulk-transfer');
  if(bulk)bulk.disabled=studentPick.size===0;
  const pickAll=$('students-pick-all');
  if(!pickAll)return;
  const visible=filteredStudentList();
  const picked=visible.filter(s=>studentPick.has(s.id)).length;
  pickAll.checked=visible.length>0&&picked===visible.length;
  pickAll.indeterminate=picked>0&&picked<visible.length;
}
async function loadStudents(){
  const id=chosen('students-section');
  students=id?await api('get_students',{sectionId:id}):[];
  for(const sid of [...studentPick]) if(!students.some(s=>s.id===sid))studentPick.delete(sid);
  renderStudents();
}
function renderStudents() {
  const filtered=filteredStudentList();
  $('students-table').innerHTML=filtered.length?table(['','#','اسم الطالب','الرمز','نوع الدوام','الملاحظات','الإجراءات'],filtered.map((s,i)=>`<tr><td class="student-pick-cell"><input type="checkbox" class="student-pick" data-id="${s.id}" aria-label="تحديد ${safe(s.name)}"${studentPick.has(s.id)?' checked':''} /></td><td>${i+1}</td><td class="student-cell">${safe(s.name)}</td><td>${safe(s.code||'—')}</td><td><span class="pill ${s.enrollment==='external'?'external':'regular'}">${s.enrollment==='external'?'انتساب • معفي':'اعتيادي'}</span></td><td>${safe(s.notes||'—')}</td><td class="no-wrap"><button type="button" class="btn mini outline" data-action="edit-student" data-id="${s.id}">تعديل</button> <button type="button" class="btn mini subtle" data-action="move-student" data-id="${s.id}">ترحيل</button> <button type="button" class="btn mini danger-outline" data-action="delete-student" data-id="${s.id}">حذف</button></td></tr>`).join('')):empty('لا توجد أسماء مطابقة في هذه الشعبة');
  updateStudentTransferUi();
}
function resetStudent(){
  $('student-form').reset();$('student-id').value='';$('student-submit').textContent='إضافة الطالب';$('student-cancel').classList.add('hidden');
}
async function submitStudent(event){event.preventDefault();const sectionId=chosen('students-section');if(!sectionId)throw new Error('أضف شعبة أولاً');
  const studentId=chosen('student-id')||null;
  const previous=studentId?students.find(s=>s.id===studentId):null;
  const enrollment=$('student-enrollment').value;
  if(previous&&previous.enrollment==='regular'&&enrollment==='external'&&!confirm('سيُعفى الطالب من الدوام والتنبيهات لكن سجلّه السابق يبقى. هل تتابع؟'))return;
  const student={id:studentId,section_id:sectionId,name:$('student-name').value,code:$('student-code').value,enrollment,notes:$('student-notes').value};
  await api('upsert_student',{student});resetStudent();await loadStudents();toast('تم حفظ بيانات الطالب');
}
async function exportStudentsCsv(){
  const sectionId=chosen('students-section');
  if(!sectionId)throw new Error('اختر الشعبة أولاً');
  const group=selectedGroup(chosen('students-class'),sectionId);
  const label=`${group.name}-${group.section}`.replace(/[\\/:*?"<>|]/g,' ').trim()||'طلاب';
  const path=await window.__TAURI__.dialog.save({defaultPath:`قائمة-${label}.csv`,filters:[{name:'Excel CSV',extensions:['csv']}]});
  if(!path)return;
  let csv=csvLine(['الاسم','الرمز','نوع الدوام','ملاحظات']);
  for(const s of students) csv+=csvLine([s.name,s.code||'',s.enrollment==='external'?'انتساب':'اعتيادي',s.notes||'']);
  await api('save_export',{path,content:csv});
  toast('تم تصدير قائمة الطلاب');
}
function sectionLabel(sectionId){
  for(const group of groups){
    const section=group.sections.find(x=>x.id===sectionId);
    if(section)return `${group.name} / ${section.name}`;
  }
  return '';
}
function transferSectionOptions(excludeSectionId){
  return groups.flatMap(group=>group.sections
    .filter(section=>section.id!==excludeSectionId)
    .map(section=>`<option value="${section.id}">${safe(group.name)} / ${safe(section.name)}</option>`)).join('');
}
function openMoveStudent(id){
  const student=students.find(s=>s.id===id);
  if(!student)return;
  const options=transferSectionOptions(student.section_id);
  if(!options){toast('لا توجد شعبة أخرى للترحيل',true);return;}
  modal('ترحيل الطالب',`<p class="muted">ترحيل <b>${safe(student.name)}</b> مع الإبقاء على سجل الغياب.</p><label>الشعبة الجديدة<select id="move-target">${options}</select></label><div class="action-row" style="margin-top:14px"><button type="button" class="btn primary" data-action="confirm-move" data-id="${id}">تأكيد الترحيل</button></div>`);
}
function openBulkMoveStudents(){
  const sectionId=chosen('students-section');
  if(!sectionId){toast('اختر الشعبة أولاً',true);return;}
  const ids=[...studentPick];
  if(!ids.length){toast('حدد طالباً واحداً على الأقل',true);return;}
  const options=transferSectionOptions(sectionId);
  if(!options){toast('لا توجد شعبة أخرى للترحيل',true);return;}
  modal('ترحيل جماعي',`<p class="muted">ترحيل <b>${ids.length}</b> طالب مع الإبقاء على سجل الغياب.</p><label>الشعبة الجديدة<select id="move-target">${options}</select></label><div class="action-row" style="margin-top:14px"><button type="button" class="btn primary" data-action="confirm-bulk-move">تأكيد الترحيل</button></div>`);
}
async function executeBulkTransfer(targetSectionId,ids){
  for(const sid of ids) await api('move_student',{id:sid,sectionId:targetSectionId});
  ids.forEach(id=>studentPick.delete(id));
}
async function importStudents(){
  const sectionId=chosen('students-section');if(!sectionId)throw new Error('اختر الشعبة أولاً');
  const n=await api('import_students',{sectionId,names:$('import-names').value,enrollment:$('import-type').value});
  $('import-area').classList.add('hidden');$('import-names').value='';await loadStudents();toast(`تمت إضافة ${n} طالب`);
}

async function loadReports(){reports=await api('get_report',{sectionId:chosen('report-section')||null});renderReports();}
function filteredReports(){
  const q=$('report-search').value.trim();
  let rows=reports.filter(x=>!q||x.student_name.includes(q));
  if($('report-alert-only').checked) rows=rows.filter(s=>s.alerts.length);
  const sort=$('report-sort').value;
  rows=[...rows].sort((a,b)=>{
    if(sort==='name')return a.student_name.localeCompare(b.student_name,'ar');
    if(sort==='lessons-desc')return b.absent_lessons-a.absent_lessons||a.student_name.localeCompare(b.student_name,'ar');
    return b.absent_days-a.absent_days||a.student_name.localeCompare(b.student_name,'ar');
  });
  return rows;
}
function printSectionSummary(){
  const rows=filteredReports();
  const sectionId=chosen('report-section');
  const scope=sectionId?sectionLabel(sectionId):'جميع الشعب';
  const period=config?`${config.year_start} — ${config.year_end}`:'';
  const bodyRows=rows.length?rows.map(r=>`<tr><td>${safe(r.student_name)}</td><td>${safe(r.class_name)} / ${safe(r.section_name)}</td><td>${r.enrollment==='external'?'انتساب':safe(r.absence_text)}</td><td>${r.enrollment==='external'?'—':r.absent_days.toFixed(2)}</td><td>${alertTags(r.alerts)||'—'}</td></tr>`).join(''):'<tr><td colspan="5">لا توجد نتائج</td></tr>';
  const html=`<h1>ملخص غياب الشعبة</h1><p class="note">${safe(config?config.school_name:'')} • ${safe(scope)} • ${safe(period)}</p><table><thead><tr><th>الطالب</th><th>الصف / الشعبة</th><th>الغياب</th><th>أيام مكافئة</th><th>التنبيهات</th></tr></thead><tbody>${bodyRows}</tbody></table><p class="note">${rows.length} طالب • ${rows.filter(s=>s.alerts.length).length} لديهم تنبيهات</p>`;
  return printDocument('ملخص غياب الشعبة',html);
}
function renderReports(){
  const rs=filteredReports();
  const regular=rs.filter(s=>s.enrollment==='regular');
  const full=regular.reduce((n,s)=>n+(s.full_absent_days||0),0);
  const partial=regular.reduce((n,s)=>n+(s.partial_absent_lessons||0),0);
  $('reports-table').innerHTML=rs.length?table(['الطالب','الصف / الشعبة','الدوام','الغياب','الإجازة','التنبيهات','تفاصيل'],rs.map(r=>
    `<tr><td class="report-name">${safe(r.student_name)}</td><td>${safe(r.class_name)} / ${safe(r.section_name)}</td><td><span class="pill ${r.enrollment==='external'?'external':'regular'}">${r.enrollment==='external'?'انتساب':'اعتيادي'}</span></td><td>${r.enrollment==='external'?'<span class="pill external">معفي</span>':`<span class="absence-phrase">${safe(r.absence_text)}</span><small class="absence-sub">${r.absent_days.toFixed(2)} يوم مكافئ • ${r.absent_lessons} درس</small>`}</td><td>${r.enrollment==='external'?'—':`<span class="absence-phrase">${safe(r.excused_text)}</span><small class="absence-sub">${r.excused_lessons} درس</small>`}</td><td>${alertTags(r.alerts)}</td><td><button class="btn mini outline" data-action="history" data-id="${r.student_id}">عرض</button></td></tr>`).join('')):empty('لا توجد نتائج');
  $('report-summary').textContent=`${rs.length} طالب • ${absencePhrase(full,partial)} • ${rs.filter(s=>s.alerts.length).length} طالب لديه تنبيهات`;
}
function countPhrase(n,kind){
  const forms=kind==='day'?['يوم واحد','يومان','أيام','يوم']:['درس واحد','درسان','دروس','درس'];
  if(n===1)return forms[0];
  if(n===2)return forms[1];
  if(n>=3&&n<=10)return `${n} ${forms[2]}`;
  return `${n} ${forms[3]}`;
}
function absencePhrase(days,lessons){
  if(!days&&!lessons)return 'لا غياب';
  const parts=[];
  if(days)parts.push(countPhrase(days,'day'));
  if(lessons)parts.push(countPhrase(lessons,'lesson'));
  return `غائب ${parts.join(' و')}`;
}
function lessonList(nums){
  if(nums.length===1)return `الدرس ${nums[0]}`;
  if(nums.length===2)return `الدرسين ${nums[0]} و${nums[1]}`;
  return `الدروس ${nums.join(' و')}`;
}
function groupAbsences(hist){
  const days=[]; let current=null;
  for(const row of hist){
    if(!current||current.date!==row.date){current={date:row.date,rows:[]};days.push(current);}
    current.rows.push(row);
  }
  const lines=[];
  for(const day of days){
    const absent=day.rows.filter(x=>x.status==='absent').map(x=>x.lesson);
    const excused=day.rows.filter(x=>x.status==='excused').map(x=>x.lesson);
    const present=day.rows.filter(x=>x.status==='present').length;
    if(!absent.length&&!excused.length)continue;
    let text,status,full=false;
    if(absent.length===day.rows.length){
      full=true; status='absent';
      text=`غائب اليوم كامل (${countPhrase(day.rows.length,'lesson')})`;
    }else if(excused.length===day.rows.length){
      status='excused';
      text=`مجاز اليوم كامل (${countPhrase(day.rows.length,'lesson')})`;
    }else{
      status=absent.length?'partial':'excused';
      const bits=[];
      if(present)bits.push(`حاضر ${countPhrase(present,'lesson')}`);
      if(absent.length)bits.push(`غائب ${lessonList(absent)}`);
      if(excused.length)bits.push(`مجاز ${lessonList(excused)}`);
      text=bits.join('، و');
    }
    lines.push({date:day.date,text,status,day:full,absentLessons:absent.length});
  }
  return lines;
}
function absenceSummary(r){
  if(r.enrollment==='external')return 'انتساب (معفي من احتساب الغياب)';
  return `${r.absence_text} • يعادل ${r.absent_days.toFixed(2)} يوم • ${r.absent_lessons} درس غياب`;
}
function absenceTable(lines){
  return lines.length?table(['التاريخ','توضيح اليوم'],lines.map(x=>`<tr><td>${safe(x.date)}</td><td>${safe(x.text)}</td></tr>`).join('')):empty('لا توجد غيابات أو إجازات في السنة الحالية');
}
async function showHistory(id){
  const r=reports.find(x=>x.student_id===id);if(!r)return;
  const lines=groupAbsences(await api('get_history',{studentId:id}));
  studentReport={report:r,lines};
  const content=`<p class="muted">${safe(r.class_name)} / ${safe(r.section_name)} • ${safe(absenceSummary(r))}</p>`+
    `<div class="action-row report-actions"><button class="btn mini secondary" data-action="print-summary" data-id="${r.student_id}">طباعة تقرير إجمالي</button><button class="btn mini secondary" data-action="print-detail" data-id="${r.student_id}">طباعة تقرير مفصل</button><button class="btn mini primary" data-action="export-student" data-id="${r.student_id}">تصدير Excel</button></div>`+
    absenceTable(lines);
  modal(`سجل الطالب: ${r.student_name}`,content);
}
function reportFacts(r, lines){
  const alerts=r.alerts.length?r.alerts.map(a=>a.name).join('، '):'لا توجد';
  return [
    ['المدرسة', config?config.school_name:''],
    ['الطالب', r.student_name],
    ['الصف / الشعبة', `${r.class_name} / ${r.section_name}`],
    ['نوع الدوام', r.enrollment==='external'?'انتساب':'اعتيادي'],
    ['الفترة', config?`${config.year_start} إلى ${config.year_end}`:''],
    ['الغياب', r.enrollment==='external'?'معفي':r.absence_text],
    ['أيام الغياب المكافئة', r.enrollment==='external'?'معفي':r.absent_days.toFixed(2)],
    ['دروس الغياب', r.enrollment==='external'?'معفي':String(r.absent_lessons)],
    ['أيام الغياب الكاملة', String(lines.filter(x=>x.day).length)],
    ['دروس الغياب في أيام غير مكتملة', String(lines.reduce((n,x)=>n+(x.day?0:x.absentLessons),0))],
    ['الإجازة', r.enrollment==='external'?'معفي':r.excused_text],
    ['دروس الحضور', String(r.present_lessons)],
    ['التنبيهات', alerts]
  ];
}
function printStudentReport(kind){
  if(!studentReport)return Promise.resolve();
  const {report:r,lines}=studentReport;
  const detailed=kind==='detail';
  const facts=reportFacts(r,lines).map(([k,v])=>`<tr><th>${safe(k)}</th><td>${safe(v)}</td></tr>`).join('');
  const detail=detailed?`<h2>تفاصيل الغياب</h2><table><thead><tr><th>التاريخ</th><th>توضيح اليوم</th></tr></thead><tbody>${lines.length?lines.map(x=>`<tr><td>${safe(x.date)}</td><td>${safe(x.text)}</td></tr>`).join(''):'<tr><td colspan="2">لا توجد غيابات أو إجازات</td></tr>'}</tbody></table>`:'';
  return printDocument(detailed?'تقرير غياب مفصل':'تقرير غياب إجمالي', `<h1>${safe(detailed?'تقرير غياب مفصل':'تقرير غياب إجمالي')} — ${safe(r.student_name)}</h1><table class="facts">${facts}</table>${detail}<p class="note">إذا غاب الطالب في كل دروس اليوم يُكتب «غائب اليوم كامل». وإذا غاب في درس محدد يُذكر رقم الدرس وعدد الدروس التي حضرها في اليوم نفسه.</p>`);
}
function csvCell(value){
  const text=String(value??'');
  const formula=/^[\s]*[=+\-@\t\r]/.test(text);
  return `"${(formula?"'"+text:text).replace(/"/g,'""')}"`;
}
function csvLine(cells){return cells.map(csvCell).join(',')+'\r\n';}
async function exportStudentExcel(){
  if(!studentReport)return;
  const {report:r,lines}=studentReport;
  const path=await window.__TAURI__.dialog.save({defaultPath:`تقرير-غياب-${r.student_name.replace(/[\\/:*?"<>|]/g,' ').trim().slice(0,80)||'طالب'}.csv`,filters:[{name:'Excel CSV',extensions:['csv']}]});
  if(!path)return;
  let csv=csvLine(['البيان','القيمة']);
  for(const [k,v] of reportFacts(r,lines)) csv+=csvLine([k,v]);
  csv+='\r\n'+csvLine(['التاريخ','توضيح اليوم']);
  if(!lines.length) csv+=csvLine(['','لا توجد غيابات أو إجازات']);
  for(const line of lines) csv+=csvLine([line.date,line.text]);
  await api('save_export',{path,content:csv});
  toast('تم تصدير تقرير الطالب إلى ملف يفتح في إكسل');
}
function printDocument(title, body){
  const root=$('print-root');
  const previous=document.title;
  const mac=/Mac/i.test(navigator.userAgent);
  root.innerHTML=body;
  document.title=title;
  if(mac) document.body.classList.add('printing');
  const cleanup=()=>{
    document.title=previous;
    root.innerHTML='';
    document.body.classList.remove('printing');
    window.removeEventListener('afterprint',cleanup);
  };
  window.addEventListener('afterprint',cleanup);
  return new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(resolve))).then(async()=>{
    if(mac){
      window.__restoreAfterPrint=()=>{window.__restoreAfterPrint=null;cleanup();};
      try{await api('print_page');}
      catch(error){cleanup();throw error;}
      return;
    }
    try{
      const pending=window.print();
      if(pending&&typeof pending.then==='function') await pending;
    } finally {
      cleanup();
    }
  });
}

function renderAlerts() {
  if(!config)return;
  $('alerts-list').innerHTML=config.alerts.length?config.alerts.map(a=>`<div class="alert-item" style="--alert-color:${a.color}"><div><div class="alert-name">${safe(a.name)} <span class="pill neutral">${a.enabled?'مفعّل':'متوقف'}</span></div><div class="alert-meta">${a.operator==='>'?'أكثر من':'يساوي أو يتجاوز'} ${a.threshold} ${a.metric==='days'?'يوم مكافئ':'درس غياب'} • اللون ${a.color}</div></div><div class="action-row"><button class="btn mini outline" data-action="edit-alert" data-id="${a.id}">تعديل</button><button class="btn mini danger-outline" data-action="delete-alert" data-id="${a.id}">حذف</button></div></div>`).join(''):empty('لا توجد قواعد تنبيه بعد');
}
function resetAlert(){ $('alert-form').reset();$('alert-id').value='';$('alert-submit').textContent='+ حفظ التنبيه';$('alert-cancel').classList.add('hidden');$('alert-threshold').value='8';$('alert-color').value='#dc454f'; }
async function submitAlert(e){e.preventDefault();const alert={
    id:chosen('alert-id')||null,name:$('alert-name').value,threshold:Number($('alert-threshold').value),metric:$('alert-metric').value,operator:$('alert-operator').value,color:$('alert-color').value,enabled:$('alert-enabled').checked
  };await api('save_alert',{alert});resetAlert();config=await api('get_config');renderAlerts();toast('تم حفظ قاعدة التنبيه');
}
async function loadHolidays(){holidays=await api('get_holidays');renderHolidays();}
function renderHolidays(){
  $('holidays-list').innerHTML=holidays.length?holidays.map(x=>`<div class="record"><div><strong>${safe(x.name)}</strong><small>${safe(x.date)}</small></div><button class="btn mini danger-outline" data-action="delete-holiday" data-date="${safe(x.date)}">إزالة</button></div>`).join(''):empty('لا توجد عطل مسجلة');
}
async function saveHoliday(e){e.preventDefault();await api('set_holiday',{date:$('holiday-date').value,name:$('holiday-name').value});$('holiday-name').value='';await loadHolidays();await refreshHolidaysCache();toast('تم تسجيل العطلة');}
function selectedWeeklyOffDays(){
  return [...document.querySelectorAll('.week-off:checked')].map(el=>Number(el.value));
}
async function saveSettings(e){e.preventDefault();await api('save_config',{schoolName:$('school-name').value,lessonsPerDay:Number($('settings-lessons').value),yearStart:$('year-start').value,yearEnd:$('year-end').value,weeklyOffDays:selectedWeeklyOffDays()});
  await refreshConfig();toast('تم حفظ الإعدادات. أعد فتح دفتر الحضور لتطبيق التغييرات.');}
function modal(title,html){$('modal-title').textContent=title;$('modal-content').innerHTML=html;$('modal-cover').classList.remove('hidden');}
function closeModal(){$('modal-cover').classList.add('hidden');}
async function exportCsv(){
  const path=await window.__TAURI__.dialog.save({defaultPath:'attendance-report.csv',filters:[{name:'CSV Excel',extensions:['csv']}]});
  if(!path)return;await api('export_report',{path,sectionId:chosen('report-section')||null});toast('تم تصدير تقرير CSV بنجاح');
}
async function backupSave(){const path=await window.__TAURI__.dialog.save({defaultPath:'attendance-backup.sqlite3',filters:[{name:'SQLite',extensions:['sqlite3']}]});if(!path)return;await api('backup_db',{path});toast('تم حفظ النسخة الاحتياطية');}
async function backupRestore(){
  const path=await window.__TAURI__.dialog.open({multiple:false,directory:false,filters:[{name:'SQLite',extensions:['sqlite3','db','sqlite']}]});
  if(!path)return;
  if(!confirm('سيتم استبدال كل بيانات البرنامج الحالية بمحتويات النسخة المختارة. هل أنت متأكد؟'))return;
  await api('restore_db',{path});sheetDirty=false;await refreshGroups();await refreshConfig();await refreshHome();toast('تمت استعادة البيانات بنجاح');
}

async function dispatchAction(el){
  const action=el.dataset.action,id=Number(el.dataset.id),date=el.dataset.date;
  if(action==='cycle'){
    const r=sheet.find(x=>x.student.id===id);if(r)modifyLesson(id,Number(el.dataset.lesson),nextStatus(r.statuses[Number(el.dataset.lesson)]));return;
  }
  if(action==='row-status'){modifyLesson(id,null,el.dataset.status);return;}
  if(action==='rename-class'||action==='rename-section'){
    const cls=action==='rename-class',record=cls?groups.find(g=>g.id===id):groups.flatMap(g=>g.sections).find(s=>s.id===id);
    if(!record)return;const name=prompt('أدخل الاسم الجديد',record.name);
    if(name===null)return;await api(cls?'rename_class':'rename_section',{id,name});await refreshGroups();toast('تم تعديل الاسم');return;
  }
  if(action==='delete-class'||action==='delete-section'){
    if(!confirm('سيتم حذف هذا العنصر وكل ما يتبعه من طلاب وسجلات غياب نهائياً. هل تؤكد الحذف؟'))return;
    await api(action==='delete-class'?'delete_class':'delete_section',{id});await refreshGroups();toast('تم الحذف');return;
  }
  if(action==='edit-student'){
    const s=students.find(x=>x.id===id);if(!s)return;
    $('student-id').value=s.id;$('student-name').value=s.name;$('student-code').value=s.code;
    $('student-enrollment').value=s.enrollment;$('student-notes').value=s.notes;
    $('student-submit').textContent='حفظ التعديل';$('student-cancel').classList.remove('hidden');$('student-name').focus();return;
  }
  if(action==='delete-student'){
    if(!confirm('سيتم حذف الطالب وسجلات حضوره وغيابه نهائياً. هل تؤكد؟'))return;
    await api('delete_student',{id});await loadStudents();toast('تم حذف الطالب');return;
  }
  if(action==='move-student'){openMoveStudent(id);return;}
  if(action==='confirm-move'){
    const target=Number($('move-target').value);
    if(!target){toast('اختر الشعبة',true);return;}
    await api('move_student',{id,sectionId:target});studentPick.delete(id);closeModal();await loadStudents();toast('تم ترحيل الطالب');
    return;
  }
  if(action==='confirm-bulk-move'){
    const target=Number($('move-target').value);
    const ids=[...studentPick];
    if(!target||!ids.length){toast('اختر الشعبة والطلاب',true);return;}
    await executeBulkTransfer(target,ids);closeModal();await loadStudents();toast(`تم ترحيل ${ids.length} طالب`);
    return;
  }
  if(action==='history'){await showHistory(id);return;}
  if(action==='print-summary'){await printStudentReport('summary');return;}
  if(action==='print-detail'){await printStudentReport('detail');return;}
  if(action==='export-student'){await exportStudentExcel();return;}
  if(action==='edit-alert'){
    const a=config.alerts.find(x=>x.id===id);if(!a)return;
    $('alert-id').value=a.id;$('alert-name').value=a.name;$('alert-metric').value=a.metric;$('alert-operator').value=a.operator;
    $('alert-threshold').value=a.threshold;$('alert-color').value=a.color;$('alert-enabled').checked=a.enabled;
    $('alert-submit').textContent='حفظ التعديل';$('alert-cancel').classList.remove('hidden');$('alert-name').focus();return;
  }
  if(action==='delete-alert'){if(!confirm('هل تريد حذف قاعدة التنبيه؟'))return;await api('delete_alert',{id});config=await api('get_config');renderAlerts();toast('تم حذف القاعدة');return;}
  if(action==='delete-holiday'){if(!confirm('إزالة هذا التاريخ من العطل؟'))return;await api('delete_holiday',{date});await loadHolidays();await refreshHolidaysCache();toast('تمت إزالة العطلة');}
}
function on(id,event,fn){$(id).addEventListener(event,e=>run(()=>fn(e)));}
function attach() {
  document.addEventListener('click', e=>{
    const go=e.target.closest('[data-page],[data-goto]');if(go){run(()=>showPage(go.dataset.page||go.dataset.goto));return;}
    const action=e.target.closest('[data-action]');if(action)run(()=>dispatchAction(action));
  });
  on('att-class','change',async()=>{sectionOptions('att-class','att-section');saveAttPrefs();updateAttendanceDateNotice();await loadSheet();});
  on('att-section','change',async()=>{saveAttPrefs();await loadSheet();});
  on('att-date','change',async()=>{saveAttPrefs();updateAttendanceDateNotice();await loadSheet();});
  on('att-lessons','change',lessonCountChanged);
  on('att-load','click',loadSheet);on('att-save','click',saveSheet);
  on('att-clear','click',async()=>{
    const sectionId=chosen('att-section'),date=$('att-date').value;
    if(!sectionId)return;
    if(!confirm('سيُحذف سجل الحضور والغياب لهذا اليوم في الشعبة المحددة فقط، وستعود جميع الخانات إلى حاضر افتراضياً. هل تؤكد؟'))return;
    await api('clear_sheet',{sectionId,date});sheetDirty=false;await loadSheet();toast('تم حذف سجل اليوم');
  });
  on('att-all-present','click',()=>allStatuses('present'));on('att-all-absent','click',()=>allStatuses('absent'));on('att-all-excused','click',()=>allStatuses('excused'));
  on('students-class','change',async()=>{studentPick.clear();sectionOptions('students-class','students-section');resetStudent();await loadStudents();});
  on('students-section','change',async()=>{studentPick.clear();resetStudent();await loadStudents();});
  on('students-search','input',renderStudents);
  on('students-bulk-transfer','click',()=>openBulkMoveStudents());
  on('student-form','submit',submitStudent);on('student-cancel','click',resetStudent);
  document.addEventListener('change',e=>{
    if(e.target.id==='students-pick-all'){
      const visible=filteredStudentList();
      if(e.target.checked)visible.forEach(s=>studentPick.add(s.id));
      else visible.forEach(s=>studentPick.delete(s.id));
      renderStudents();
      return;
    }
    if(e.target.classList.contains('student-pick')){
      const sid=Number(e.target.dataset.id);
      if(e.target.checked)studentPick.add(sid);else studentPick.delete(sid);
      updateStudentTransferUi();
    }
  });
  on('import-toggle','click',()=>$('import-area').classList.toggle('hidden'));on('import-confirm','click',importStudents);
  on('class-form','submit',async e=>{e.preventDefault();await api('create_class',{name:$('class-name').value});$('class-name').value='';await refreshGroups();toast('تمت إضافة الصف');});
  on('section-form','submit',async e=>{e.preventDefault();await api('create_section',{classId:chosen('section-parent'),name:$('section-name').value});$('section-name').value='';await refreshGroups();toast('تمت إضافة الشعبة');});
  on('section-parent','change',renderClasses);
  on('report-section','change',loadReports);on('report-search','input',renderReports);on('report-alert-only','change',renderReports);on('report-sort','change',renderReports);
  on('report-export','click',exportCsv);on('report-print-section','click',()=>run(()=>printSectionSummary()));
  on('students-export','click',exportStudentsCsv);
  $('att-table').addEventListener('keydown',handleSheetKeydown);
  on('alert-form','submit',submitAlert);on('alert-cancel','click',resetAlert);
  on('holiday-form','submit',saveHoliday);on('settings-form','submit',saveSettings);
  on('backup-save','click',backupSave);on('backup-restore','click',backupRestore);
  on('modal-close','click',closeModal);on('modal-ok','click',closeModal);
  on('modal-cover','click',e=>{if(e.target.id==='modal-cover')closeModal();});
  document.addEventListener('keydown',e=>{
    if(e.key==='Escape')closeModal();
    if((e.ctrlKey||e.metaKey)&&e.key==='s'){
      if(page==='attendance'){
        e.preventDefault();
        if(sheetDirty&&!attendanceBlocked($('att-date').value)) run(()=>saveSheet());
      }
    }
  });
  window.addEventListener('beforeunload',e=>{if(sheetDirty){e.preventDefault();e.returnValue='';}});
}
async function initialize() {
  $('current-date').textContent=new Intl.DateTimeFormat('ar-IQ',{dateStyle:'full'}).format(new Date());
  $('att-date').value=localDate();$('holiday-date').value=localDate();
  attach();
  if(!window.__TAURI__){modal('تنبيه التشغيل',`<p>هذه واجهة برنامج سطح المكتب. لتشغيلها بقاعدة بيانات SQLite، افتحها عبر <b>cargo tauri dev</b> بعد تجهيز Rust وTauri.</p><p class="muted">يمكن عرض التصميم في المتصفح، لكن حفظ البيانات متاح داخل تطبيق سطح المكتب فقط.</p>`);return;}
  try{
    await refreshGroups();
    restoreAttPrefs();
    await refreshConfig();
    await refreshHolidaysCache();
    updateAttendanceDateNotice();
    await refreshHome();
  }catch(e){toast('تعذر بدء البرنامج: '+String(e),true);}
}
initialize();
