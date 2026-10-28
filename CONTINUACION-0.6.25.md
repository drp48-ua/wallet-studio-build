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

## Integración autorizada en main

Las dos comprobaciones Windows/MSVC del commit 74e67e3556da21f9f90eec480beb722e436267a6 terminaron correctamente: ejecuciones 40566844072 y 40566934600. PR #8: https://github.com/drp48-ua/wallet-studio-build/pull/8. Se integró mediante merge normal, sin force-push: main fcd5e0849be594709a59e939f5b08073611e5079.

El paquete GNU local contiene 676 archivos, sin datos privados, con EXE idéntico al candidato. ZIP SHA-256: 4eb5ec8defa58f3e01d00a7122a65b4158718dd1fc84e2bb985099205c255ad6. EXE SHA-256: 54835d4448a1ca2fd78358e40e531b897e99e271f16d946a7144f73e8d05db9b. Se revisó la demostración aislada 0.6.25 en tamaño pequeño/maximizado y con tres tarjetas ficticias/estado vacío, sin red ni USB. Los nuevos mensajes de incidencias se validaron mediante pruebas de presentación; no se atribuye a esta revisión visual una captura real.

La publicación firmada build 2 está en curso: https://github.com/drp48-ua/wallet-studio-build/actions/runs/40569332467. Falta verificar resultado, firma, ZIP y EXE reales. Verificador privado ignorado preparado: server/test-runtime/verify-published-0625.py. El perfil real conserva su actualización activa 0.6.24 build 1. El responsable no ha actualizado ese perfil ni iniciado la captura física del segundo iPhone: el usuario quiere probar personalmente el salto desde 0.6.24.

## Publicación final verificada

La ejecución 40569332467 terminó correctamente. Release estable disponible: https://github.com/drp48-ua/wallet-studio-build/releases/tag/desktop-build-2. Versión 0.6.25, build 2, main fcd5e0849be594709a59e939f5b08073611e5079. Se descargaron los archivos públicos reales y se verificaron firma RSA-PSS/SHA-256 con la clave fijada del cliente, tamaño y SHA-256 del ZIP, SHA-256 del ejecutable y las 676 entradas de archivos sin rutas privadas/prohibidas.

ZIP MSVC publicado SHA-256: e0e941e592d66499baf69b669952be710825eccd563b2a22ae7fc02308c5b50a. EXE publicado SHA-256: 5b8a4831c7f59fe5c22e5bd8c2f8890dec5f9cde1cd97ba1ead1042b9258a378. Se conserva copia de entrega pública fuera del perfil real; la prueba solicitada debe partir de 0.6.24, Ayuda, Buscar actualizaciones, esperar el aviso de 0.6.25 preparada, cerrar normalmente y volver a abrir el mismo acceso. No se ha sustituido en nombre del usuario su aplicación activa ni se certifica todavía la captura física del segundo iPhone.

## Incidencia de inicio después de la actualización

El usuario abrió correctamente el ejecutable publicado en updates/build-2 y encontró almacenamiento-revision. Se comprobó un marcador vacío .cloud-import.lock fechado antes de la publicación de 0.6.25; no había storage-blocked.json ni .migration.lock. El diagnóstico nativo explícito de solo lectura pasó: 9 archivos en cuarentena con parseo correcto, 7 baselines de staging válidos, 2 no válidos y una cadena de staging no válida. Estas incidencias anteriores se conservaron; no se reclasificaron como originales verificados.

Se comprobó por apertura exclusiva de lectura que el marcador de importación no estaba en uso. Bajo un bloqueo exclusivo temporal de operaciones se movió únicamente ese marcador vacío a lock-recovery/cloud-import-orphan-20261027-<identificador-aleatorio>.evidence. Se verificó su conservación exacta; no se borraron originales, cuarentenas, diarios ni marcadores de revisión de tarjetas. Al pulsar Volver a comprobar mis copias locales, la 0.6.25 superó el error de almacenamiento, restauró la sesión guardada y mostró las 11 tarjetas locales. En ese punto la comprobación de red de la cuenta seguía pendiente y no había un iPhone detectado.

La causa general del marcador huérfano sigue en el código: import_bundle conserva .cloud-import.lock si el proceso se interrumpe antes de su cierre normal. Esta actuación repara el estado local comprobado; no es una nueva versión ni una recuperación automática general. Una futura corrección debe validar el estado duradero y excluir un propietario activo antes de tratar un marcador huérfano, manteniendo los bloqueos de evidencias corruptas o desconocidas. No retirar estos marcadores a ciegas.

El implementador configurado revisó la prevención sin modificar código ni ejecutar otra compilación. Descartó DeleteOnClose aplicado a toda la importación: una interrupción durante escrituras parciales podría retirar la barrera global y otra instancia ya abierta podría omitir la auditoría de inicio. Plan de continuación: propietario exclusivo y limpieza automática solo durante staging/validación; antes de la primera escritura en destinos, conservar una barrera duradera y evidencia suficiente para reproducción exacta. Probar interrupción temprana, propietario activo, escritura parcial y conservación de bloqueos globales/por tarjeta. La recuperación automática tras escrituras requiere diario y reproducción verificada. No cabe presentarla como resuelta por liberar el marcador de este PC.

Checkpoint de cuota: 94 % usado en ventana principal (6 % restante compartido); no se utilizó el crédito de reinicio. Se conservan main/release 0.6.25 sin cambios de código posteriores. La app superó el arranque con 11 tarjetas y sesión guardada; la comprobación de red seguía pendiente en la última observación. Prioridad al continuar: verificar resultado de sesión, implementar la prevención segura y probarla antes de publicar otra versión. No repetir reparaciones de datos ni devolver originales de teléfonos sin revisar su evidencia y autorización específica.
