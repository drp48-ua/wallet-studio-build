# Auditoría aislada del recorrido de cliente

Fecha: 28/10/2026. Base publicada: 0.6.26, main 8917fc72133c4f5f79bb9f9ad924639f2747911d. Rama: codex/isolated-journey-audit-20261028. El usuario solicita probar fallos y corregirlos, desde compra hasta transferencia. Se mantiene la autorización anterior para subir cambios revisados a main y publicar actualizaciones firmadas. Las pruebas destructivas utilizan exclusivamente datos ficticios y carpetas temporales.

## Fallos reproducidos y cambios

1. Shopify entrega todavía Wallet-Studio-0.6.17-Windows.zip, 8.373.267 bytes, desde el producto activo. Se comprobó con el conector Digital Products. Pendiente sustituir esa entrega por el paquete nuevo validado; no se han creado pedidos ni efectuado pagos reales.
2. Un envío de correo antiguo que falla después de un reenvío válido borraba el código nuevo. Una prueba concurrente lo reprodujo con invalid_activation. El borrado se limita ahora al digest y fecha del código cuyo envío falló. La regresión pasa.
3. La preparación de una importación creaba prematuramente .cloud-import.lock; al terminar el proceso dejaba un bloqueo global huérfano. La preparación depende ahora del bloqueo de operación del sistema, que se libera al terminar el proceso.
4. Un error de escritura con identidad conocida podía retirar la barrera después de escribir parcialmente. Antes de cualquier modificación se guarda el ZIP exacto por digest y una intención duradera. Solo se retira la barrera tras lectura exacta y validación del destino. Una escritura parcial conserva el bloqueo. No hay reproducción automática ni retirada de marcadores antiguos.
5. El ensayo nativo local pasó, pero su limpieza falló al superar el límite de rutas abreviadas de Windows. Se corrigieron las rutas del entorno de integración y se espera el cierre de sus manejadores antes de limpiar. El recorrido y su limpieza pasan con rutas largas.

## Evidencia obtenida

| Recorrido o fallo | Entorno | Resultado observado |
| --- | --- | --- |
| Pago autenticado, activación, sesión y licencia | HTTP local, correo capturado en memoria | Correcto; no se omite la licencia ni se habilita registro libre |
| Dos cuentas nuevas y segundo equipo | HTTP local, identidades ficticias | Copia accesible a su cuenta; otra cuenta obtiene 404 y lista vacía |
| Biblioteca y resolución | 56 imágenes nativas ficticias, 1536×969 | Metadatos, descarga exacta y caché verificados |
| Cierre de sesión | HTTP local | Revoca esa sesión y conserva la del segundo equipo |
| Correo antiguo fallido tras reenvío | Hilos y reloj controlados | Código nuevo utilizable después de la corrección |
| Cancelación, reembolso, código vencido, intentos y tres equipos | Suite de cuentas del servidor | Pruebas existentes incluidas en la suite completa |
| Interrupción durante preparación | Proceso nativo ficticio terminado realmente | Libera propietario y permite reintento sin marcador huérfano |
| Interrupción antes/después de empezar a escribir | Proceso nativo ficticio terminado realmente | Conserva intención y ZIP; impide reentrada insegura |
| Corrupción posterior a escritura | Inyección sobre archivos ficticios | Conserva la barrera; no comunica importación verificada |
| Propietario activo y marcadores antiguos | Dos procesos y archivos ficticios | No se salta el propietario ni se retiran bloqueos existentes |
| Incidencias de otro iPhone, cambio de sesión/cuenta, originales corruptos | Suite nativa con identidades ficticias | Controles de identidad, aislamiento y evidencia conservados |
| Biblioteca sin duplicados y bytes preparados | Suite nativa | Incluye codificaciones PNG distintas y aplicaciones repetidas |
| Componentes Apple ausentes/disponibles | Pruebas de lógica y demostración anterior 0.6.26 | No equivale a instalar/desinstalar iTunes en Windows limpio |
| Sin iTunes en Windows limpio | Pendiente | Sandbox no instalado; máquinas de VirtualBox inaccesibles. El usuario pidió dejar esta prueba pendiente |
| Pago real, correo externo y entrega tras pago | Pendiente | Los adaptadores externos no se ejercitan mediante las cuentas ficticias |
| Captura y aplicación en iPhone físico | Pendiente | Se solicitó disponibilidad; no se ha modificado ningún teléfono |

Suite del servidor: 99 pruebas, cero fallos, 3 omitidas. Dos nuevas pruebas del recorrido pasan; el fallo de correo fue reproducido antes de corregirlo. Suite nativa final del responsable: 251 correctas, cero fallidas, 8 ignoradas y 4 exclusiones de hardware en 42,14 s. Ensayo nativo HTTP adicional: una prueba correcta, con dos perfiles ficticios, sincronización, archivo ausente/corrupto y aislamiento de sesión; limpieza final correcta tras arreglar las rutas largas. El implementador configurado obtuvo además 145 pruebas dirigidas correctas. Los recuentos se solapan; no se suman.

Registros, paquetes, cuentas y archivos ficticios se conservan bajo server/test-runtime o work/test-temp, excluidos de Git. No se utilizaron claves ni originales privados para estas pruebas.

## Continuación y límites

PR #10 incorporada a main: 7ac9c66e0bd489fb292bc3e9619223e7fae3c12f. Las dos comprobaciones Windows/MSVC previas terminaron correctamente (40687268709 y 40687254813). La publicación de main 40689582801 terminó correctamente y publicó 0.6.27, build 4, etiqueta desktop-build-4. Se descargó el paquete público y se verificaron la firma RSA-PSS del manifiesto, su vínculo con repositorio/main/commit, tamaño, SHA256, ejecutable y 676 entradas sin datos privados.

- ZIP público SHA256: 8530bca9916e0cc8e3591d20122d1d69ef969ce15848e19db0cc00e973ade783.
- EXE público SHA256: 4293650c10d85ed264156357168f6ec13b837325da5fe9dafc20bbc0291f8452.
- Publicación: https://github.com/drp48-ua/wallet-studio-build/releases/tag/desktop-build-4.

El servidor wallet-studio-accounts se desplegó manualmente al mismo commit de main. Render confirmó Deploy succeeded / Live, duración 46,4 s. /health respondió correctamente y /api/designs sin sesión devolvió 401. No se cambiaron secretos, plan ni rama de despliegue. La corrección del correo está desplegada; no se envió correo real de prueba.

La tienda pública permitió añadir una unidad del producto correcto y llegar al checkout por 6,99 EUR. No se introdujeron datos personales ni se completó el pago. Se retiró únicamente la unidad añadida durante el ensayo, dejando el carrito vacío como al principio. Esto comprueba navegación y precio, no el cobro ni la entrega posterior. El paquete público 0.6.27 está cargado en el diálogo de Digital Products; sustituir la descarga 0.6.17 y guardar sigue pendiente de confirmación inmediata por la regla de eliminación de archivos mediante interfaz.

## Estado físico y revisión local pendiente

El usuario indicó que conectaría su iPhone. La comprobación nativa de solo lectura detectó un teléfono; Windows confirmó componentes Apple disponibles y servicio activo. La app 0.6.27 mostró el aviso de componentes listos en la parte superior del inicio. No se abrió la búsqueda de tarjetas ni se capturó, aplicó o recuperó una imagen.

El perfil real contiene un marcador antiguo de importación vacío, anterior al cambio 0.6.27. La preparación conserva el bloqueo y no permite entrar. La app se cerró antes de revisar los archivos. No se retiró el marcador ni se alteraron originales, cuarentenas o diarios. Una migración local correcta no demuestra que una importación anterior esté completa: puede existir un prefijo válido del historial y faltar revisiones posteriores. Se requiere comparar exhaustivamente la evidencia entrante retenida con los destinos, mantener las incidencias y no desbloquear escrituras con identidad desconocida.

Se añadió en esta rama un diagnóstico exclusivamente de prueba, ignorado y con opt-in explícito: diagnose_all_retained_stages_read_only. Enumera ambos árboles retenidos y compara evidencia ligada por digest con los destinos. Dos regresiones sintéticas pasan y comprueban que no cambia ningún archivo. No modifica el comportamiento productivo, no está incluido en el ejecutable publicado y nunca autoriza recuperación ni retirada de barreras.

El diagnóstico real se ejecutó con la app cerrada y bajo un bloqueo exclusivo temporal del sistema. Terminó correctamente: 19 directorios de preparación y 15 validados; 27 archivos de destino coincidentes; dos stages sin archivo ZIP que pruebe su procedencia. Otros stages o validaciones quedaron indeterminados por estructura incompleta/no admitida o límites de lectura; estas categorías no demuestran corrupción. Resultado global indeterminate=true. Por tanto, no existe prueba suficiente para retirar la barrera. El marcador antiguo conservó bytes y fecha; se liberó únicamente el bloqueo temporal del diagnóstico. No se tocaron tarjetas.

Los primeros intentos del lanzador PowerShell no esperaron a un ejecutable del subsistema gráfico. Se corrigió el lanzador privado para esperar al proceso; el diagnóstico nativo terminó con exit 0. No se atribuye ese fallo del lanzador a la aplicación.

Pendiente: completar la sustitución de la entrega de Shopify; revisar el bloqueo antiguo; identificar tarjeta y dispositivo antes de la prueba física. Windows limpio sin iTunes queda aplazado por instrucción del usuario. El pago real y la recepción externa del correo no se han certificado.

Esta auditoría demuestra los escenarios ejecutados. No certifica cualquier iPhone, iOS, banco o tarjeta. Una recuperación parcial incierta exige revisión; conservar evidencia y bloquear es el resultado seguro esperado.
