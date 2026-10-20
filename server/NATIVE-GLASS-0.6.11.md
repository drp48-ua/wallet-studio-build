# Interfaz nativa glass — 0.6.11

La referencia visual es `wallet-studio-glassmorphism/public/styles.css` y su composición HTML. Solo se toma el diseño: fondo oscuro, luces cian/violeta, bordes iluminados, capas translúcidas y navegación. No se incorpora el servidor de demostración, sus datos, contadores, cuentas o acciones simuladas. No hay WebView, navegador integrado, nuevas dependencias ni DWM/backdrop blur.

`windows-source/src/glass.rs` pinta geometría vectorial acotada en la ventana egui existente. Los degradados y luces no hacen IO, red, decodificación de imágenes ni temporizadores permanentes por fotograma. Los botones conservan widgets y semántica de egui, sus estados habilitados y las mismas llamadas de aplicación.

Los helpers compartidos, la cabecera, navegación, barra de estado y dock de acciones usan el nuevo estilo. Biblioteca se divide en catálogo y selección a partir de 900 puntos de contenido; en ventanas pequeñas se apila. La carga del diseño seleccionado mantiene prioridad sobre las miniaturas. El formulario de acceso y sus estados existentes reciben un panel de cristal y una ilustración vectorial.

Se conserva todo el motor 0.6.10: login y sesiones, exclusión/recuperación del almacenamiento, máximo de equipos, licencias, copias y diarios, captura, dispositivo, aplicación/restauración y catálogo privado nativo. No se cambia el backend, la metadata comercial, sus límites ni los originales. La metadata adicional de los seis diseños y el diagnóstico del backend corresponden al lead.

## Revisión visual aislada

El mismo EXE admite `--wallet-studio-demo` con carpeta temporal nueva, sin cuenta, red ni teléfono. `--wallet-studio-auth-demo` muestra los estados de acceso sin credenciales. `--wallet-studio-library-demo` abre Biblioteca. La variable `WALLET_STUDIO_DEMO_DESIGNS_DIR` puede señalar una carpeta local de PNG de ejemplo; solo dentro de demo se extraen sus dimensiones verificadas para mostrar archivos del tamaño real. Ningún dato demo se incorpora al perfil de producción.

El lead debe revisar ventana pequeña y maximizada, acceso, Tarjetas vacío/con ejemplos, Biblioteca/selección/Usar diseño, Teclado, Cuenta y Ayuda. Aplicar/restaurar siguen bloqueados en demo; no se deben declarar transferencias reales.

## Validación y recepción

Pruebas ejecutadas: suite Rust requerida, 170 pruebas de aplicación y una de motor correctas, dos ignoradas y cuatro de hardware excluidas; incluye cuatro pruebas nuevas de geometría, controles deshabilitados y composición compacta/amplia. Integración sintética Rust/WinHTTP/DPAPI con servidor local: una prueba correcta. Tras integrar `4a1b967` y `5449e4e`, 12 pruebas de `design_library` correctas contra el catálogo final. `git diff --check` correcto. El lead revisó y aceptó el código congelado antes de la única compilación de producción. Compilación GNU de producción finalizada correctamente en 3 min 19 s; recurso FileVersion/ProductVersion 0.6.11. La aceptación visual real del host corresponde al lead. El worker no usa AppData, credenciales ni un teléfono para este cambio.

Se revisó la contribución `8e11473`: se adaptaron sus envoltorios visuales de importación de temas y marco de teclado. La navegación, los paneles y los botones usan el módulo integrado `glass.rs`; se conservan los controles nativos y no se presentan contadores de biblioteca como copias originales verificadas.

Comandos: `cargo +stable-x86_64-pc-windows-gnu test --release --locked --offline --manifest-path windows-source/Cargo.toml -- --skip test_usbmux_query --skip test_list_connected_devices --skip test_syslog_service_receive --skip test_saved_cards_purging`; `python server/tests/desktop_integration.py`; `cargo +stable-x86_64-pc-windows-gnu test --release --locked --offline --manifest-path windows-source/Cargo.toml --bin aircard design_library -- --test-threads=1`. Se utilizó el runtime GNU existente, sin instalar dependencias. Los logs locales ignorados se conservan en `server/test-runtime/{login-storage-required-tests.log,login-storage-desktop-integration.log,native-glass-final-catalog.log,native-glass-build.log}`. No se repitió la suite Python del backend para este cambio visual; los cambios de servidor integrados y su aceptación corresponden al lead.


## Candidato inicial retirado tras revisión visual

- EXE: `windows/Wallet Studio.exe`, 12.715.008 bytes; SHA-256 `03ce05e5832963aa12fce901c8449dd15e94c389b4fa8787e27600912871d952`.
- ZIP: `dist/Wallet-Studio-0.6.11-Windows.zip`, 8.351.382 bytes; SHA-256 `1d2e0b7c97d41011749694a01ab7f84cf026ff42b04ac2bf67461f804d1dcf93`.
- Recepción local ignorada: `server/test-runtime/native-glass-artifact-receipt.json`. CRC de todas las entradas correcto; EXE empaquetado idéntico; 112 hashes privados de los catálogos principal/legacy excluidos; sin runtime privado, diseños, Node, HTML/CSS/JS; LEEME y novedades 0.6.11 presentes. El paquete conserva las licencias existentes. El binario no tiene firma Authenticode.

La compilación contiene la base integrada `5449e4e` y los cambios visuales revisados. Después de compilar se añadió únicamente una prueba ignorada `cfg(test)` de diagnóstico de descargas: no modifica el código de producción ni invalida el hash anterior. Este diagnóstico requiere activación explícita y una sesión guardada; no inicializa almacenamiento ni escribe cachés, no imprime identidades, credenciales, bytes, cabeceras o errores sin filtrar. Solo emite IDs de catálogo validados, indicador nativo y categorías fijas. Su objetivo es investigar fallos observados por el lead: la integridad del paquete no demuestra disponibilidad de todos los objetos del servidor.

Pendiente de aceptación: revisión visual del host en tamaños pequeño/grande, y diagnóstico/solución del servidor de diseños a cargo del lead. No se probó ni modificó un teléfono para esta interfaz.


## Corrección de composición detectada en host

El primer candidato mostraba únicamente el fondo: la capa personalizada `Order::Background` se componía sobre los paneles estándar. Se retiró ese candidato. La corrección dibuja en `egui::LayerId::background()` antes de crear los paneles, por lo que el mismo orden de inserción garantiza contenido posterior visible. La nueva prueba `ambient_is_painted_before_visible_panel_content` inspecciona las formas finales y exige que el primer mesh ambiental preceda al texto de los paneles. Se vuelve a compilar únicamente por este fallo confirmado; los hashes finales sustituirán los anteriores.


La revisión dirigida posterior a la corrección ejecutó las cinco pruebas de `glass` correctamente (incluye el orden final de pintura). El primer intento de enlazar coincidió con el diagnóstico host utilizando el EXE de pruebas y falló por bloqueo de archivo; tras terminar ese proceso, se repitió el enlace y pasó. El lead ejecutó el diagnóstico protegido con la cuenta existente: 112/112 GET verificados, 56 originales nativos y 56 vistas previas, sin errores, en 184,66 s. Una ejecución agregada anterior tuvo fallos transitorios; este resultado no garantiza disponibilidad futura ni sustituye la validación de cada descarga. No se relajó ninguna comprobación.
