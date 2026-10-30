#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod delete;
mod drives;
mod report;
mod scan;

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Instant;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

use scan::{Kind, Progress, Query, SortKey, Tree};

#[derive(Default)]
struct AppState {
    tree: Mutex<Option<Tree>>,
    cancel: Arc<AtomicBool>,
    scanning: AtomicBool,
    /// Borrado o actualización de carpeta en curso.
    busy: AtomicBool,
    cancel_delete: Arc<AtomicBool>,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

fn with_tree<T>(
    state: &State<'_, AppState>,
    f: impl FnOnce(&Tree) -> Result<T, String>,
) -> Result<T, String> {
    let guard = lock(&state.tree);
    let tree = guard
        .as_ref()
        .ok_or_else(|| "No hay datos de escaneo.".to_string())?;
    f(tree)
}

// ───────────────────────────── DTOs ─────────────────────────────

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct NodeDto {
    id: u32,
    parent: u32,
    name: String,
    is_dir: bool,
    size: u64,
    files: u64,
    mtime: i64,
}

impl NodeDto {
    fn new(t: &Tree, id: u32) -> Self {
        let n = &t.nodes[id as usize];
        NodeDto {
            id,
            parent: n.parent,
            name: n.name.to_string(),
            is_dir: n.is_dir,
            size: n.size,
            files: n.files,
            mtime: n.mtime,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ChildrenDto {
    items: Vec<NodeDto>,
    total: usize,
    rest_count: usize,
    rest_size: u64,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct ProgressDto {
    files: u64,
    dirs: u64,
    bytes: u64,
    errors: u64,
    current: String,
    elapsed_ms: u64,
    top: Vec<LiveDto>,
    top_total: usize,
    top_rest_size: u64,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct LiveDto {
    name: String,
    is_dir: bool,
    size: u64,
    files: u64,
}

impl From<&Progress> for ProgressDto {
    fn from(p: &Progress) -> Self {
        ProgressDto {
            files: p.files,
            dirs: p.dirs,
            bytes: p.bytes,
            errors: p.errors,
            current: p.current.clone(),
            elapsed_ms: p.elapsed.as_millis() as u64,
            top: p
                .top
                .iter()
                .map(|i| LiveDto { name: i.name.to_string(), is_dir: i.is_dir, size: i.size, files: i.files })
                .collect(),
            top_total: p.top_total,
            top_rest_size: p.top_rest_size,
        }
    }
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct DoneDto {
    root_id: u32,
    root: String,
    size: u64,
    files: u64,
    dirs: u64,
    errors: u64,
    elapsed_ms: u64,
    cancelled: bool,
}

impl DoneDto {
    fn new(t: &Tree) -> Self {
        DoneDto {
            root_id: 0,
            root: t.nodes[0].name.to_string(),
            size: t.nodes[0].size,
            files: t.files,
            dirs: t.dirs,
            errors: t.error_count,
            elapsed_ms: t.elapsed.as_millis() as u64,
            cancelled: t.cancelled,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TopFileDto {
    id: u32,
    parent: u32,
    name: String,
    path: String,
    size: u64,
    mtime: i64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SearchItemDto {
    id: u32,
    parent: u32,
    name: String,
    path: String,
    is_dir: bool,
    size: u64,
    files: u64,
    mtime: i64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SearchDto {
    items: Vec<SearchItemDto>,
    count: u64,
    total_size: Option<u64>,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct SearchArgs {
    text: String,
    /// Bytes.
    min_size: u64,
    /// Solo lo modificado hace más de N días.
    older_than_days: Option<u32>,
    /// Extensiones separadas por comas, con o sin punto: «iso, .zip».
    exts: String,
    /// files | dirs | all
    kind: String,
    limit: usize,
}

#[derive(Serialize)]
struct ExtDto {
    ext: String,
    count: u64,
    size: u64,
}

#[derive(Serialize)]
struct ErrDto {
    path: String,
    message: String,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct DeleteItemDto {
    id: u32,
    name: String,
    ok: bool,
    error: Option<String>,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct DeleteDto {
    results: Vec<DeleteItemDto>,
    /// Bytes realmente eliminados.
    freed: u64,
    /// Totales actualizados de la raíz escaneada.
    size: u64,
    files: u64,
    dirs: u64,
    cancelled: bool,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct DeleteProgressDto {
    files: u64,
    bytes: u64,
    errors: u64,
    current: String,
    elapsed_ms: u64,
    index: usize,
    total: usize,
    name: String,
    /// delete | refresh
    phase: String,
}

#[derive(Serialize)]
struct RefreshDto {
    size: u64,
    files: u64,
    dirs: u64,
    removed: bool,
}

// ─────────────────────────── Comandos ───────────────────────────

#[tauri::command]
fn start_scan(
    app: AppHandle,
    state: State<'_, AppState>,
    path: String,
    threads: usize,
) -> Result<(), String> {
    if state.scanning.swap(true, Ordering::SeqCst) {
        return Err("Ya hay un escaneo en curso.".into());
    }
    state.cancel.store(false, Ordering::SeqCst);

    std::thread::spawn(move || {
        let state = app.state::<AppState>();
        *lock(&state.tree) = None; // libera la memoria del escaneo anterior
        let cancel = state.cancel.clone();
        let emitter = app.clone();

        let outcome = catch_unwind(AssertUnwindSafe(|| {
            scan::scan(&path, threads, cancel, |p| {
                let _ = emitter.emit("scan-progress", ProgressDto::from(p));
            })
        }));

        match outcome {
            Ok(Ok(tree)) => {
                let done = DoneDto::new(&tree);
                *lock(&state.tree) = Some(tree);
                state.scanning.store(false, Ordering::SeqCst);
                let _ = app.emit("scan-done", done);
            }
            Ok(Err(msg)) => {
                state.scanning.store(false, Ordering::SeqCst);
                let _ = app.emit("scan-error", msg);
            }
            Err(_) => {
                state.scanning.store(false, Ordering::SeqCst);
                let _ = app.emit("scan-error", "Error interno durante el escaneo.".to_string());
            }
        }
    });
    Ok(())
}

#[tauri::command]
fn cancel_scan(state: State<'_, AppState>) {
    state.cancel.store(true, Ordering::SeqCst);
}

#[tauri::command]
async fn get_children(
    state: State<'_, AppState>,
    id: u32,
    sort: String,
    desc: bool,
    limit: usize,
) -> Result<ChildrenDto, String> {
    with_tree(&state, |t| {
        let page = t
            .children_page(id, SortKey::parse(&sort), desc, limit.clamp(1, 5000))
            .ok_or_else(|| "Carpeta no encontrada.".to_string())?;
        Ok(ChildrenDto {
            items: page.ids.iter().map(|&i| NodeDto::new(t, i)).collect(),
            total: page.total,
            rest_count: page.rest_count,
            rest_size: page.rest_size,
        })
    })
}

#[tauri::command]
async fn get_ancestors(state: State<'_, AppState>, id: u32) -> Result<Vec<NodeDto>, String> {
    with_tree(&state, |t| {
        if t.nodes.get(id as usize).map_or(true, |n| n.removed) {
            return Err("Carpeta no encontrada.".to_string());
        }
        Ok(t.ancestors(id).into_iter().map(|i| NodeDto::new(t, i)).collect())
    })
}

#[tauri::command]
async fn get_path(state: State<'_, AppState>, id: u32) -> Result<String, String> {
    with_tree(&state, |t| {
        if id as usize >= t.nodes.len() {
            return Err("Elemento no encontrado.".to_string());
        }
        Ok(t.full_path(id).to_string_lossy().into_owned())
    })
}

#[tauri::command]
async fn get_top_files(state: State<'_, AppState>, limit: usize) -> Result<Vec<TopFileDto>, String> {
    with_tree(&state, |t| {
        Ok(t.top_files(limit.clamp(1, 2000))
            .into_iter()
            .map(|i| {
                let n = &t.nodes[i as usize];
                TopFileDto {
                    id: i,
                    parent: n.parent,
                    name: n.name.to_string(),
                    path: t.full_path(i).to_string_lossy().into_owned(),
                    size: n.size,
                    mtime: n.mtime,
                }
            })
            .collect())
    })
}

fn build_query(args: &SearchArgs) -> Query {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    Query {
        text: args.text.clone(),
        min_size: args.min_size,
        modified_before: args.older_than_days.map(|d| now - i64::from(d) * 86_400),
        exts: args
            .exts
            .split(|c: char| c == ',' || c == ';' || c.is_whitespace())
            .map(|e| e.trim().trim_start_matches(['.', '*']).to_lowercase())
            .filter(|e| !e.is_empty())
            .collect(),
        kind: Kind::parse(&args.kind),
    }
}

#[tauri::command]
async fn search(state: State<'_, AppState>, args: SearchArgs) -> Result<SearchDto, String> {
    let q = build_query(&args);
    with_tree(&state, |t| {
        let r = t.search(&q, args.limit.clamp(1, 2000));
        Ok(SearchDto {
            count: r.count,
            total_size: r.total_size,
            items: r
                .ids
                .into_iter()
                .map(|i| {
                    let n = &t.nodes[i as usize];
                    SearchItemDto {
                        id: i,
                        parent: n.parent,
                        name: n.name.to_string(),
                        path: t.full_path(i).to_string_lossy().into_owned(),
                        is_dir: n.is_dir,
                        size: n.size,
                        files: n.files,
                        mtime: n.mtime,
                    }
                })
                .collect(),
        })
    })
}

// ───────────────────────────── Informes ─────────────────────────────

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ExportDto {
    path: String,
    rows: usize,
    /// El informe no incluye todos los elementos (hay un resumen del resto).
    truncated: bool,
}

fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn report_row(t: &Tree, id: u32) -> report::Row {
    let n = &t.nodes[id as usize];
    report::Row {
        name: n.name.to_string(),
        dir: if n.parent == scan::NO_PARENT {
            String::new()
        } else {
            t.full_path(n.parent).to_string_lossy().into_owned()
        },
        path: t.full_path(id).to_string_lossy().into_owned(),
        is_dir: n.is_dir,
        size: n.size,
        files: n.files,
        mtime: n.mtime,
    }
}

fn folder_report(t: &Tree, id: u32, names: &(String, String)) -> Result<report::Report, String> {
    let node = t
        .nodes
        .get(id as usize)
        .filter(|n| !n.removed && n.is_dir)
        .ok_or_else(|| "Carpeta no encontrada.".to_string())?;
    let page = t
        .children_page(id, SortKey::Size, true, 50_000)
        .ok_or_else(|| "Carpeta no encontrada.".to_string())?;
    let root_size = t.nodes[0].size;
    let path = t.full_path(id).to_string_lossy().into_owned();
    Ok(report::Report {
        center: names.0.clone(),
        unit: names.1.clone(),
        title: if id == 0 { "Escaneo completo".to_string() } else { format!("Carpeta «{}»", node.name) },
        subtitle: path,
        details: vec![],
        kpis: vec![
            report::Kpi { label: "Tamaño".into(), value: report::fmt_bytes(node.size), note: "de esta carpeta".into() },
            report::Kpi { label: "Archivos".into(), value: report::fmt_int(node.files), note: "en total, con subcarpetas".into() },
            report::Kpi { label: "Elementos".into(), value: report::fmt_int(page.total as u64), note: "directos en la carpeta".into() },
            report::Kpi {
                label: "% del escaneo".into(),
                value: report::fmt_pct(node.size, root_size),
                note: format!("de {}", report::fmt_bytes(root_size)),
            },
        ],
        pct_base: node.size,
        total_row: Some(("Total de la carpeta".into(), node.size)),
        rows: page.ids.iter().map(|&i| report_row(t, i)).collect(),
        rest_count: page.rest_count,
        rest_size: Some(page.rest_size),
        with_dir: false,
        generated: now_secs(),
    })
}

fn search_report(t: &Tree, args: &SearchArgs, names: &(String, String)) -> Result<report::Report, String> {
    let q = build_query(args);
    let r = t.search(&q, 50_000);
    let root_size = t.nodes[0].size;
    let mut details = Vec::new();
    let kind = match q.kind {
        Kind::Files => "archivos",
        Kind::Dirs => "carpetas",
        Kind::All => "archivos y carpetas",
    };
    details.push(format!("Se buscan {kind}."));
    if !args.text.trim().is_empty() {
        details.push(format!("Nombre: {}", args.text.trim()));
    }
    if args.min_size > 0 {
        details.push(format!("Tamaño mínimo: {}", report::fmt_bytes(args.min_size)));
    }
    if let Some(d) = args.older_than_days {
        details.push(format!("Sin modificar desde hace más de {} días", report::fmt_int(u64::from(d))));
    }
    if !q.exts.is_empty() {
        details.push(format!("Extensiones: {}", q.exts.join(", ")));
    }
    if details.len() == 1 {
        details.push("Sin más filtros: se listan los más grandes.".into());
    }
    let rows: Vec<report::Row> = r.ids.iter().map(|&i| report_row(t, i)).collect();
    let listed: u64 = rows.iter().map(|x| x.size).sum();
    Ok(report::Report {
        center: names.0.clone(),
        unit: names.1.clone(),
        title: "Resultado de la búsqueda".into(),
        subtitle: t.full_path(0).to_string_lossy().into_owned(),
        details,
        kpis: vec![
            report::Kpi { label: "Resultados".into(), value: report::fmt_int(r.count), note: "elementos encontrados".into() },
            report::Kpi {
                label: "Tamaño total".into(),
                value: r.total_size.map(report::fmt_bytes).unwrap_or_else(|| "—".into()),
                note: if r.total_size.is_some() { "de los archivos hallados".into() } else { "solo se suma con archivos".into() },
            },
            report::Kpi { label: "En el informe".into(), value: report::fmt_int(rows.len() as u64), note: "los más grandes".into() },
            report::Kpi { label: "Escaneo".into(), value: report::fmt_bytes(root_size), note: "tamaño total escaneado".into() },
        ],
        pct_base: r.total_size.unwrap_or(root_size),
        total_row: r.total_size.map(|s| ("Total de los resultados".to_string(), s)),
        rest_count: (r.count as usize).saturating_sub(rows.len()),
        rest_size: r.total_size.map(|s| s.saturating_sub(listed)),
        rows,
        with_dir: true,
        generated: now_secs(),
    })
}

/// `espacio.ini` junto al ejecutable: permite fijar el nombre del centro en todos los equipos.
fn ini_value(key: &str) -> Option<String> {
    let path = std::env::current_exe().ok()?.parent()?.join("espacio.ini");
    let text = std::fs::read_to_string(path).ok()?;
    report::parse_ini(&text)
        .into_iter()
        .find(|(k, v)| k == key && !v.trim().is_empty())
        .map(|(_, v)| report::clean_name(&v))
}

/// Valores por defecto de los informes: los de `espacio.ini` si existe, o los de fábrica.
fn default_names() -> (String, String, bool) {
    let c = ini_value("centro");
    let u = ini_value("servicio");
    let from_file = c.is_some() || u.is_some();
    (
        c.unwrap_or_else(|| report::DEFAULT_CENTER.to_string()),
        u.unwrap_or_else(|| report::DEFAULT_UNIT.to_string()),
        from_file,
    )
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DefaultsDto {
    center: String,
    unit: String,
    from_file: bool,
}

#[tauri::command]
fn get_report_defaults() -> DefaultsDto {
    let (center, unit, from_file) = default_names();
    DefaultsDto { center, unit, from_file }
}

/// Genera un informe (PDF o CSV) de una carpeta o de una búsqueda y lo guarda en `path`.
#[tauri::command]
async fn export_report(
    state: State<'_, AppState>,
    kind: String,
    id: Option<u32>,
    args: Option<SearchArgs>,
    format: String,
    path: String,
    center: Option<String>,
    unit: Option<String>,
) -> Result<ExportDto, String> {
    let pdf = match format.as_str() {
        "pdf" => true,
        "csv" => false,
        _ => return Err("Formato de informe no válido.".into()),
    };
    // Lo escrito por el usuario manda; si está vacío, `espacio.ini`; y si no, los de fábrica.
    let (dc, du, _) = default_names();
    let pick = |mine: Option<String>, fallback: String| {
        mine.map(|s| report::clean_name(&s)).filter(|s| !s.is_empty()).unwrap_or(fallback)
    };
    let names = (pick(center, dc), pick(unit, du));
    let rep = with_tree(&state, |t| match kind.as_str() {
        "folder" => folder_report(t, id.unwrap_or(0), &names),
        "search" => search_report(t, &args.ok_or_else(|| "Faltan los filtros de la búsqueda.".to_string())?, &names),
        _ => Err("Tipo de informe no válido.".to_string()),
    })?;
    let mut target = PathBuf::from(path.trim());
    let want = if pdf { "pdf" } else { "csv" };
    if target.extension().map_or(true, |e| !e.eq_ignore_ascii_case(want)) {
        let mut s = target.into_os_string();
        s.push(format!(".{want}"));
        target = PathBuf::from(s);
    }
    let bytes = if pdf { report::to_pdf(&rep) } else { report::to_csv(&rep) };
    std::fs::write(&target, bytes)
        .map_err(|e| {
            let why = if e.kind() == std::io::ErrorKind::NotFound {
                "la carpeta de destino no existe".to_string()
            } else {
                scan::friendly_io(&e)
            };
            format!("No se pudo guardar «{}»: {}", target.display(), why)
        })?;
    Ok(ExportDto {
        path: target.to_string_lossy().into_owned(),
        rows: rep.rows.len(),
        truncated: rep.rest_count > 0 || (pdf && rep.rows.len() > report::PDF_MAX_ROWS),
    })
}

/// Muestra un archivo cualquiera (p. ej. un informe recién guardado) en el Explorador.
#[tauri::command]
fn reveal_path(path: String) -> Result<(), String> {
    let p = PathBuf::from(path);
    if !p.exists() {
        return Err("El archivo ya no existe.".into());
    }
    open_in_explorer(&p, false)
}

/// Enlace de donaciones. Es fijo: la interfaz no puede pedir que se abra otra dirección.
const DONATE_URL: &str = "https://www.paypal.me/marianoog";

/// Abre el enlace de donaciones en el navegador predeterminado.
#[tauri::command]
fn open_donate() -> Result<(), String> {
    #[cfg(windows)]
    let mut cmd = std::process::Command::new("explorer.exe");
    #[cfg(not(windows))]
    let mut cmd = std::process::Command::new("xdg-open");
    cmd.arg(DONATE_URL).spawn().map(|_| ()).map_err(|e| e.to_string())
}

#[tauri::command]
async fn get_extensions(state: State<'_, AppState>, limit: usize) -> Result<Vec<ExtDto>, String> {
    with_tree(&state, |t| {
        Ok(t.extensions(limit.clamp(1, 2000))
            .into_iter()
            .map(|e| ExtDto { ext: e.ext, count: e.count, size: e.size })
            .collect())
    })
}

#[tauri::command]
async fn get_errors(state: State<'_, AppState>) -> Result<Vec<ErrDto>, String> {
    with_tree(&state, |t| {
        Ok(t.errors
            .iter()
            .map(|e| ErrDto { path: e.path.clone(), message: e.message.clone() })
            .collect())
    })
}

#[tauri::command]
async fn reveal(state: State<'_, AppState>, id: u32) -> Result<(), String> {
    let (path, is_dir) = with_tree(&state, |t| {
        let n = t
            .nodes
            .get(id as usize)
            .ok_or_else(|| "Elemento no encontrado.".to_string())?;
        Ok((t.full_path(id), n.is_dir))
    })?;
    open_in_explorer(&path, is_dir)
}

#[tauri::command]
fn list_drives() -> Vec<drives::Drive> {
    drives::list()
}

/// Capacidad y espacio libre. Va aparte de `list_drives` y en otro hilo: una unidad de red caída
/// puede tardar en responder y no debe bloquear la interfaz.
#[tauri::command]
async fn get_drive_space(path: String) -> Option<drives::Space> {
    tauri::async_runtime::spawn_blocking(move || drives::space(&path)).await.ok().flatten()
}

struct Target {
    id: u32,
    name: String,
    path: PathBuf,
    is_dir: bool,
}

/// Valida lo marcado y resuelve sus rutas. Si hay una carpeta y algo de dentro,
/// solo cuenta la carpeta.
fn resolve_targets(state: &State<'_, AppState>, ids: &[u32]) -> Result<Vec<Target>, String> {
    with_tree(state, |t| {
        let wanted: HashSet<u32> = ids.iter().copied().collect();
        let mut out = Vec::new();
        for &id in &wanted {
            let n = t
                .nodes
                .get(id as usize)
                .ok_or_else(|| "Elemento no encontrado.".to_string())?;
            if id == 0 {
                return Err("No se puede eliminar la carpeta raíz del escaneo.".to_string());
            }
            if n.removed {
                return Err("Ese elemento ya no está en la lista; actualiza la carpeta.".to_string());
            }
            let anc = t.ancestors(id);
            if anc[..anc.len() - 1].iter().any(|a| wanted.contains(a)) {
                continue;
            }
            let path = t.full_path(id);
            if path.parent().is_none() {
                return Err(format!("«{}» es una raíz de unidad: no se elimina.", path.display()));
            }
            out.push(Target { id, name: n.name.to_string(), path, is_dir: n.is_dir });
        }
        if out.is_empty() {
            return Err("No hay nada que eliminar.".to_string());
        }
        Ok(out)
    })
}

/// Elimina de forma DEFINITIVA (sin papelera) lo marcado. Trabaja en segundo plano y
/// avisa con los eventos `delete-progress` y `delete-done` (o `delete-error`).
/// Al terminar cada elemento, el árbol se sincroniza con lo que quede realmente en disco.
#[tauri::command]
fn start_delete(app: AppHandle, state: State<'_, AppState>, ids: Vec<u32>) -> Result<(), String> {
    if state.scanning.load(Ordering::SeqCst) {
        return Err("Hay un escaneo en curso.".into());
    }
    if state.busy.swap(true, Ordering::SeqCst) {
        return Err("Hay otra operación en curso.".into());
    }
    let todo = match resolve_targets(&state, &ids) {
        Ok(t) => t,
        Err(e) => {
            state.busy.store(false, Ordering::SeqCst);
            return Err(e);
        }
    };
    state.cancel_delete.store(false, Ordering::SeqCst);

    std::thread::spawn(move || {
        let state = app.state::<AppState>();
        let counters = delete::Counters::default();
        let started = Instant::now();
        let total = todo.len();
        let cancel = state.cancel_delete.clone();
        let mut items: Vec<DeleteItemDto> = Vec::new();
        let mut cancelled = false;

        let emit = |index: usize, name: &str, phase: &str| {
            let current = counters.current.lock().map(|c| c.clone()).unwrap_or_default();
            let _ = app.emit(
                "delete-progress",
                DeleteProgressDto {
                    files: counters.files.load(Ordering::Relaxed),
                    bytes: counters.bytes.load(Ordering::Relaxed),
                    errors: counters.errors.load(Ordering::Relaxed),
                    current,
                    elapsed_ms: started.elapsed().as_millis() as u64,
                    index,
                    total,
                    name: name.to_string(),
                    phase: phase.to_string(),
                },
            );
        };

        let outcome = catch_unwind(AssertUnwindSafe(|| {
            for (i, t) in todo.iter().enumerate() {
                if cancel.load(Ordering::SeqCst) {
                    cancelled = true;
                    break;
                }
                emit(i + 1, &t.name, "delete");
                let out = delete::delete_target(&t.path, t.is_dir, 16, &cancel, &counters, || {
                    emit(i + 1, &t.name, "delete")
                });
                cancelled |= out.cancelled;

                if out.fully_deleted {
                    if let Some(tree) = lock(&state.tree).as_mut() {
                        tree.remove(t.id);
                    }
                } else if t.is_dir {
                    // Borrado parcial: se vuelve a leer del disco lo que queda.
                    emit(i + 1, &t.name, "refresh");
                    let fresh = scan::scan(
                        &t.path.to_string_lossy(),
                        16,
                        Arc::new(AtomicBool::new(false)),
                        |p| {
                            if let Ok(mut c) = counters.current.try_lock() {
                                *c = p.current.clone();
                            }
                            emit(i + 1, &t.name, "refresh");
                        },
                    );
                    if let Some(tree) = lock(&state.tree).as_mut() {
                        match fresh {
                            Ok(sub) => {
                                tree.graft(t.id, sub);
                            }
                            Err(_) => {
                                tree.remove(t.id); // ya no existe
                            }
                        }
                    }
                }

                let error = if out.fully_deleted {
                    None
                } else if out.cancelled {
                    Some("detenido; queda parte del contenido".to_string())
                } else {
                    Some(match out.samples.first() {
                        Some((p, m)) => format!("{} errores; el primero: {}: {}", out.error_count, p, m),
                        None => "no se pudo eliminar".to_string(),
                    })
                };
                items.push(DeleteItemDto { id: t.id, name: t.name.clone(), ok: out.fully_deleted, error });
            }
        }));

        state.busy.store(false, Ordering::SeqCst);
        match outcome {
            Ok(()) => {
                let (size, files, dirs) = lock(&state.tree)
                    .as_ref()
                    .map(|t| (t.nodes[0].size, t.files, t.dirs))
                    .unwrap_or((0, 0, 0));
                let _ = app.emit(
                    "delete-done",
                    DeleteDto {
                        results: items,
                        freed: counters.bytes.load(Ordering::Relaxed),
                        size,
                        files,
                        dirs,
                        cancelled,
                    },
                );
            }
            Err(_) => {
                let _ = app.emit("delete-error", "Error interno durante el borrado.".to_string());
            }
        }
    });
    Ok(())
}

#[tauri::command]
fn cancel_delete(state: State<'_, AppState>) {
    state.cancel_delete.store(true, Ordering::SeqCst);
}

/// Vuelve a leer una carpeta del disco y actualiza el árbol (y los totales de sus ancestros).
#[tauri::command]
async fn refresh_node(state: State<'_, AppState>, id: u32) -> Result<RefreshDto, String> {
    if state.scanning.load(Ordering::SeqCst) {
        return Err("Hay un escaneo en curso.".into());
    }
    if state.busy.swap(true, Ordering::SeqCst) {
        return Err("Hay otra operación en curso.".into());
    }
    let res = refresh_inner(&state, id);
    state.busy.store(false, Ordering::SeqCst);
    res
}

fn refresh_inner(state: &State<'_, AppState>, id: u32) -> Result<RefreshDto, String> {
    let path = with_tree(state, |t| {
        let n = t
            .nodes
            .get(id as usize)
            .filter(|n| !n.removed)
            .ok_or_else(|| "Carpeta no encontrada.".to_string())?;
        if !n.is_dir {
            return Err("Solo se pueden actualizar carpetas.".to_string());
        }
        Ok(t.full_path(id))
    })?;

    let fresh = scan::scan(&path.to_string_lossy(), 16, Arc::new(AtomicBool::new(false)), |_| {});
    let mut guard = lock(&state.tree);
    let tree = guard
        .as_mut()
        .ok_or_else(|| "No hay datos de escaneo.".to_string())?;
    let removed = match fresh {
        Ok(sub) => {
            tree.graft(id, sub);
            false
        }
        Err(msg) => {
            if id == 0 || path.exists() {
                return Err(msg); // sigue ahí pero no se puede leer
            }
            tree.remove(id); // ya no existe
            true
        }
    };
    Ok(RefreshDto { size: tree.nodes[0].size, files: tree.files, dirs: tree.dirs, removed })
}

#[cfg(windows)]
fn open_in_explorer(path: &Path, is_dir: bool) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    let mut cmd = std::process::Command::new("explorer.exe");
    if is_dir {
        cmd.arg(path);
    } else {
        cmd.raw_arg(format!("/select,\"{}\"", path.display()));
    }
    cmd.spawn().map(|_| ()).map_err(|e| e.to_string())
}

#[cfg(not(windows))]
fn open_in_explorer(path: &Path, is_dir: bool) -> Result<(), String> {
    let target = if is_dir { path } else { path.parent().unwrap_or(path) };
    std::process::Command::new("xdg-open")
        .arg(target)
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            start_scan,
            cancel_scan,
            get_children,
            get_ancestors,
            get_path,
            get_top_files,
            get_extensions,
            search,
            export_report,
            get_report_defaults,
            reveal_path,
            open_donate,
            get_errors,
            reveal,
            list_drives,
            get_drive_space,
            start_delete,
            cancel_delete,
            refresh_node,
        ])
        .run(tauri::generate_context!())
        .expect("error al iniciar la aplicación");
}
