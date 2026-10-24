# Copias y recuperación entre dispositivos — continuación

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
