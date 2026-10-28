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

Pendiente: compilación y paquete 0.6.27, comprobaciones Windows/MSVC, publicación firmada, despliegue del cambio del servidor y actualización de la entrega de Shopify. No considerar la corrección del correo desplegada ni la entrega de Shopify actualizada hasta comprobarlo en cada servicio. Revisar la tarjeta y el dispositivo antes de cualquier prueba física.

Esta auditoría demuestra los escenarios ejecutados. No certifica cualquier iPhone, iOS, banco o tarjeta. Una recuperación parcial incierta exige revisión; conservar evidencia y bloquear es el resultado seguro esperado.
