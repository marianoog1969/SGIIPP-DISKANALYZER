/**
 * Novedades que muestra el manual (sección «Novedades»), de la más reciente a la más antigua.
 * Hay que añadir aquí una entrada en cada versión, con su fecha (AAAA-MM-DD), igual que en CHANGELOG.md.
 */
export interface Release {
  version: string;
  date: string;
  items: string[];
}

export const RELEASES: Release[] = [
  {
    version: "1.6.3",
    date: "2026-11-06",
    items: [
      "Corregido el nombre de la aplicación: SGIIPP-DISKANALYZER (con Y), también en el ejecutable y en la carpeta de datos.",
    ],
  },
  {
    version: "1.6.2",
    date: "2026-11-06",
    items: [
      "Corregido: en «Buscar» y «Archivos más grandes» la ruta de cada archivo se superponía al nombre.",
    ],
  },
  {
    version: "1.6.1",
    date: "2026-11-05",
    items: [
      "Mapa de disco rediseñado: huecos entre rectángulos, esquinas redondeadas, colores más armoniosos y etiquetas más legibles.",
      "Anchos de columna por defecto reajustados para dejar más sitio al nombre.",
    ],
  },
  {
    version: "1.6.0",
    date: "2026-11-02",
    items: [
      "Nueva pestaña «Duplicados»: busca archivos con el mismo contenido, muestra cuánto espacio se recuperaría y permite eliminar las copias sobrantes (siempre se conserva una).",
      "Columna «Cambio»: compara cada carpeta con el escaneo anterior de la misma ruta (crece, baja o es nueva). Se puede desactivar y borrar el historial en Ajustes.",
      "El mapa muestra también las subcarpetas dentro de cada carpeta; un clic entra directamente en la subcarpeta.",
      "Barra de capacidad de la unidad en la cabecera (usado y libre).",
      "Navegación: botones Atrás y Adelante (Alt+← y Alt+→) y doble clic para entrar en una carpeta. Retroceso sigue subiendo un nivel.",
      "Filtro rápido por nombre encima de la tabla de carpetas.",
      "Columnas de la tabla con ancho ajustable (se recuerda).",
      "Los atajos de teclado pasan al botón «Atajos»; el pie queda más limpio.",
      "Esta sección de Novedades, con la fecha de cada versión.",
    ],
  },
  {
    version: "1.5.0",
    date: "2026-10-30",
    items: [
      "«Acerca de»: apartado «Apoya el proyecto» con código QR, botón «Donar con PayPal» y «Copiar enlace».",
      "Créditos sin mención a la Secretaría General.",
    ],
  },
  {
    version: "1.4.1",
    date: "2026-10-28",
    items: ["Nuevo diálogo «Acerca de» con los créditos (Servicios de Informática, CIS Guillermo Miranda, Murcia). También desde el pie."],
  },
  {
    version: "1.4.0",
    date: "2026-10-25",
    items: [
      "Columna «%» con barra de color según el peso (azul; ámbar desde el 15 %; rojo desde el 40 %).",
      "Resaltado enlazado: la fila bajo el ratón se marca en el mapa y al revés.",
      "Tema automático, claro u oscuro, que se recuerda.",
      "Pantalla de inicio con las unidades y su espacio libre, y las últimas rutas; un clic escanea.",
    ],
  },
  {
    version: "1.3.3",
    date: "2026-10-24",
    items: ["Manual actualizado: botones Ajustes y Manual, y problema frecuente sobre el nombre del centro."],
  },
  {
    version: "1.3.2",
    date: "2026-10-21",
    items: ["La aplicación pasa a llamarse SGIIPP-DISKANALYZER (título, manual, informes y ejecutable)."],
  },
  {
    version: "1.3.0",
    date: "2026-10-21",
    items: [
      "Manual de ayuda integrado y accesible (F1).",
      "«Ajustes»: nombre del centro y del servicio para los informes PDF, o archivo espacio.ini junto al programa.",
    ],
  },
  {
    version: "1.2.0",
    date: "2026-10-21",
    items: [
      "Exportar informes en PDF y en CSV para Excel, con el estilo de los informes del centro.",
      "Confirmación reforzada al borrar mucho espacio: hay que escribir ELIMINAR.",
      "Selección como en el Explorador: Shift+clic para un rango y Ctrl+clic para sumar o quitar.",
    ],
  },
  {
    version: "1.1.0",
    date: "2026-10-20",
    items: [
      "Vista en vivo durante el escaneo: la tabla y el mapa se llenan mientras se lee el disco.",
      "Pestaña «Buscar» (Ctrl+F) por nombre, tipo, tamaño, antigüedad y extensión.",
      "Menú contextual con clic derecho y atajos de teclado (F5, Ctrl+A, Supr, Esc).",
      "Mensajes de error claros en español.",
    ],
  },
  {
    version: "1.0.1",
    date: "2026-10-20",
    items: [
      "Corregido: la vista no se actualizaba después de borrar.",
      "Borrado de carpetas grandes rehecho: en paralelo, con progreso y botón «Detener».",
      "Botón «Actualizar» para volver a leer la carpeta actual.",
    ],
  },
  {
    version: "1.0.0",
    date: "2026-10-20",
    items: [
      "Primera versión: escáner paralelo de unidades locales y de red con progreso en vivo.",
      "Vistas de carpetas con mapa, archivos más grandes, extensiones y errores. Selección y eliminación con confirmación.",
    ],
  },
];

/** «2026-11-02» → «02/11/2026» */
export function fmtReleaseDate(iso: string): string {
  const [y, m, d] = iso.split("-");
  return `${d}/${m}/${y}`;
}
