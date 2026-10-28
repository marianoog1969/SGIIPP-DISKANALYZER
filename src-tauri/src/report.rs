//! Informes exportables: CSV (para Excel) y PDF con el estilo de los informes del centro.
//!
//! El PDF se escribe a mano (texto, rectángulos y líneas con las fuentes estándar Helvetica),
//! así que no hace falta ninguna dependencia. No depende de Tauri y se prueba con `cargo test`.

use std::fmt::Write as _;

// ───────────────────────────── Datos ─────────────────────────────

pub struct Row {
    pub name: String,
    /// Carpeta que lo contiene (se muestra bajo el nombre en las búsquedas).
    pub dir: String,
    pub path: String,
    pub is_dir: bool,
    pub size: u64,
    pub files: u64,
    pub mtime: i64,
}

pub struct Kpi {
    pub label: String,
    pub value: String,
    pub note: String,
}

/// Nombre del centro y servicio que salen en los informes si nadie los cambia.
pub const DEFAULT_CENTER: &str = "Centro de Inserción Social «Guillermo Miranda» · Murcia";
pub const DEFAULT_UNIT: &str = "Servicios de Informática";

pub struct Report {
    /// Nombre del centro (banda, cabecera y pie del PDF).
    pub center: String,
    /// Servicio o unidad que genera el informe.
    pub unit: String,
    pub title: String,
    pub subtitle: String,
    /// Líneas con los filtros aplicados (búsquedas).
    pub details: Vec<String>,
    pub kpis: Vec<Kpi>,
    /// Base para calcular el porcentaje de cada fila.
    pub pct_base: u64,
    /// Fila final de totales: (texto, bytes).
    pub total_row: Option<(String, u64)>,
    pub rows: Vec<Row>,
    pub rest_count: usize,
    /// Tamaño de lo que no cabe en la lista, si se conoce.
    pub rest_size: Option<u64>,
    /// Mostrar la carpeta de cada elemento bajo su nombre.
    pub with_dir: bool,
    /// Instante de generación (segundos Unix).
    pub generated: i64,
}

// ─────────────────────────── Formatos ───────────────────────────

pub fn fmt_int(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::with_capacity(s.len() + s.len() / 3);
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push('.');
        }
        out.push(c);
    }
    out
}

fn dec(v: f64, digits: usize) -> String {
    format!("{:.*}", digits, v).replace('.', ",")
}

/// Igual que en la interfaz: 3 cifras significativas y coma decimal.
pub fn fmt_bytes(n: u64) -> String {
    if n < 1024 {
        return format!("{n} B");
    }
    let units = ["KB", "MB", "GB", "TB", "PB"];
    let mut v = n as f64;
    let mut i = 0usize;
    loop {
        v /= 1024.0;
        if v < 1024.0 || i == units.len() - 1 {
            break;
        }
        i += 1;
    }
    let digits = if v >= 100.0 { 0 } else if v >= 10.0 { 1 } else { 2 };
    let mut int_part = dec(v, digits);
    if digits == 0 || v >= 1000.0 {
        // miles con punto en cifras grandes (p. ej. 1.234 PB)
        if let Some((a, b)) = int_part.split_once(',') {
            int_part = format!("{}{}{}", fmt_int(a.parse().unwrap_or(0)), if b.is_empty() { "" } else { "," }, b);
        } else if let Ok(a) = int_part.parse::<u64>() {
            int_part = fmt_int(a);
        }
    }
    format!("{} {}", int_part, units[i])
}

pub fn fmt_pct(part: u64, total: u64) -> String {
    let p = if total > 0 { part as f64 / total as f64 * 100.0 } else { 0.0 };
    format!("{} %", dec(p, 1))
}

/// Días desde 1970-01-01 → (año, mes, día), en calendario gregoriano.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// (año, mes, día, hora, minuto) en UTC.
fn utc_parts(secs: i64) -> (i64, u32, u32, u32, u32) {
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let (y, m, d) = civil_from_days(days);
    (y, m, d, (rem / 3600) as u32, ((rem % 3600) / 60) as u32)
}

/// Hora local de Windows (con su horario de verano); en otros sistemas, UTC.
#[cfg(windows)]
fn local_parts(secs: i64) -> (i64, u32, u32, u32, u32) {
    win_tz::local(secs).unwrap_or_else(|| utc_parts(secs))
}

#[cfg(not(windows))]
fn local_parts(secs: i64) -> (i64, u32, u32, u32, u32) {
    utc_parts(secs)
}

#[cfg(windows)]
mod win_tz {
    #[repr(C)]
    struct FileTime {
        low: u32,
        high: u32,
    }

    #[repr(C)]
    #[derive(Default, Clone, Copy)]
    struct SystemTime {
        year: u16,
        month: u16,
        day_of_week: u16,
        day: u16,
        hour: u16,
        minute: u16,
        second: u16,
        millis: u16,
    }

    #[link(name = "kernel32")]
    extern "system" {
        fn FileTimeToSystemTime(ft: *const FileTime, st: *mut SystemTime) -> i32;
        fn SystemTimeToTzSpecificLocalTime(
            tz: *const core::ffi::c_void,
            utc: *const SystemTime,
            local: *mut SystemTime,
        ) -> i32;
    }

    pub fn local(secs: i64) -> Option<(i64, u32, u32, u32, u32)> {
        let ticks = secs.checked_add(11_644_473_600)?.checked_mul(10_000_000)?;
        if ticks < 0 {
            return None;
        }
        let ticks = ticks as u64;
        let ft = FileTime { low: (ticks & 0xFFFF_FFFF) as u32, high: (ticks >> 32) as u32 };
        let mut utc = SystemTime::default();
        let mut loc = SystemTime::default();
        // SAFETY: punteros a estructuras locales válidas; tz nulo = zona horaria actual.
        unsafe {
            if FileTimeToSystemTime(&ft, &mut utc) == 0 {
                return None;
            }
            if SystemTimeToTzSpecificLocalTime(std::ptr::null(), &utc, &mut loc) == 0 {
                return None;
            }
        }
        Some((loc.year as i64, loc.month as u32, loc.day as u32, loc.hour as u32, loc.minute as u32))
    }
}

pub fn fmt_date(secs: i64) -> String {
    if secs <= 0 {
        return "—".to_string();
    }
    let (y, mo, d, h, mi) = local_parts(secs);
    format!("{d:02}/{mo:02}/{y:04} {h:02}:{mi:02}")
}

// ─────────────────────── Configuración (espacio.ini) ───────────────────────

/// Lee líneas `clave=valor` (comentarios con `#` o `;`). Las claves van en minúsculas.
pub fn parse_ini(text: &str) -> Vec<(String, String)> {
    text.trim_start_matches('\u{feff}')
        .lines()
        .filter_map(|l| {
            let l = l.trim();
            if l.is_empty() || l.starts_with('#') || l.starts_with(';') {
                return None;
            }
            let (k, v) = l.split_once('=')?;
            let v = v.trim();
            let v = v.strip_prefix('"').and_then(|x| x.strip_suffix('"')).unwrap_or(v);
            Some((k.trim().to_lowercase(), v.trim().to_string()))
        })
        .collect()
}

/// Valor de un texto libre: sin espacios sobrantes y con un tope razonable de longitud.
pub fn clean_name(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(150).collect()
}

// ───────────────────────────── CSV ─────────────────────────────

fn csv_field(s: &str, out: &mut String) {
    if s.contains([';', '"', '\n', '\r']) {
        out.push('"');
        out.push_str(&s.replace('"', "\"\""));
        out.push('"');
    } else {
        out.push_str(s);
    }
}

/// CSV para Excel en español: UTF-8 con marca BOM, separador `;` y fin de línea Windows.
pub fn to_csv(r: &Report) -> Vec<u8> {
    let mut s = String::from('\u{feff}');
    for (i, h) in [
        "Nombre",
        "Ruta completa",
        "Tipo",
        "Tamaño (bytes)",
        "Tamaño",
        "% del total",
        "Archivos",
        "Modificado",
    ]
    .iter()
    .enumerate()
    {
        if i > 0 {
            s.push(';');
        }
        s.push_str(h);
    }
    s.push_str("\r\n");
    for row in &r.rows {
        csv_field(&row.name, &mut s);
        s.push(';');
        csv_field(&row.path, &mut s);
        s.push(';');
        s.push_str(if row.is_dir { "Carpeta" } else { "Archivo" });
        s.push(';');
        let _ = write!(s, "{}", row.size);
        s.push(';');
        s.push_str(&fmt_bytes(row.size));
        s.push(';');
        let p = if r.pct_base > 0 { row.size as f64 / r.pct_base as f64 * 100.0 } else { 0.0 };
        s.push_str(&dec(p, 2));
        s.push(';');
        if row.is_dir {
            let _ = write!(s, "{}", row.files);
        }
        s.push(';');
        s.push_str(&if row.mtime > 0 { fmt_date(row.mtime) } else { String::new() });
        s.push_str("\r\n");
    }
    if r.rest_count > 0 {
        let _ = write!(s, "(y {} elementos más);;;", fmt_int(r.rest_count as u64));
        match r.rest_size {
            Some(b) => {
                let _ = write!(s, "{};{};;;\r\n", b, fmt_bytes(b));
            }
            None => s.push_str(";;;;\r\n"),
        }
    }
    if let Some((label, bytes)) = &r.total_row {
        csv_field(label, &mut s);
        let _ = write!(s, ";;;{};{};;;\r\n", bytes, fmt_bytes(*bytes));
    }
    s.into_bytes()
}

// ───────────────────────────── PDF ─────────────────────────────

type Rgb = (f32, f32, f32);

const fn hex(v: u32) -> Rgb {
    (((v >> 16) & 0xFF) as f32 / 255.0, ((v >> 8) & 0xFF) as f32 / 255.0, (v & 0xFF) as f32 / 255.0)
}

const AZUL_FONDO: Rgb = hex(0x1B3A6B);
const AZUL_PRIMARIO: Rgb = hex(0x2563A8);
const AZUL_CLARO: Rgb = hex(0x4A90D9);
const NARANJA: Rgb = hex(0xE8601C);
const GRIS_TEXTO: Rgb = hex(0x4A4A4A);
const GRIS_CLARO: Rgb = hex(0x9B9B9B);
const GRIS_FONDO: Rgb = hex(0xF5F7FA);
const BORDE: Rgb = hex(0xE0E6EF);
const BLANCO: Rgb = hex(0xFFFFFF);
const AZUL_TOTAL: Rgb = hex(0xE8F0FB);
const TEXTO_SOBRE_AZUL: Rgb = hex(0xC9D3E6);
const NEGRO: Rgb = hex(0x1A1A1A);

const PAGE_W: f32 = 595.0;
const PAGE_H: f32 = 842.0;
const ML: f32 = 40.0;
const MR: f32 = 555.0;
const CONTENT_W: f32 = MR - ML;
/// Filas máximas en el PDF (el CSV admite muchas más).
pub const PDF_MAX_ROWS: usize = 3000;

#[derive(Clone, Copy, PartialEq)]
enum Font {
    Regular,
    Bold,
    Italic,
}

impl Font {
    fn res(self) -> &'static str {
        match self {
            Font::Regular => "F1",
            Font::Bold => "F2",
            Font::Italic => "F3",
        }
    }
}

// Anchos AFM de Helvetica y Helvetica-Bold, caracteres 32..=126 (milésimas de em).
const W_REG: [u16; 95] = [
    278, 278, 355, 556, 556, 889, 667, 191, 333, 333, 389, 584, 278, 333, 278, 278, 556, 556, 556, 556, 556, 556,
    556, 556, 556, 556, 278, 278, 584, 584, 584, 556, 1015, 667, 667, 722, 722, 667, 611, 778, 722, 278, 500, 667,
    556, 833, 722, 778, 667, 778, 722, 667, 611, 722, 667, 944, 667, 667, 611, 278, 278, 278, 469, 556, 333, 556,
    556, 500, 556, 556, 278, 556, 556, 222, 222, 500, 222, 833, 556, 556, 556, 556, 333, 500, 278, 556, 500, 722,
    500, 500, 500, 334, 260, 334, 584,
];
const W_BOLD: [u16; 95] = [
    278, 333, 474, 556, 556, 889, 722, 238, 333, 333, 389, 584, 278, 333, 278, 278, 556, 556, 556, 556, 556, 556,
    556, 556, 556, 556, 333, 333, 584, 584, 584, 611, 975, 722, 722, 722, 722, 667, 611, 778, 722, 278, 556, 722,
    611, 833, 722, 778, 667, 778, 722, 667, 611, 722, 667, 944, 667, 667, 611, 333, 278, 333, 584, 556, 333, 556,
    611, 556, 611, 556, 333, 611, 611, 278, 278, 556, 278, 889, 611, 611, 611, 611, 389, 556, 333, 611, 556, 778,
    556, 556, 500, 389, 280, 389, 584,
];

/// Carácter → byte de la codificación WinAnsi (Windows-1252), que es la que usa el PDF.
fn cp1252(c: char) -> u8 {
    match c {
        '\u{20}'..='\u{7E}' => c as u8,
        '\u{A0}'..='\u{FF}' => c as u32 as u8,
        '€' => 0x80,
        '…' => 0x85,
        '‘' => 0x91,
        '’' => 0x92,
        '“' => 0x93,
        '”' => 0x94,
        '•' => 0x95,
        '–' => 0x96,
        '—' => 0x97,
        c if c.is_control() => b' ',
        _ => b'?',
    }
}

fn base_letter(c: char) -> char {
    match c {
        'á' | 'à' | 'â' | 'ä' | 'ã' | 'å' => 'a',
        'Á' | 'À' | 'Â' | 'Ä' | 'Ã' | 'Å' => 'A',
        'é' | 'è' | 'ê' | 'ë' => 'e',
        'É' | 'È' | 'Ê' | 'Ë' => 'E',
        'í' | 'ì' | 'î' | 'ï' => 'i',
        'Í' | 'Ì' | 'Î' | 'Ï' => 'I',
        'ó' | 'ò' | 'ô' | 'ö' | 'õ' => 'o',
        'Ó' | 'Ò' | 'Ô' | 'Ö' | 'Õ' => 'O',
        'ú' | 'ù' | 'û' | 'ü' => 'u',
        'Ú' | 'Ù' | 'Û' | 'Ü' => 'U',
        'ñ' => 'n',
        'Ñ' => 'N',
        'ç' => 'c',
        'Ç' => 'C',
        other => other,
    }
}

fn char_width(font: Font, c: char) -> f32 {
    let table = if font == Font::Bold { &W_BOLD } else { &W_REG };
    let b = cp1252(c);
    let w = match b {
        32..=126 => table[(b - 32) as usize],
        0x85 | 0x97 => 1000,
        0x96 => 556,
        0xB7 => 278,
        0xAB | 0xBB => 556,
        _ => {
            let bl = base_letter(c);
            if bl != c && (bl as u32) < 127 {
                table[(bl as u8 - 32) as usize]
            } else {
                556
            }
        }
    };
    w as f32
}

fn text_width(font: Font, size: f32, s: &str) -> f32 {
    s.chars().map(|c| char_width(font, c)).sum::<f32>() * size / 1000.0
}

/// Recorta por el final con «…» para que quepa en `max` puntos.
fn fit_end(font: Font, size: f32, s: &str, max: f32) -> String {
    if text_width(font, size, s) <= max {
        return s.to_string();
    }
    let mut chars: Vec<char> = s.chars().collect();
    while !chars.is_empty() {
        chars.pop();
        let cand: String = chars.iter().collect::<String>() + "…";
        if text_width(font, size, &cand) <= max {
            return cand;
        }
    }
    "…".to_string()
}

/// Recorta por el principio (útil en rutas: se conserva el final).
fn fit_start(font: Font, size: f32, s: &str, max: f32) -> String {
    if text_width(font, size, s) <= max {
        return s.to_string();
    }
    let chars: Vec<char> = s.chars().collect();
    for skip in 1..chars.len() {
        let cand = format!("…{}", chars[skip..].iter().collect::<String>());
        if text_width(font, size, &cand) <= max {
            return cand;
        }
    }
    "…".to_string()
}

/// Cadena literal PDF: ASCII con escapes octales para lo que no lo es.
fn pdf_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('(');
    for c in s.chars() {
        let b = cp1252(c);
        match b {
            b'(' | b')' | b'\\' => {
                out.push('\\');
                out.push(b as char);
            }
            32..=126 => out.push(b as char),
            _ => {
                let _ = write!(out, "\\{b:03o}");
            }
        }
    }
    out.push(')');
    out
}

#[derive(Default)]
struct Canvas {
    ops: String,
}

impl Canvas {
    fn rect(&mut self, x: f32, y: f32, w: f32, h: f32, c: Rgb) {
        let _ = writeln!(self.ops, "{:.3} {:.3} {:.3} rg {:.2} {:.2} {:.2} {:.2} re f", c.0, c.1, c.2, x, y, w, h);
    }

    fn line(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, c: Rgb, width: f32) {
        let _ = writeln!(
            self.ops,
            "{:.2} w {:.3} {:.3} {:.3} RG {:.2} {:.2} m {:.2} {:.2} l S",
            width, c.0, c.1, c.2, x1, y1, x2, y2
        );
    }

    fn text(&mut self, x: f32, y: f32, font: Font, size: f32, c: Rgb, s: &str) {
        let _ = writeln!(
            self.ops,
            "BT /{} {:.2} Tf {:.3} {:.3} {:.3} rg {:.2} {:.2} Td {} Tj ET",
            font.res(),
            size,
            c.0,
            c.1,
            c.2,
            x,
            y,
            pdf_string(s)
        );
    }

    fn text_right(&mut self, x_right: f32, y: f32, font: Font, size: f32, c: Rgb, s: &str) {
        let w = text_width(font, size, s);
        self.text(x_right - w, y, font, size, c, s);
    }
}

// Columnas de la tabla.
const COL_NAME_W: f32 = 212.0;
const COL_SIZE_R: f32 = 318.0;
const COL_BAR_X: f32 = 328.0;
const COL_BAR_W: f32 = 48.0;
const COL_PCT_R: f32 = 424.0;
const COL_FILES_R: f32 = 478.0;
const COL_DATE_X: f32 = 486.0;

const FOOT_Y: f32 = 36.0;
const BODY_MIN_Y: f32 = 54.0;

fn table_header(cv: &mut Canvas, y: f32) {
    cv.rect(ML, y - 16.0, CONTENT_W, 16.0, AZUL_FONDO);
    let ty = y - 11.0;
    cv.text(ML + 6.0, ty, Font::Bold, 7.0, BLANCO, "NOMBRE");
    cv.text_right(COL_SIZE_R, ty, Font::Bold, 7.0, BLANCO, "TAMAÑO");
    cv.text(COL_BAR_X, ty, Font::Bold, 7.0, BLANCO, "PESO");
    cv.text_right(COL_PCT_R, ty, Font::Bold, 7.0, BLANCO, "%");
    cv.text_right(COL_FILES_R, ty, Font::Bold, 7.0, BLANCO, "ARCHIVOS");
    cv.text(COL_DATE_X, ty, Font::Bold, 7.0, BLANCO, "MODIFICADO");
}

/// Une con `sep` las partes que no estén vacías.
fn join_nonempty(parts: &[&str], sep: &str) -> String {
    parts.iter().map(|p| p.trim()).filter(|p| !p.is_empty()).collect::<Vec<_>>().join(sep)
}

fn running_header(cv: &mut Canvas, center: &str) {
    let y = PAGE_H - 30.0;
    cv.text(ML, y, Font::Regular, 8.0, GRIS_TEXTO, &fit_end(Font::Regular, 8.0, center, 360.0));
    cv.text_right(MR, y, Font::Bold, 8.0, AZUL_PRIMARIO, "INFORME DE ESPACIO EN DISCO");
    cv.line(ML, y - 6.0, MR, y - 6.0, AZUL_PRIMARIO, 0.8);
}

fn first_page_top(cv: &mut Canvas, r: &Report) -> f32 {
    // Banda azul con el título, como en la portada de los informes del centro.
    let band_h = 150.0;
    cv.rect(0.0, PAGE_H - band_h, PAGE_W, band_h, AZUL_FONDO);
    cv.rect(0.0, PAGE_H - band_h, 6.0, band_h, NARANJA);
    cv.text(ML, PAGE_H - 40.0, Font::Bold, 8.0, AZUL_CLARO, "I N F O R M E   D E   E S P A C I O   E N   D I S C O");
    let title = fit_end(Font::Bold, 21.0, &r.title, CONTENT_W);
    cv.text(ML, PAGE_H - 70.0, Font::Bold, 21.0, BLANCO, &title);
    let sub = fit_start(Font::Regular, 9.0, &r.subtitle, CONTENT_W);
    cv.text(ML, PAGE_H - 90.0, Font::Regular, 9.0, TEXTO_SOBRE_AZUL, &sub);
    cv.text(
        ML,
        PAGE_H - 114.0,
        Font::Bold,
        8.0,
        NARANJA,
        &fit_end(Font::Bold, 8.0, &r.center.to_uppercase(), CONTENT_W),
    );
    cv.text(
        ML,
        PAGE_H - 129.0,
        Font::Italic,
        8.0,
        TEXTO_SOBRE_AZUL,
        &join_nonempty(&[&r.unit, &format!("Generado el {}", fmt_date(r.generated))], " · "),
    );

    let mut y = PAGE_H - band_h - 18.0;
    if !r.kpis.is_empty() {
        let n = r.kpis.len() as f32;
        let gap = 10.0;
        let w = (CONTENT_W - gap * (n - 1.0)) / n;
        let h = 58.0;
        for (i, k) in r.kpis.iter().enumerate() {
            let x = ML + i as f32 * (w + gap);
            let top = y;
            cv.rect(x, top - h, w, h, GRIS_FONDO);
            let color = if i % 2 == 0 { AZUL_PRIMARIO } else { NARANJA };
            cv.rect(x, top - 3.0, w, 3.0, color);
            cv.text(x + 8.0, top - 16.0, Font::Regular, 6.5, GRIS_CLARO, &fit_end(Font::Regular, 6.5, &k.label.to_uppercase(), w - 14.0));
            cv.text(x + 8.0, top - 38.0, Font::Bold, 18.0, color, &fit_end(Font::Bold, 18.0, &k.value, w - 14.0));
            cv.text(x + 8.0, top - 52.0, Font::Regular, 7.0, GRIS_CLARO, &fit_end(Font::Regular, 7.0, &k.note, w - 14.0));
        }
        y -= h + 14.0;
    }
    for d in &r.details {
        cv.text(ML, y - 8.0, Font::Regular, 8.5, GRIS_TEXTO, &fit_end(Font::Regular, 8.5, d, CONTENT_W));
        y -= 13.0;
    }
    y - 8.0
}

struct Rowspec<'a> {
    cv: &'a mut Canvas,
    y: f32,
    h: f32,
    zebra: bool,
}

fn draw_row(rs: &mut Rowspec, r: &Report, row: &Row) {
    let (cv, y, h) = (&mut *rs.cv, rs.y, rs.h);
    if rs.zebra {
        cv.rect(ML, y - h, CONTENT_W, h, GRIS_FONDO);
    }
    let font = if row.is_dir { Font::Bold } else { Font::Regular };
    let ty = if r.with_dir { y - 10.0 } else { y - h / 2.0 - 2.8 };
    cv.text(ML + 6.0, ty, font, 8.0, NEGRO, &fit_end(font, 8.0, &row.name, COL_NAME_W - 6.0));
    if r.with_dir {
        cv.text(
            ML + 6.0,
            y - 19.5,
            Font::Regular,
            6.5,
            GRIS_CLARO,
            &fit_start(Font::Regular, 6.5, &row.dir, COL_NAME_W + 40.0),
        );
    }
    let mid = y - h / 2.0 - 2.8;
    cv.text_right(COL_SIZE_R, mid, Font::Regular, 8.0, GRIS_TEXTO, &fmt_bytes(row.size));
    let p = if r.pct_base > 0 { (row.size as f64 / r.pct_base as f64).clamp(0.0, 1.0) as f32 } else { 0.0 };
    cv.rect(COL_BAR_X, y - h / 2.0 - 2.5, COL_BAR_W, 5.0, BORDE);
    if p > 0.0 {
        cv.rect(COL_BAR_X, y - h / 2.0 - 2.5, (COL_BAR_W * p).max(1.0), 5.0, AZUL_PRIMARIO);
    }
    cv.text_right(COL_PCT_R, mid, Font::Regular, 8.0, GRIS_TEXTO, &fmt_pct(row.size, r.pct_base));
    if row.is_dir {
        cv.text_right(COL_FILES_R, mid, Font::Regular, 8.0, GRIS_TEXTO, &fmt_int(row.files));
    }
    cv.text(COL_DATE_X, mid, Font::Regular, 8.0, GRIS_TEXTO, &fmt_date(row.mtime));
}

fn draw_summary_row(cv: &mut Canvas, y: f32, label: &str, bytes: Option<u64>, bold: bool, bg: Option<Rgb>, r: &Report) {
    let h = 17.0;
    if let Some(c) = bg {
        cv.rect(ML, y - h, CONTENT_W, h, c);
    }
    let font = if bold { Font::Bold } else { Font::Italic };
    let ty = y - 11.5;
    cv.text(ML + 6.0, ty, font, 8.0, NEGRO, &fit_end(font, 8.0, label, COL_NAME_W - 6.0));
    if let Some(b) = bytes {
        cv.text_right(COL_SIZE_R, ty, font, 8.0, GRIS_TEXTO, &fmt_bytes(b));
        cv.text_right(COL_PCT_R, ty, font, 8.0, GRIS_TEXTO, &fmt_pct(b, r.pct_base));
    }
}

pub fn to_pdf(r: &Report) -> Vec<u8> {
    let mut pages: Vec<Canvas> = Vec::new();
    let mut cv = Canvas::default();
    let mut y = first_page_top(&mut cv, r);
    table_header(&mut cv, y);
    y -= 16.0;

    let row_h = if r.with_dir { 25.0 } else { 15.0 };
    let shown = r.rows.len().min(PDF_MAX_ROWS);
    let extra_rest = r.rows.len() - shown;
    let rest_count = r.rest_count + extra_rest;
    let rest_size = r.rest_size.map(|b| b + r.rows[shown..].iter().map(|x| x.size).sum::<u64>());

    for (i, row) in r.rows[..shown].iter().enumerate() {
        if y - row_h < BODY_MIN_Y {
            pages.push(std::mem::take(&mut cv));
            running_header(&mut cv, &r.center);
            y = PAGE_H - 52.0;
            table_header(&mut cv, y);
            y -= 16.0;
        }
        let mut rs = Rowspec { cv: &mut cv, y, h: row_h, zebra: i % 2 == 1 };
        draw_row(&mut rs, r, row);
        y -= row_h;
    }

    let tail = (rest_count > 0) as u32 + r.total_row.is_some() as u32;
    if tail > 0 && y - 17.0 * (tail as f32) < BODY_MIN_Y {
        pages.push(std::mem::take(&mut cv));
        running_header(&mut cv, &r.center);
        y = PAGE_H - 52.0;
    }
    if rest_count > 0 {
        draw_summary_row(&mut cv, y, &format!("… y {} elementos más", fmt_int(rest_count as u64)), rest_size, false, None, r);
        y -= 17.0;
    }
    if let Some((label, bytes)) = &r.total_row {
        cv.line(ML, y, MR, y, AZUL_PRIMARIO, 0.8);
        draw_summary_row(&mut cv, y, label, Some(*bytes), true, Some(AZUL_TOTAL), r);
    }
    if r.rows.is_empty() && r.rest_count == 0 {
        cv.text(ML + 6.0, y - 14.0, Font::Italic, 9.0, GRIS_CLARO, "No hay elementos que mostrar.");
    }
    pages.push(cv);

    // Pie de página con la numeración (ya se conoce el total).
    let total = pages.len();
    for (i, p) in pages.iter_mut().enumerate() {
        p.line(ML, FOOT_Y + 10.0, MR, FOOT_Y + 10.0, AZUL_CLARO, 0.5);
        p.text(
            ML,
            FOOT_Y,
            Font::Regular,
            8.0,
            GRIS_CLARO,
            &fit_end(
                Font::Regular,
                8.0,
                &join_nonempty(&[&format!("SGIIPP-DISKANALIZER v{}", env!("CARGO_PKG_VERSION")), &r.unit, &r.center], " · "),
                CONTENT_W - 70.0,
            ),
        );
        p.text_right(MR, FOOT_Y, Font::Regular, 8.0, GRIS_CLARO, &format!("Pág. {} de {}", i + 1, total));
    }

    assemble(&pages, &r.title, &join_nonempty(&[&r.unit, &r.center], " · "))
}

/// Une las páginas en un archivo PDF válido (con tabla de referencias cruzadas).
fn assemble(pages: &[Canvas], title: &str, author: &str) -> Vec<u8> {
    let mut out: Vec<u8> = b"%PDF-1.4\n%\xE2\xE3\xCF\xD3\n".to_vec();
    let mut offsets: Vec<usize> = Vec::new();
    let first_page_id = 7usize;

    fn push(out: &mut Vec<u8>, offsets: &mut Vec<usize>, body: &[u8]) {
        offsets.push(out.len());
        let id = offsets.len();
        out.extend_from_slice(format!("{id} 0 obj\n").as_bytes());
        out.extend_from_slice(body);
        out.extend_from_slice(b"\nendobj\n");
    }

    push(&mut out, &mut offsets, b"<< /Type /Catalog /Pages 2 0 R >>");
    let kids: String = (0..pages.len()).map(|i| format!("{} 0 R", first_page_id + 2 * i)).collect::<Vec<_>>().join(" ");
    push(&mut out, &mut offsets, format!("<< /Type /Pages /Kids [{kids}] /Count {} >>", pages.len()).as_bytes());
    for base in ["Helvetica", "Helvetica-Bold", "Helvetica-Oblique"] {
        push(
            &mut out,
            &mut offsets,
            format!("<< /Type /Font /Subtype /Type1 /BaseFont /{base} /Encoding /WinAnsiEncoding >>").as_bytes(),
        );
    }
    push(
        &mut out,
        &mut offsets,
        format!(
            "<< /Title {} /Author {} /Creator (SGIIPP-DISKANALIZER {}) /Producer (SGIIPP-DISKANALIZER) >>",
            pdf_string(title),
            pdf_string(author),
            env!("CARGO_PKG_VERSION")
        )
        .as_bytes(),
    );
    for (i, p) in pages.iter().enumerate() {
        let content_id = first_page_id + 2 * i + 1;
        push(
            &mut out,
            &mut offsets,
            format!(
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 595 842] /Resources << /Font << /F1 3 0 R /F2 4 0 R /F3 5 0 R >> >> /Contents {content_id} 0 R >>"
            )
            .as_bytes(),
        );
        let mut body = format!("<< /Length {} >>\nstream\n", p.ops.len()).into_bytes();
        body.extend_from_slice(p.ops.as_bytes());
        body.extend_from_slice(b"endstream");
        push(&mut out, &mut offsets, &body);
    }

    let xref_at = out.len();
    out.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", offsets.len() + 1).as_bytes());
    for o in &offsets {
        out.extend_from_slice(format!("{o:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(
        format!("trailer\n<< /Size {} /Root 1 0 R /Info 6 0 R >>\nstartxref\n{xref_at}\n%%EOF\n", offsets.len() + 1).as_bytes(),
    );
    out
}

// ─────────────────────────── Pruebas ───────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(n: usize, with_dir: bool) -> Report {
        Report {
            center: DEFAULT_CENTER.into(),
            unit: DEFAULT_UNIT.into(),
            title: "Carpeta «Usuarios» — año (2026)".into(),
            subtitle: "\\\\srv-cis\\datos\\Usuarios".into(),
            details: if with_dir { vec!["Nombre: *.bak · Tamaño mínimo: 1 GB".into()] } else { vec![] },
            kpis: vec![
                Kpi { label: "Tamaño".into(), value: "134 GB".into(), note: "esta carpeta".into() },
                Kpi { label: "Archivos".into(), value: "1.027".into(), note: "en total".into() },
                Kpi { label: "Elementos".into(), value: "40".into(), note: "directos".into() },
                Kpi { label: "% del escaneo".into(), value: "44,9 %".into(), note: "de 299 GB".into() },
            ],
            pct_base: 1_000_000,
            total_row: Some(("Total de la carpeta".into(), 1_000_000)),
            rows: (0..n)
                .map(|i| Row {
                    name: format!("garcía.ñ{i} (copia) \\ \"x\"; 日本"),
                    dir: "\\\\srv-cis\\datos\\Usuarios\\una carpeta bastante larga\\otra más".into(),
                    path: format!("\\\\srv-cis\\datos\\Usuarios\\garcía.ñ{i}"),
                    is_dir: i % 3 == 0,
                    size: 1_000_000 / (i as u64 + 1),
                    files: i as u64 * 7,
                    mtime: 1_760_000_000 + i as i64 * 3600,
                })
                .collect(),
            rest_count: 12,
            rest_size: Some(4096),
            with_dir,
            generated: 1_760_000_000,
        }
    }

    #[test]
    fn number_and_date_formats() {
        assert_eq!(fmt_int(0), "0");
        assert_eq!(fmt_int(1027), "1.027");
        assert_eq!(fmt_int(1_234_567), "1.234.567");
        assert_eq!(fmt_bytes(900), "900 B");
        assert_eq!(fmt_bytes(1536), "1,50 KB");
        assert_eq!(fmt_bytes(134 * 1024 * 1024 * 1024), "134 GB");
        assert_eq!(fmt_bytes(16 * 1024 * 1024 * 1024 + 100 * 1024 * 1024), "16,1 GB");
        assert_eq!(fmt_bytes(8_900_000_000), "8,29 GB");
        assert_eq!(fmt_pct(449, 1000), "44,9 %");
        assert_eq!(fmt_pct(1, 0), "0,0 %");
        let (y, m, d, h, mi) = utc_parts(0);
        assert_eq!((y, m, d, h, mi), (1970, 1, 1, 0, 0));
        // 2026-10-21 18:24 UTC
        assert_eq!(utc_parts(1_792_607_040), (2026, 10, 21, 18, 24));
        assert_eq!(utc_parts(951_782_400), (2000, 2, 29, 0, 0)); // bisiesto
        assert_eq!(fmt_date(0), "—");
    }

    #[test]
    fn csv_is_excel_friendly_and_escaped() {
        let r = sample(3, false);
        let csv = to_csv(&r);
        assert_eq!(&csv[..3], &[0xEF, 0xBB, 0xBF], "BOM UTF-8");
        let s = String::from_utf8(csv).unwrap();
        assert!(s.contains("\r\n") && !s.contains("\r\r"));
        let lines: Vec<&str> = s.trim_start_matches('\u{feff}').split("\r\n").collect();
        assert!(lines[0].starts_with("Nombre;Ruta completa;Tipo;Tamaño (bytes);"));
        // el nombre lleva «;» y comillas → entre comillas y comillas duplicadas
        assert!(lines[1].starts_with("\"garcía.ñ0 (copia) \\ \"\"x\"\"; 日本\";"), "{}", lines[1]);
        assert!(lines[1].contains(";Carpeta;1000000;"));
        assert!(lines[1].contains(";100,00;")); // coma decimal
        assert!(lines[2].contains(";Archivo;500000;"));
        assert!(s.contains("(y 12 elementos más)"));
        assert!(s.contains("Total de la carpeta;;;1000000;"));
    }

    #[test]
    fn text_encoding_and_widths() {
        assert_eq!(pdf_string("a(b)\\c"), "(a\\(b\\)\\\\c)");
        assert_eq!(pdf_string("ñ"), "(\\361)");
        assert_eq!(pdf_string("—"), "(\\227)");
        assert_eq!(pdf_string("日"), "(?)");
        let w = text_width(Font::Regular, 10.0, "Hola");
        assert!((w - (722.0 + 556.0 + 222.0 + 556.0) / 100.0).abs() < 0.01);
        assert!(text_width(Font::Bold, 10.0, "Hola") > w);
        let f = fit_end(Font::Regular, 8.0, "una carpeta con un nombre larguísimo", 60.0);
        assert!(f.ends_with('…') && text_width(Font::Regular, 8.0, &f) <= 60.0);
        let g = fit_start(Font::Regular, 8.0, "\\\\servidor\\recurso\\carpeta\\subcarpeta", 80.0);
        assert!(g.starts_with('…') && g.ends_with("subcarpeta") && text_width(Font::Regular, 8.0, &g) <= 80.0);
        assert_eq!(fit_end(Font::Regular, 8.0, "corto", 200.0), "corto");
    }

    /// Comprueba la estructura: cabecera, tabla xref con los desplazamientos exactos y `%%EOF`.
    fn check_structure(pdf: &[u8]) -> usize {
        assert!(pdf.starts_with(b"%PDF-1.4\n"));
        assert!(pdf.ends_with(b"%%EOF\n"));
        let find_last = |needle: &[u8]| pdf.windows(needle.len()).rposition(|w| w == needle).unwrap();
        let sx = find_last(b"startxref\n");
        let at: usize = std::str::from_utf8(&pdf[sx + 10..]).unwrap().lines().next().unwrap().trim().parse().unwrap();
        assert!(pdf[at..].starts_with(b"xref\n"), "startxref apunta a xref");
        let xref = std::str::from_utf8(&pdf[at..]).unwrap(); // desde aquí todo es ASCII
        let mut lines = xref.lines();
        lines.next();
        let head: Vec<&str> = lines.next().unwrap().split(' ').collect();
        assert_eq!(head[0], "0");
        let count: usize = head[1].parse().unwrap();
        assert_eq!(lines.next().unwrap(), "0000000000 65535 f ");
        for id in 1..count {
            let off: usize = lines.next().unwrap()[..10].parse().unwrap();
            assert!(pdf[off..].starts_with(format!("{id} 0 obj\n").as_bytes()), "objeto {id} en su sitio");
        }
        let text = String::from_utf8_lossy(pdf);
        text.matches("/Type /Page ").count()
    }

    #[test]
    fn pdf_structure_one_and_many_pages() {
        let small = to_pdf(&sample(5, false));
        assert_eq!(check_structure(&small), 1);
        let big = to_pdf(&sample(400, true));
        let pages = check_structure(&big);
        assert!(pages >= 8, "400 filas de 25 pt no caben en {pages} páginas");
        let text = String::from_utf8_lossy(&big);
        assert!(text.contains(&format!("P\\341g. {pages} de {pages}")), "pie con el total de páginas");
        assert!(text.contains(&format!("P\\341g. 1 de {pages}")));
        // todo el contenido es ASCII salvo la cabecera binaria del principio
        assert!(big[16..].iter().all(|&b| b < 128));
        let empty = Report { rows: vec![], rest_count: 0, total_row: None, ..sample(0, false) };
        assert_eq!(check_structure(&to_pdf(&empty)), 1);
    }

    #[test]
    fn pdf_caps_rows_and_summarises_the_rest() {
        let n = PDF_MAX_ROWS + 50;
        let r = sample(n, false);
        let pdf = to_pdf(&r);
        check_structure(&pdf);
        let text = String::from_utf8_lossy(&pdf);
        assert!(text.contains(&format!("{} elementos m", fmt_int(62)))); // 12 + 50
    }

    #[test]
    fn center_and_unit_are_configurable() {
        let mut r = sample(60, false); // 2 páginas: la cabecera repetida empieza en la segunda
        r.center = "Centro Penitenciario de Prueba".into();
        r.unit = "Unidad de Pruebas".into();
        let pdf = to_pdf(&r);
        check_structure(&pdf);
        let t = String::from_utf8_lossy(&pdf);
        assert!(t.contains("(CENTRO PENITENCIARIO DE PRUEBA)"), "banda en mayúsculas");
        assert!(t.contains("(Centro Penitenciario de Prueba)"), "cabecera");
        assert!(t.contains("Unidad de Pruebas \\267 Generado el"), "servicio en la banda");
        assert!(t.contains("SGIIPP-DISKANALIZER v") && t.contains("Unidad de Pruebas \\267 Centro Penitenciario de Prueba"), "pie");
        assert!(!t.contains("Guillermo") && !t.contains("GUILLERMO"), "no queda el nombre por defecto");
        assert!(t.contains("/Author (Unidad de Pruebas \\267 Centro Penitenciario de Prueba)"));
        // sin servicio: no quedan separadores sueltos
        r.unit = String::new();
        let t2 = String::from_utf8_lossy(&to_pdf(&r)).into_owned();
        assert!(t2.contains("(Generado el ") && !t2.contains("\\267 \\267") && !t2.contains("( \\267"));
        // nombre larguísimo: se recorta y el PDF sigue siendo válido
        r.center = "X".repeat(150);
        check_structure(&to_pdf(&r));
    }

    #[test]
    fn ini_parsing() {
        let v = parse_ini("\u{feff}# comentario\r\n; otro\r\n\r\nCentro = \"CIS Murcia\"\r\nservicio=Informática \r\nsinigual\r\n=vacío\r\n");
        assert_eq!(v[0], ("centro".into(), "CIS Murcia".into()));
        assert_eq!(v[1], ("servicio".into(), "Informática".into()));
        assert_eq!(v.len(), 3); // «=vacío» tiene clave vacía y entra, pero no se usará
        assert_eq!(clean_name("  Centro   de \n Prueba  "), "Centro de Prueba");
        assert_eq!(clean_name(&"ñ".repeat(300)).chars().count(), 150);
    }

    /// Con `ESPACIO_DUMP=/carpeta` guarda ejemplos para revisarlos a ojo; si no, no hace nada.
    #[test]
    fn dump_samples_when_asked() {
        let Ok(dir) = std::env::var("ESPACIO_DUMP") else { return };
        std::fs::write(format!("{dir}/carpeta.pdf"), to_pdf(&sample(60, false))).unwrap();
        std::fs::write(format!("{dir}/busqueda.pdf"), to_pdf(&sample(45, true))).unwrap();
        std::fs::write(format!("{dir}/carpeta.csv"), to_csv(&sample(8, false))).unwrap();
    }
}
