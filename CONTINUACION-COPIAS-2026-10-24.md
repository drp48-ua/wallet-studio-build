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
