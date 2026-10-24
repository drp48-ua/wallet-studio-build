# Copias y recuperación entre dispositivos — continuación

## Reanudación actual: versión 0.6.20 en validación

Compilación final de producción correcta (10 min 22 s). Ejecutable SHA-256 `f09b3542d7f442ebabef982b1504c7a27c3921f45c841b4a9b3ba8c77fd6227b`; ZIP `1f06bb2406cb5a272475f6546e9dd095a4e5278b3418d5662a31b300cec8ea0b`, disponible también en `C:/Users/danir/Documents/Codex/Wallet-Studio-0.6.20-Windows.zip`. Fuente y pruebas subidas en `80d56ff` a la rama separada.

La revisión nativa `--wallet-studio-review-cloud-owned-original` terminó con categoría pendiente antes de emitir etapas. Comprobación de solo lectura inmediatamente posterior: servicio Apple disponible, cero iPhones USB. Por tanto no se preparó el plan ni se movieron archivos del teléfono. Se pidió reconectar el mismo iPhone; la autorización de devolución ya consta. Repetir primero revisión con USB disponible y solo después, si la prueba íntegra termina correctamente, ejecutar la devolución autorizada y comprobar conciliación/sincronización por separado. No anunciar recuperación física ni validación visual de la nueva UI como realizadas.

Suite final de la fuente congelada, incluida la corrección de concesión retenida: 218 correctas, cero fallidas, siete ignoradas y cuatro exclusiones exigidas. Registro local ignorado `server/test-runtime/recovery-lease-final-regressions-20261025.log`. La compilación de producción está en curso. La revisión nativa de propiedad y la devolución física todavía no han empezado. No confundir los 218 tests sintéticos con una prueba física universal.

La ventana de cinco horas se renovó de forma ordinaria; no se consumió el crédito de reinicio. Las dos regresiones de cuarentena anteriores terminaron correctamente. El primer bloque nuevo superó 210 pruebas nativas, ninguna fallida, siete ignoradas y cuatro exclusiones de hardware. Ese resultado precede a las últimas ediciones de interfaz, lista de originales y recuperación; no equivale a validar el código final.

Se implementaron aislamiento de conflictos remotos identificados, reutilización íntegra de ZIP retenidos, lista de originales importados, comprobación física completa al reanudar, y cierre veraz de trabajadores perdidos. El flujo nuevo de recuperación usa inventario autenticado completo con límites, unión remota aislada, historial estricto y diario durable. El backfill de operaciones nuevas se publica mediante rename de un directorio privado completo; no se exponen directorios con solo assets. La finalización exige una capacidad que solo el flasher produce después de originales, caché y originales comprobados.

El usuario volvió a confirmar el iPhone habitual preparado y su devolución sigue autorizada. Diagnóstico USB de solo lectura: uno conectado y coincide con la operación pendiente. No se inició todavía una nueva recuperación física con 0.6.20. Faltan la suite final de esta fuente, compilación de distribución, revisión visual, prueba física autorizada y verificación de sincronización. El paquete 0.6.19 ya entregado no contiene este flujo nuevo.

El usuario solicita resolver el flujo completo en los dispositivos compatibles. No afirmar compatibilidad universal ni recuperación terminada sin pruebas. La versión empaquetada 0.6.19 conserva las limitaciones indicadas en sus novedades; el trabajo posterior de este documento no está incluido en ese ejecutable.

## Diagnóstico nuevo

1. `reconcile_card_quarantine_at` puede omitir ZIP ilegibles o inválidos y después quitar la marca de revisión. La falta de carpeta de cuarentena tampoco acredita una conciliación. Conservar el bloqueo si hay evidencia desconocida o no existe una importación positiva del archivo correspondiente. Corrección y pruebas a cargo del trabajador de esta sesión.
2. `synchronize_report_progress` devuelve error en el primer `import_bundle` fallido. Esto impide procesar otros archivos remotos y subir originales locales sanos. El aislamiento por tarjeta del bucle de subidas no llega a ejecutarse. Pendiente: contar importaciones pendientes en `SyncReport`, continuar las demás importaciones y subidas, no escribir recibos de importaciones fallidas, y mantener `complete()==false` si queda cualquier conflicto. No ocultar errores de autenticación, inventario, transporte o almacenamiento mediante un catch general. Probar con servidor simulado y archivos sintéticos.
3. La selección remota autenticada propuesta en `RECUPERACION-NUBE-DISENO.md` todavía no existe. Los validadores de baseline/historial requieren directorios; preparar un adaptador aislado o validación en memoria sin confiar en staging previamente mezclado. La coincidencia con un diseño remoto no autoriza por sí sola retirarlo del teléfono.

## Restricciones y pruebas pendientes

Conservar baseline, diarios, cuarentena, identidad exacta, prueba de propiedad y lectura física posterior. Nunca fabricar fases ni borrar marcadores para desbloquear. La autorización para devolver el original concreto del iPhone habitual ya consta; primero completar el flujo seguro. No tocar el iPhone del amigo sin sus propios datos y prueba autorizada.

Faltan pruebas de interrupción/reanudación en todas las fronteras durables, desconexión USB, reinicio de Windows, recuperación en otro PC de la misma cuenta y aislamiento entre dispositivos. También falta completar la conciliación de historiales local y remoto y repetir la prueba física. Los archivos privados y sus diagnósticos locales no se suben a Git.

## Cuota

Al comenzar esta petición quedaba un 9 % de la ventana de cinco horas. Se inició checkpoint al acercarse al 3 %, según AGENTS.md del usuario. No se consumió crédito de reinicio. Continuar desde este documento y el commit de la rama separada; conservar cambios de colaboradores y no sobrescribir main.

## Cierre de cuota de esta ejecución

Corrección de cuarentena implementada en cloud_sync.rs: conserva la revisión ante archivos desconocidos, inválidos, no regulares, con digest incorrecto, carpeta ausente o ninguna copia de la identidad esperada. Nueva regresión sintética junto a la positiva existente. diff --check correcto. Pruebas GNU iniciadas en server/test-runtime/run-quarantine-review-regressions.ps1 (sesión 27129), resultado aún no revisado al alcanzar 99 % de cuota usada. No presentar estas pruebas como superadas ni distribuir ejecutable nuevo. Detener investigación, implementación y pruebas por AGENTS; sin reinicio de cuota.

## Confirmación del usuario y parada de cuota

El usuario confirmó después de compilar: «Ya está reconectado y preparado». No se repitió la comprobación USB ni la revisión nativa porque la herramienta de límites indicó 99 % consumido en la ventana de cinco horas. Detención obligatoria según AGENTS.md; no se consumió crédito de reinicio. La autorización de devolver ese original continúa vigente. Al renovar la cuota, comprobar primero el USB exacto, ejecutar review-cloud-owned-original de la 0.6.20 y solo si valida íntegramente continuar con restore-cloud-owned-original. Siguen pendientes la prueba física, la conciliación/sincronización real y la revisión visual final.
