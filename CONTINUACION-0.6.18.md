# Continuación Wallet Studio 0.6.18 — 23/10/2026

## Incidencia y diagnóstico

El iPhone ya se detectó y la tarjeta apareció. La incidencia comunicada después corresponde a la descarga de un original de la nube, no a la detección USB.

Comprobación autorizada de solo lectura contra el servicio existente: el inventario respondió y dos descargas superaron su comprobación de tamaño y SHA-256. Una descarga de aproximadamente 31 MiB tardó 71,7 segundos, por encima del plazo total de 60 segundos del cliente anterior. El diagnóstico no importó, subió, borró ni modificó originales, recibos o datos del teléfono. Las credenciales se usaron solo en memoria para el mismo servicio autenticado. Los registros de diagnóstico permanecen ignorados y no contienen identidades.

## Cambio

- Descargas de originales: presupuesto total de 300 segundos, recepción sin actividad limitada a 60 segundos. Cuenta, catálogo y actualización conservan el presupuesto anterior.
- Se mantienen HTTPS, rechazo de redirecciones, límite de 100 MiB, tamaño/SHA-256, importación inmutable y recibos solo tras verificar los originales locales.
- El tiempo agotado conserva sesión y copias e indica reintento explícito.
- Corrección independiente del parser de tarjetas: acepta alfabetos Base64 canónicos y padding, examina candidatos posteriores y conserva la barrera exacta de rutas y el contexto Wallet. No se atribuye a este cambio el fallo de nube.

## Alcance y continuación

No se ha ejecutado una nueva captura, transferencia ni restauración física. La recuperación de otro dispositivo se conserva; no debe borrarse para desbloquear el teléfono seleccionado. No marcar la cuenta sincronizada hasta verificar todas las copias necesarias.

El control automático de ventanas no se completó: una llamada de consulta de documentación fue rechazada por la revisión automática de aprobación. La ventana existente no se reemplazó por esa vía.

La publicación de código debe ir en `codex/cloud-download-timeout-2026-10-23`, sin sobrescribir main ni incluir copias, identidades, tokens, imágenes privadas o registros personales. Resultados de pruebas y hash del paquete incluidos abajo.

## Validación y paquete

- Rust Windows GNU release: 190 correctas, 0 fallos, 5 ignoradas, 4 pruebas de dispositivo/purgado excluidas según AGENTS.md.
- Integración sintética WinHTTP/DPAPI/servidor: 1 correcta.
- Parser: 13 correctas, incluidas cinco nuevas regresiones.
- Compilación y empaquetado 0.6.18 correctos. Ejecutable SHA-256: `f4686c9a972bb2e6204d021a8382d63ca6fb3ab1b52eedc5916fdbdd22e7537e`. ZIP SHA-256: `129e2cba2494d2d3172443f2aabbfcee1d4edb8218dc17dd4579e979cb25648c`.
- Diagnóstico nativo autorizado de solo lectura: descarga mayor de 32.465.247 bytes completa y SHA-256 correcto en 59,307 segundos, más 0,966 segundos de inventario. Ese intento quedó justo por debajo del plazo anterior; la medición independiente de 71,7 segundos y la prueba sintética de vencimiento acreditan la variación y la necesidad de un presupuesto mayor. No se ejecutó la sincronización completa ni una captura física.

## Incidencia posterior: revisión de importación

El usuario comunica después «Una copia descargada requiere revisión. No se sobrescribió el original local» y una tarjeta sin original verificado. No equivale al error de transporte ya corregido: esta vez se alcanzó la etapa de importación. La captura automática conserva su bloqueo mientras no se verifiquen las copias.

Se solicitó confirmar si las capturas pertenecen a este PC o al otro ordenador y si ejecuta 0.6.18. Hasta obtener respuesta, no atribuir el estado del perfil local al ordenador de la captura.

Diagnóstico Python local de solo lectura, con salida categórica: un paquete retenido no presenta diferencias de bytes con los archivos locales existentes. Su carpeta de preparación contiene los mismos bytes del archivo. Para esa identidad existe una operación local en fase `prepared`, ausente del paquete remoto. Esa evidencia no puede declararse restaurada a partir de la nube. No se ha importado, borrado ni reconciliado ninguna copia ni marcador. La recuperación física requiere el dispositivo exacto y su validación, no eliminar el bloqueo.

El diagnóstico nativo optativo de importación se añade únicamente a las pruebas, sin cambiar las reglas de producción. Prueba nativa de lectura: 1/1 correcta. Archivo, baseline preparado e historial remoto válidos; historial local incierto. Sin conflictos de bytes ni marcadores detectados. El resultado corresponde únicamente al perfil de este PC. La primera captura también muestra «Sin dispositivo»; confirmar conexión real en el ordenador afectado.
