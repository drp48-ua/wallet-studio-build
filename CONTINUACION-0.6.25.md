# Wallet Studio 0.6.25: bloqueo por incidencias de otro dispositivo

## Reproducción observada

En 0.6.24, otro iPhone se detectó por USB, se identificó una tarjeta nueva y la búsqueda syslog se detuvo correctamente. La captura automática quedó esperando la comprobación de la nube. Al terminar, la cuenta mostró 11 copias verificadas, 9 copias remotas en revisión y 6 locales en revisión (3 por conflicto y 3 por recuperación). La propia UI indicaba que la recuperación pendiente de exportación pertenecía a otro iPhone y no bloqueaba al seleccionado. No se presenta esta prueba como una copia guardada.

La causa es la condición global de captura: CloudMessage.synced depende de SyncReport.complete(), que exige cero incidencias en toda la cuenta. Por tanto, las incidencias de dispositivos ajenos impiden guardar un original nuevo. Durante la inspección solo se consultaron estado, registro y metadatos locales; no se inició una recuperación ni se aplicó un diseño.

## Corrección en preparación

Rama separada: codex/scoped-cloud-capture-20261027. Main comprobado: aca5756ff372bca6a040e9ee9cbc2418c20e4d66. Se conserva la documentación posterior de la 0.6.24; no se reescribe main.

La autorización de captura debe exigir una comprobación autenticada e inventario completo, con incidencias clasificadas por identidad validada. Una incidencia conocida de otro dispositivo no debe bloquear al seleccionado. Cualquier recuperación o incidencia del mismo dispositivo, inventario incompleto, identidad desconocida, corrupción, sesión no verificada o cambio de cuenta debe mantener el bloqueo. No se eliminan cuarentenas ni se marcan incidencias como resueltas para permitir la captura.

Un único implementador crack_builder modifica cloud_sync.rs, app.rs y pruebas relacionadas. El responsable conserva versión, empaquetado, revisión, Git y publicación. La versión prevista es 0.6.25. Pendiente: aceptación del cambio real, pruebas de regresión, compilación, CI y publicación firmada. No hay todavía una prueba física positiva de esta corrección en el segundo iPhone.

## Aceptación del código

El responsable revisó el diff final de los tres módulos. CaptureInventory se crea solo después de completar todo el inventario autenticado y la validación de su propietario. La prueba vive en memoria, se liga a cuenta, servidor y huella de la sesión exacta, y se elimina al iniciar/fallar una sincronización o cambiar la sesión. Las incidencias locales solo se aíslan cuando existe una identidad de baseline validada; las desconocidas o corruptas mantienen el bloqueo global. Cualquier incidencia conocida del mismo iPhone bloquea todas sus nuevas capturas. Los controles de procedencia, recuperación y verificación anteriores a las escrituras del teléfono permanecen intactos.

Las pruebas de nube terminaron con 31 correctas y 3 ignoradas; app/presentación/cola, con 27 correctas. La primera suite completa del entorno restringido obtuvo 239 correctas y dos fallos en pruebas existentes de transporte WinHTTP local. Sin cambiar esas pruebas, el responsable repitió el mismo binario con acceso normal: 241 correctas, cero fallidas, 7 ignoradas y cuatro exclusiones de hardware, en 30,80 s. Registros protegidos e ignorados: server/test-runtime/scoped-capture-0625-*.log.

El usuario autorizó expresamente integrar en main al terminar para probar la actualización desde 0.6.24. Se conservará su instancia 0.6.24 sin actualizarla en su lugar. Pendiente de este checkpoint: paquete de distribución, comprobación Windows/MSVC y publicación firmada; la captura física en el segundo iPhone todavía no se da por comprobada.
