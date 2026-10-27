Estado vigente de la entrega nueva: véase [CONTINUACION-0.6.23.md](CONTINUACION-0.6.23.md).

# Copias y recuperación — continuación del 25 de octubre de 2026

## Estado comprobado

La recuperación física autorizada del original concreto del iPhone habitual terminó correctamente con 0.6.20. Primero pasó la revisión autenticada completa; después el CLI devolvió `original_verified_and_restored`, código cero. La comprobación posterior confirmó cero exportaciones pendientes, cero intentos preparados, diario completo, evidencia íntegra y las cinco operaciones vinculadas restauradas. No repetir esta devolución por rutina.

Esto no equivale a sincronizar toda la cuenta ni a validar todos los iPhones. Al abrir Cuenta apareció un conflicto de otra tarjeta del mismo dispositivo. La tarjeta recuperada sigue conciliada.

## Causa adicional localizada

Una copia remota antigua de esa otra tarjeta contiene imágenes modificadas entre los archivos declarados originales. Sus checksums internos son coherentes, pero los bytes no coinciden con los recursos del emisor. El manifiesto y los recursos del emisor sí coinciden con un original local válido. El staging mezclado añade dos operaciones aplicadas y tampoco constituye un historial válido.

No borrar ese archivo antiguo, sobrescribir originales ni escoger entre originales legítimos distintos. El trabajo de 0.6.21 refuerza la procedencia de cada archivo presente y prepara una clasificación conservadora del archivo antiguo, preservándolo como evidencia. Errores de lectura, identidad distinta o emisor distinto deben seguir bloqueando. La 0.6.20 abierta todavía muestra sincronización pendiente; no anunciar resuelto ese conflicto hasta validar la nueva fuente y el perfil real.

## Fuente y validación

Repositorio `drp48-ua/wallet-studio-build`, rama separada `codex/recovery-onboarding-2026-10-24`. Último checkpoint publicado: `78d9350`. No sobrescribir main ni cambios de colaboradores. Los cambios posteriores de 0.6.21 aún requieren revisión, pruebas, compilación y publicación.

La fuente final de 0.6.20 superó 218 pruebas nativas, cero fallidas, siete ignoradas y cuatro exclusiones exigidas. Ejecutable SHA-256: `f09b3542d7f442ebabef982b1504c7a27c3921f45c841b4a9b3ba8c77fd6227b`. ZIP: `C:/Users/danir/Documents/Codex/Wallet-Studio-0.6.20-Windows.zip`, SHA-256 `1f06bb2406cb5a272475f6546e9dd095a4e5278b3418d5662a31b300cec8ea0b`.

En 0.6.21 se añadieron reintentos GET para HTTP 502/503/504 dentro de un único plazo. No se repiten mutaciones, activaciones ni transferencias físicas. El bloque inicial superó 222 pruebas en serie; una fixture WinHTTP falló en paralelo por presupuesto de prueba. Se está corrigiendo la fixture y añadiendo regresiones de procedencia antes de repetir la suite final. No atribuir esos resultados anteriores a la fuente posterior.

## Próximos pasos

1. Revisar el trabajo del único implementador configurado en bank_backup.rs, cloud_sync.rs y adaptadores mínimos. El responsable mantiene versiones, documentación, perfil privado, UI y hardware.
2. Congelar fuente, ejecutar las comprobaciones requeridas y compilar 0.6.21 con el script privado ignorado server/test-runtime/build-release-0.6.21.ps1 mediante el PowerShell incluido.
3. Cerrar 0.6.20 cuando esté inactiva, abrir la nueva app y comprobar Cuenta y sincronización real. No repetir la recuperación física completada ni escribir sobre otra tarjeta sin autorización propia.
4. Revisar UI grande y pequeña, auditar ZIP, guardar hashes, actualizar notas y subir solo fuente y documentación segura a la rama separada.

## Privacidad y cuota

Diagnósticos, ZIP remotos, imágenes, identificadores y tokens permanecen fuera de Git. No se consumió ningún crédito de reinicio. Consultar límites en checkpoints; reservar margen para guardar y publicar. Detener investigación, implementación y pruebas al quedar un 1 % o menos, según las instrucciones del usuario.

## Validación y pendientes al cerrar esta ejecución

La 0.6.21 compiló correctamente en producción (3 min 10 s). Ejecutable SHA-256 `6fd6f7128505fb5360cf3783c6b6e0daf53dc3c53be740aa1ccbcfc8e9cf884c`; ZIP `1f97f2ccbbdb0cc4d8ac418dd9843d69420312dfa37219dc7d1694115e304cae`, copiado a `C:/Users/danir/Documents/Codex/Wallet-Studio-0.6.21-Windows.zip`. Auditoría del paquete: 684 entradas, ninguna entrada privada detectada. Fuente final: 224 pruebas correctas, cero fallidas, siete ignoradas y cuatro exclusiones, ejecutadas en serie; registro privado `provenance-final-serial-regressions-20261025.log`. El ensayo anterior en paralelo falló en WinHTTP y tres fixtures; las fixtures se corrigieron y WinHTTP continúa como limitación de paralelismo documentada. No afirmar que la suite paralela final pasó.

La app real 0.6.21 está abierta y la sesión guardada funciona. Cuenta terminó el intento con HTTP 413, sincronización pendiente. El servidor remoto fija `MAX_REMOTE_BUNDLE=32*1024*1024`; un payload local de historial completo ocupa unos 39,32 MiB antes del pequeño overhead ZIP (exportación Stored). No subir el límite a ciegas: revisar memoria/concurrencia, límite del bucket, cuotas y conservación inmutable; o diseñar particionado verificable. No borrar historial para reducir tamaño.

La prueba real preservó dos archivos en rejected-originals, pero no produjo ninguna disposición positiva; quedan dos ZIP en cuarentena. Por tanto NO afirmar resuelto el conflicto antiguo del perfil. Revisar de forma privada qué validación impide la resolución exhaustiva y si hay una copia principal antigua sin procedencia válida. Esta versión bloquea esas primarias y no implementa variantes/puntero activo ni las sobrescribe. Conservar todos los bytes y los originales fiables existentes.

La recuperación física anterior sigue comprobada: diario íntegro/completo, cinco operaciones restauradas, revisión de esa tarjeta liberada, cero exportaciones pendientes y cero intentos preparados. No repetirla para intentar arreglar HTTP 413.

Revisión visual: app real a 1182×685 y demo aislada a 1182×791, con galería y controles legibles. El parámetro demo 800×600 no produjo ese tamaño efectivo; la revisión de ventana realmente pequeña sigue pendiente. Demo cerrada; app real queda abierta en Cuenta. No se hizo una nueva transferencia al teléfono.

Implementación dirigida con el único worker crack_builder configurado; sin cambios de servidor ni despliegue remoto en esta fase. Pendientes principales: límite HTTP 413, conflicto real restante, migración preservadora de originales antiguos inválidos y prueba en otro PC/dispositivo. No anunciar que todos los dispositivos están resueltos. Reservar cuota para checkpoint y no consumir crédito de reinicio.

## Reanudación del 25 de octubre — 0.6.22 en preparación

La ventana de cuota se renovó de forma ordinaria; no se consumió el crédito de reinicio. La rama remota se volvió a comprobar y coincidía con c8cf5b8 antes de editar.

La causa de la falta de disposiciones positivas quedó localizada en la fuente: canonical_for_rejection pasaba toda la carpeta global de historiales a active_cloud_intent_at, cuyo contrato exige una unión aislada de una única identidad. Con varias tarjetas válidas la comprobación devolvía fallo por identidad mezclada. El arreglo debe aislar y validar todas las revisiones de la identidad objetivo, manteniendo bloqueados sus historiales inválidos.

Para HTTP 413 se aprobó generar partes schema1 independientes de hasta 32 MiB. Cada parte conserva el baseline íntegro y operaciones enteras; no se truncan revisiones ni se borran snapshots. Se mantienen recibos exactos, importación idempotente y unión autenticada para recuperación. Un baseline o una operación con su baseline que no quepa de forma indivisible debe producir un error claro y conservar todo. No aumentar límites del servidor o del bucket sin revisar memoria y cuotas.

Los parámetros 800x600 de la demo anterior eran rechazados por el filtro de tamaños de main.rs y se usaba el tamaño por defecto. Para revisar el mínimo real usar 960x640, aceptado por la fuente. No es necesario cambiar la UI para corregir ese diagnóstico de prueba.

Esta sección es un contrato de trabajo; las comprobaciones y resultados de 0.6.22 todavía están pendientes.

### Fuente 0.6.22 aceptada

Validación final nativa en serie: 229 correctas, cero fallidas, siete ignoradas y cuatro exclusiones de hardware. Compilación de pruebas 2 min 15 s; ejecución 40,83 s. Registro privado ignorado: server/test-runtime/partitioned-cloud-accepted-source-regressions-20261025.log. Incluye 39 MiB de alta entropía en dos partes, importación inversa y otro perfil/PC, unión para prueba de recuperación, interrupción del segundo PUT y reanudación sin repetir la parte confirmada, llegada adversa normal y legado secuencia uno, historial mixto y alteración del objetivo, y conservación de cuarentena compatible hasta conciliación exhaustiva.

Solo los conflictos semánticos exactos de la unión se difieren; los snapshots entrantes se validan aislados primero. No se reclasifican errores de parseo o lectura como conflictos temporales. La numeración antigua solo se difiere bajo las mismas fechas positivas y estrictamente crecientes exigidas antes; nunca autoriza una escritura física por sí misma.

Fuente congelada después de corregir una regresión descubierta en pruebas: la conciliación ya no retira un ZIP compatible mientras otro de esa identidad siga pendiente. El empaquetado de producción está en curso. La prueba real de sincronización con 0.6.22 todavía no se ha realizado.

### Primera comprobación real de 0.6.22 y causa adicional

El primer candidato 0.6.22 resolvió HTTP 413: la app subió dos partes y verificó once copias. No se perdió ninguna de las 195 piezas inmutables registradas antes de la prueba. El diario físico anterior sigue completo y verificado. Ese candidato todavía mantuvo dos copias en revisión; por tanto no se consideró terminado.

Se localizó la causa adicional sin modificar datos: quarantine también contenía un archivo aliases-<digest>.json creado por merge_cards al conservar nombres antiguos inválidos. El contenido es un array de registros hash/name, no un archivo de copia bancaria. Los resolutores trataban todo no-ZIP como evidencia desconocida y bloqueaban ambas tarjetas.

La fuente final añade un clasificador estricto y no autoritativo para ese artefacto: nombre exacto con digest real, archivo regular seguro de hasta 1 MiB, array acotado, solo campos hash/name de texto acotado. No se interpreta el hash como identidad ni se certifica una copia. El artefacto se conserva byte por byte; otros JSON, archivos desconocidos o digest/formato incorrectos mantienen el bloqueo. Hay regresiones positivas y negativas. La nueva suite y reconstrucción están en curso; los hashes del primer candidato 0.6.22 quedan obsoletos y no deben publicarse como finales.

## Parada solicitada ante error — 26 de octubre de 2026

El usuario pidió continuar y parar si aparecían errores. Se comprobó que la fuente final con el clasificador de alias superó 230 pruebas, cero fallidas, siete ignoradas y cuatro exclusiones (45,60 s; registro alias-artifact-final-regressions-20261025.log). La compilación final terminó correctamente en 4 min 21 s. Ejecutable final SHA-256: 655e7c7c8c038565a43ea5487640eea397f13c5f146378a53b2eaeda24e71bc1. ZIP interno dist SHA-256: 385f1f1df9e6c453e04a678ce8025e6b3e757c0a11dfa48303170de67c256497. Este ZIP aún contiene el recuento anterior de 229 en sus notas; la fuente de las notas se actualizó a 230, pero no se volvió a empaquetar.

Se abrió el ejecutable final. La preparación local terminó y se cargó la sesión guardada, pero la comprobación en línea acabó con conexión pendiente y sincronización pendiente. No se confirmó la sincronización ni la resolución real de las dos cuarentenas con este binario. No se diagnosticó ni reintentó la conexión, respetando la orden de parar. Se cerró la app de prueba para evitar sus reintentos automáticos. No se inició ninguna transferencia al iPhone.

No anunciar la cuenta sincronizada ni todas las recuperaciones resueltas. Quedan pendientes comprobar la conexión cuando el usuario autorice continuar, probar la resolución real y la integridad preservada, revisar la ventana mínima y publicar el checkpoint seguro en la rama separada. Los cambios de esta entrega siguen locales y sin commit; último commit publicado c8cf5b8. No sobrescribir cambios de colaboradores.

IMPORTANTE: el ZIP de C:/Users/danir/Documents/Codex/Wallet-Studio-0.6.22-Windows.zip todavía corresponde al primer candidato anterior al arreglo de alias. No entregarlo como versión final. El ejecutable final correcto está en dist/candidate-0.6.22/Wallet Studio.exe. Antes de distribuir, empaquetar las notas actualizadas, auditar y copiar el ZIP final a la ubicación de entrega. No se consumió crédito de reinicio.

## Prueba de ahorro de tráfico autorizada — 26 de octubre de 2026

El usuario confirmó «prueba» tras identificar las descargas completas redundantes al usar diseños. Se conserva su condición de parar si la compilación o las pruebas terminan con error. La restricción de cuota comunicada por Supabase sigue vigente; esta prueba es local y no reintenta la conexión del perfil ni descarga imágenes del servicio real.

Fuente prevista 0.6.23: si el PNG local está íntegro, se consulta el catálogo autenticado existente (hasta 64 KiB), se exige permiso vigente y coincidencia exacta de ID, nombre de archivo, SHA-256, bytes y dimensiones, y se vuelve a verificar el PNG después de la espera de red. Si falta o está dañado, se conserva el GET autenticado del archivo y la reparación atómica. La caché no concede permiso de aplicación sin verificación en línea. Cambios limitados a design_library.rs, metadatos de versión y documentación; se preservan los arreglos locales de 0.6.22.

La revisión del responsable aceptó la implementación y sus cuatro regresiones nuevas. El trabajador configurado ejecuta primero las pruebas del módulo y, solo si pasan, la suite nativa completa en serie. Todavía no se afirma que hayan terminado ni se ha compilado o distribuido la versión 0.6.23.
