import { useEffect, useRef, type ReactNode } from "react";

interface Section {
  id: string;
  title: string;
  body: ReactNode;
}

const K = ({ children }: { children: ReactNode }) => <kbd>{children}</kbd>;

const SECTIONS: Section[] = [
  {
    id: "que-es",
    title: "Para qué sirve",
    body: (
      <>
        <p>
          SGIIPP-DISKANALIZER muestra qué carpetas y archivos ocupan más sitio en una unidad local o de red, como hacen TreeSize o
          WinDirStat. Sirve para localizar rápidamente dónde se está llenando un disco y, si hace falta, limpiar desde
          la propia aplicación.
        </p>
        <p>Funciona solo en Windows y usa las credenciales de la sesión con la que se ha iniciado el equipo.</p>
      </>
    ),
  },
  {
    id: "empezar",
    title: "Primeros pasos: escanear",
    body: (
      <>
        <ol>
          <li>
            Escribe la ruta en la barra superior: una carpeta de red (<code>\\servidor\recurso</code>), una unidad (
            <code>Z:\</code>) o una carpeta (<code>C:\Datos</code>). También puedes pulsar el botón de una unidad o
            «Examinar…».
          </li>
          <li>
            Elige los <strong>hilos</strong>. En red suelen ir bien 16 o 32; en un disco duro mecánico local, 4 u 8.
          </li>
          <li>
            Pulsa <strong>Escanear</strong> (o Intro). Las últimas rutas quedan recordadas en el campo de texto.
          </li>
        </ol>
        <p>
          Al abrir el programa, la pantalla de inicio muestra una tarjeta por cada unidad con su espacio libre y una
          barra de ocupación (ámbar a partir del 75 % y rojo a partir del 90 %), y la lista de las últimas rutas. Un
          clic en una tarjeta o en una ruta reciente inicia el escaneo directamente.
        </p>
        <p className="callout">
          <strong>Importante:</strong> ejecuta la aplicación como usuario normal, <em>no como administrador</em>. Un
          proceso con permisos elevados no ve las unidades de red asignadas en la sesión normal.
        </p>
      </>
    ),
  },
  {
    id: "vivo",
    title: "Mientras escanea: vista en vivo",
    body: (
      <>
        <p>
          Durante el escaneo se muestran los contadores (archivos, carpetas, tamaño, tiempo y velocidad) y, debajo, la
          tabla y el mapa de las carpetas de primer nivel con sus tamaños creciendo. No hace falta esperar al final
          para ver dónde está el espacio.
        </p>
        <p>
          <strong>Detener y ver lo escaneado</strong> no descarta nada: muestra lo leído hasta ese momento y lo marca
          como «Escaneo parcial».
        </p>
      </>
    ),
  },
  {
    id: "pantalla",
    title: "Entender los resultados",
    body: (
      <>
        <p>
          Arriba aparece el tamaño total, el número de archivos y carpetas, la duración y, si los hay, los elementos
          sin acceso. Debajo hay cinco pestañas:
        </p>
        <ul>
          <li>
            <strong>Carpetas:</strong> tabla de la carpeta actual y mapa de colores (treemap) a su lado. Cada
            rectángulo es proporcional al espacio que ocupa. Al pasar el ratón por encima se ve la ruta completa, el
            tamaño, el porcentaje, los archivos y la fecha.
          </li>
          <li>
            <strong>Buscar:</strong> filtros por nombre, tipo, tamaño, antigüedad y extensión.
          </li>
          <li>
            <strong>Archivos más grandes:</strong> los 300 mayores de todo el escaneo, estén donde estén.
          </li>
          <li>
            <strong>Por extensión:</strong> cuánto ocupa cada tipo de archivo (.pdf, .bak, .iso…).
          </li>
          <li>
            <strong>Errores:</strong> carpetas que no se pudieron leer y el motivo (por ejemplo, sin permisos).
          </li>
        </ul>
        <p>
          El tamaño es el <em>tamaño lógico</em> de los archivos, no el espacio que ocupan realmente en el disco, que
          puede ser algo mayor por el tamaño de los clústeres.
        </p>
        <p>
          En la columna <strong>%</strong> una barra muestra el peso de cada elemento dentro de la carpeta: azul si es
          pequeño, ámbar a partir del 15 % y rojo a partir del 40 %. Al pasar el ratón por una fila se resalta su
          rectángulo en el mapa, y al pasarlo por el mapa se resalta su fila.
        </p>
        <p>
          En la fila de las unidades, a la derecha, están los botones <strong>Tema</strong> (automático, claro u
          oscuro; el automático sigue el de Windows), <strong>Ajustes</strong> (nombre del centro para los informes) y{" "}
          <strong>Manual</strong> (F1). En la parte inferior se recuerdan los atajos principales.
        </p>
      </>
    ),
  },
  {
    id: "moverse",
    title: "Moverse por las carpetas",
    body: (
      <ul>
        <li>Haz clic en el nombre de una carpeta, o en su rectángulo del mapa, para entrar.</li>
        <li>
          Usa las migas de pan (la ruta sobre la tabla) para volver a cualquier nivel, o la flecha hacia arriba. También
          sirven <K>Retroceso</K> y <K>Alt</K>+<K>←</K>.
        </li>
        <li>
          El botón <strong>⟳ Actualizar</strong> (o <K>F5</K>) vuelve a leer del disco solo la carpeta actual, sin
          repetir todo el escaneo. Es útil si alguien ha cambiado archivos mientras tanto.
        </li>
        <li>Pulsa en la cabecera de una columna para ordenar por nombre, tamaño, archivos o fecha.</li>
      </ul>
    ),
  },
  {
    id: "buscar",
    title: "Buscar y filtrar",
    body: (
      <>
        <p>
          Abre la pestaña <strong>Buscar</strong> o pulsa <K>Ctrl</K>+<K>F</K>. Los resultados se actualizan solos al
          cambiar cualquier filtro y se ordenan de mayor a menor tamaño (hasta 500 en pantalla).
        </p>
        <dl>
          <dt>Nombre</dt>
          <dd>
            Un fragmento del nombre (<code>backup</code>) o un patrón con comodines: <code>*</code> sustituye a
            cualquier texto y <code>?</code> a un carácter (<code>*.bak</code>, <code>informe?</code>).
          </dd>
          <dt>Tipo</dt>
          <dd>
            Archivos, carpetas o ambos. El total en bytes solo se calcula al buscar archivos (con carpetas se contaría
            dos veces lo mismo).
          </dd>
          <dt>Tamaño mínimo</dt>
          <dd>En KB, MB o GB.</dd>
          <dt>Sin tocar</dt>
          <dd>Lo que no se modifica desde hace más de un mes, seis meses, uno, dos o cinco años.</dd>
          <dt>Extensiones</dt>
          <dd>
            Una o varias separadas por comas: <code>iso, zip, bak</code>.
          </dd>
        </dl>
      </>
    ),
  },
  {
    id: "borrar",
    title: "Seleccionar y eliminar",
    body: (
      <>
        <p className="callout danger">
          <strong>El borrado es definitivo:</strong> los elementos no van a la papelera y no se pueden recuperar.
        </p>
        <ol>
          <li>
            Marca las casillas de la tabla de carpetas o de los resultados de búsqueda. Con <K>Shift</K>+clic se marca
            un rango; con <K>Ctrl</K>+clic se suma o se quita un elemento; <K>Ctrl</K>+<K>A</K> marca todo lo visible.
          </li>
          <li>
            Pulsa <strong>Eliminar…</strong> (o <K>Supr</K>) y revisa el resumen.
          </li>
          <li>
            Si lo seleccionado supera <strong>5 GB</strong> (<strong>1 GB</strong> en unidades de red), escribe{" "}
            <strong>ELIMINAR</strong> para habilitar el botón.
          </li>
          <li>Confirma. Verás el progreso y podrás detenerlo con el botón «Detener».</li>
        </ol>
        <p>
          Si marcas una carpeta y algo de dentro, solo se borra la carpeta (que ya lo incluye). La carpeta raíz del
          escaneo y las raíces de unidad no se pueden borrar. Si un archivo no se puede eliminar, el programa sigue con
          el resto y al terminar te dice cuáles han fallado y por qué. La lista se vuelve a leer del disco, así que
          siempre refleja lo que queda.
        </p>
      </>
    ),
  },
  {
    id: "exportar",
    title: "Exportar informes (PDF y CSV)",
    body: (
      <>
        <p>
          El botón <strong>Exportar…</strong> aparece a la derecha de las migas de pan y en la pestaña Buscar. Genera
          el informe de la carpeta actual o de los resultados de la búsqueda, según donde estés.
        </p>
        <ul>
          <li>
            <strong>Informe PDF:</strong> portada con el título y la ruta, tarjetas de resumen, tabla con barras de
            peso, fila de total y numeración de páginas. Incluye hasta 3.000 filas; el resto se resume en una línea.
          </li>
          <li>
            <strong>Archivo CSV (para Excel):</strong> se abre directamente en Excel con acentos correctos. Trae la
            ruta completa y el tamaño exacto en bytes, hasta 50.000 filas.
          </li>
        </ul>
        <p>
          Al terminar aparece un aviso con el enlace «Mostrar en el Explorador» para abrir la carpeta donde se ha
          guardado.
        </p>
        <h3>Nombre del centro en los informes</h3>
        <p>
          En <strong>Ajustes</strong> (arriba a la derecha) puedes escribir el <em>nombre del centro</em> y el{" "}
          <em>servicio o unidad</em>. Salen en la banda, la cabecera y el pie del PDF. Si los dejas vacíos se usan los
          valores por defecto. El botón «Restablecer» los vacía.
        </p>
        <p>
          Para dejar el nombre fijado en todos los equipos, crea un archivo <code>espacio.ini</code> junto al programa
          (codificación UTF-8) con estas dos líneas:
        </p>
        <pre>
          <code>
            {"centro=Nombre del centro\nservicio=Nombre del servicio"}
          </code>
        </pre>
        <p>
          Prioridad: lo escrito en Ajustes, después <code>espacio.ini</code>, después el valor por defecto.
        </p>
      </>
    ),
  },
  {
    id: "menu",
    title: "Menú contextual (clic derecho)",
    body: (
      <p>
        Con el botón derecho sobre una fila o un rectángulo del mapa se abre un menú con: abrir carpeta, mostrar en el
        Explorador, copiar la ruta, seleccionar y eliminar. Se cierra con <K>Esc</K> o haciendo clic fuera.
      </p>
    ),
  },
  {
    id: "atajos",
    title: "Atajos de teclado",
    body: (
      <table>
        <caption className="sr-only">Atajos de teclado de SGIIPP-DISKANALIZER</caption>
        <thead>
          <tr>
            <th scope="col">Tecla</th>
            <th scope="col">Acción</th>
          </tr>
        </thead>
        <tbody>
          {(
            [
              ["F1", "Abrir o cerrar este manual"],
              ["F5", "Actualizar la carpeta actual desde el disco"],
              ["Ctrl + F", "Ir a Buscar"],
              ["Ctrl + A", "Seleccionar todo lo visible"],
              ["Supr", "Eliminar lo seleccionado (pide confirmación)"],
              ["Esc", "Quitar la selección, cerrar un menú o una ventana"],
              ["Retroceso / Alt + ←", "Subir a la carpeta superior"],
              ["Shift + clic", "Seleccionar un rango de filas"],
              ["Ctrl + clic", "Sumar o quitar una fila de la selección"],
              ["Intro", "Escanear (en el campo de la ruta)"],
            ] as [string, string][]
          ).map(([k, a]) => (
            <tr key={k}>
              <th scope="row">
                <kbd>{k}</kbd>
              </th>
              <td>{a}</td>
            </tr>
          ))}
        </tbody>
      </table>
    ),
  },
  {
    id: "problemas",
    title: "Problemas frecuentes",
    body: (
      <dl>
        <dt>No veo mi unidad de red</dt>
        <dd>
          Comprueba que no estás ejecutando la aplicación como administrador y que la unidad está asignada en tu
          sesión. También puedes escribir directamente la ruta <code>\\servidor\recurso</code>.
        </dd>
        <dt>Hay carpetas «sin acceso»</dt>
        <dd>
          Son carpetas para las que tu usuario no tiene permiso de lectura. En la pestaña Errores aparecen con el
          motivo. Su tamaño no se incluye en el total.
        </dd>
        <dt>No se puede eliminar un archivo</dt>
        <dd>
          Los motivos más habituales son «En uso por otro programa o usuario» (hay que cerrarlo), «Sin permisos» o
          «Ruta demasiado larga». El programa borra todo lo demás y te indica qué ha quedado.
        </dd>
        <dt>El tamaño no coincide con el del Explorador</dt>
        <dd>
          SGIIPP-DISKANALIZER suma el tamaño lógico de los archivos y no cuenta enlaces simbólicos ni uniones, para no contar dos
          veces lo mismo. El Explorador puede mostrar el espacio ocupado en disco.
        </dd>
        <dt>El informe PDF sale con otro nombre de centro</dt>
        <dd>
          Revisa <strong>Ajustes</strong>: lo escrito allí manda sobre <code>espacio.ini</code>. Si quieres usar el
          valor del archivo o el de fábrica, vacía el campo con «Restablecer» y guarda. Los Ajustes son por usuario de
          Windows, así que otro usuario del mismo equipo puede ver un nombre distinto.
        </dd>
        <dt>El escaneo de red es lento</dt>
        <dd>Sube el número de hilos (16 o 32). En discos mecánicos locales conviene bajarlo a 4 u 8.</dd>
      </dl>
    ),
  },
  {
    id: "acerca",
    title: "Acerca de",
    body: (
      <>
        <p>
          SGIIPP-DISKANALIZER v{__APP_VERSION__}
          <br />
          Servicios de Informática · CIS Guillermo Miranda · Murcia
          <br />
          Secretaría General de Instituciones Penitenciarias
        </p>
        <p>Los créditos también están en el botón «Acerca de» de la barra superior.</p>
        <p>
          El historial de cambios de cada versión está en el archivo <code>CHANGELOG.md</code> que acompaña a la
          aplicación.
        </p>
      </>
    ),
  },
];

interface Props {
  onClose: () => void;
}

export default function Manual({ onClose }: Props) {
  const panelRef = useRef<HTMLDivElement>(null);
  const titleRef = useRef<HTMLHeadingElement>(null);
  const contentRef = useRef<HTMLDivElement>(null);
  const closeRef = useRef(onClose);
  closeRef.current = onClose;

  // Al abrir, el foco pasa al título; al cerrar, vuelve a donde estaba.
  useEffect(() => {
    const prev = document.activeElement as HTMLElement | null;
    titleRef.current?.focus();
    return () => prev?.focus?.();
  }, []);

  // Esc / F1 cierran. Tab se queda dentro de la ventana.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape" || e.key === "F1") {
        e.preventDefault();
        e.stopPropagation();
        closeRef.current();
        return;
      }
      if (e.key !== "Tab") return;
      const nodes = panelRef.current?.querySelectorAll<HTMLElement>(
        'button, a[href], input, select, textarea, [tabindex]:not([tabindex="-1"])',
      );
      if (!nodes || nodes.length === 0) return;
      const first = nodes[0];
      const last = nodes[nodes.length - 1];
      const active = document.activeElement;
      if (e.shiftKey && (active === first || !panelRef.current?.contains(active))) {
        e.preventDefault();
        last.focus();
      } else if (!e.shiftKey && active === last) {
        e.preventDefault();
        first.focus();
      }
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, []);

  const go = (id: string) => {
    const el = contentRef.current?.querySelector<HTMLElement>(`#man-${id}`);
    if (!el) return;
    el.scrollIntoView({ block: "start" });
    el.focus();
  };

  return (
    <div className="manual-back" onMouseDown={onClose}>
      <div
        ref={panelRef}
        className="manual"
        role="dialog"
        aria-modal="true"
        aria-labelledby="man-title"
        onMouseDown={(e) => e.stopPropagation()}
      >
        <header className="manual-head">
          <h1 id="man-title" tabIndex={-1} ref={titleRef}>
            Manual de SGIIPP-DISKANALIZER
          </h1>
          <span className="muted">v{__APP_VERSION__}</span>
          <button className="manual-close" onClick={onClose}>
            Cerrar <kbd>Esc</kbd>
          </button>
        </header>
        <div className="manual-body">
          <nav aria-label="Índice del manual" className="manual-toc">
            <ol>
              {SECTIONS.map((s) => (
                <li key={s.id}>
                  <a
                    href={`#man-${s.id}`}
                    onClick={(e) => {
                      e.preventDefault();
                      go(s.id);
                    }}
                  >
                    {s.title}
                  </a>
                </li>
              ))}
            </ol>
          </nav>
          <div className="manual-content" ref={contentRef}>
            {SECTIONS.map((s) => (
              <section key={s.id} aria-labelledby={`man-${s.id}`}>
                <h2 id={`man-${s.id}`} tabIndex={-1}>
                  {s.title}
                </h2>
                {s.body}
              </section>
            ))}
          </div>
        </div>
      </div>
    </div>
  );
}
