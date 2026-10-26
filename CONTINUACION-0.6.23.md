# Continuación — Wallet Studio 0.6.23 — 26 de octubre de 2026

## Estado vigente

El usuario autorizó probar el ahorro de tráfico. Mantiene la condición de parar ante un error de compilación o pruebas. Ninguna ejecución de esta entrega falló; no se hicieron reparaciones ni reintentos tras una prueba fallida. Se conserva el trabajo local anterior de 0.6.22; último checkpoint remoto al comenzar: c8cf5b8, rama codex/recovery-onboarding-2026-10-24 de drp48-ua/wallet-studio-build.

## Cambio nuevo

Al usar un diseño con caché íntegra, design_library.rs consulta el catálogo autenticado existente, con límite de 64 KiB. Solo devuelve el PNG local tras comprobar permiso vigente, ID, nombre de archivo, SHA-256, bytes y dimensiones exactos, y volver a leer y verificar el PNG después de la espera. No transforma ni reduce la imagen. La caché de aplicación no se acepta sin permiso en línea. La vista previa conserva su comportamiento anterior.

Una caché ausente o dañada mantiene el GET autenticado de imagen, verificación completa y reparación atómica. Permiso retirado, sesión inválida, catálogo distinto o inaccesible y bytes alterados bloquean el uso sin borrar la evidencia existente. El cambio no requiere modificar ni desplegar el servidor actual.

## Verificación

Revisión del responsable sobre el diff completo aceptada. Único implementador: el rol crack_builder ya configurado, sin cambiar de modelo. No leyó perfiles privados ni servicios reales. Pruebas del módulo: 19 correctas, cero fallidas, dos ignoradas. Suite final nativa en serie: 234 correctas, cero fallidas, siete ignoradas y cuatro exclusiones de hardware; 85,79 segundos. Registros privados ignorados: paid-cache-focused-20261026.log y paid-cache-full-20261026.log, dentro de server/test-runtime.

La prueba de caché reutilizada comprueba una petición de metadatos y cero peticiones de imagen, con bytes y dimensiones originales intactos. Otras pruebas cubren caché ausente o corrupta, reparación y descarga inválida, trece casos de revocación/metadatos/autenticación/desconexión, entradas inválidas y modificación concurrente del PNG. La compilación de distribución 0.6.23 terminó correctamente en 6 min 36 s mediante server/test-runtime/build-release-0.6.23.ps1. Ejecutable SHA-256: 9a604543fa22f8073a63f9bf14e1898bf8978b99fd62008b96089b00f85df0cb. ZIP SHA-256: e0f2b5e510174fadcacd0d1dc5cc25ccbfa51e30903f2bf363b4b5c3045e67cc. Auditoría positiva del paquete: 684 entradas, cero entradas fuera de la lista de distribución autorizada. Copia de entrega: C:/Users/danir/Documents/Codex/Wallet-Studio-0.6.23-Windows.zip. No se abrió la app contra el servicio restringido ni se repitió la restauración física.

## Trabajo anterior incluido

La fuente conserva recuperación física verificada con diario durable, procedencia del emisor, aislamiento por tarjeta, partes de hasta 32 MiB con historial íntegro, conciliación tras llegadas en distinto orden y clasificación estricta de aliases-<digest>.json como nombres conservados sin autoridad sobre originales. Los archivos desconocidos mantienen el bloqueo.

La devolución física autorizada del original concreto del iPhone habitual terminó y fue verificada: diario completo, evidencia íntegra, cinco operaciones ligadas restauradas y cero exportaciones pendientes. No repetirla por rutina. En la primera prueba real de 0.6.22 se resolvió HTTP 413 con dos subidas; permanecieron dos cuarentenas por el artefacto de nombres, cuyo arreglo posterior superó 230 pruebas pero no pudo comprobarse en el perfil real.

## Límite externo y próximos pasos

Supabase notificó 25,42 GB de Cached Egress frente a 5,5 GB disponibles y restricciones hasta el 14 de noviembre de 2026. No confundir ese contador con Egress de la pantalla general. La última comprobación real terminó con conexión y sincronización pendientes; se cerró la app según la orden del usuario. Esta entrega no reintenta esa conexión ni pretende levantar el bloqueo de cuota. El ahorro afecta al tráfico futuro; sin registros del proveedor no se atribuyen los 25 GB completos a la app.

Cuando el servicio esté disponible y se autorice continuar: comprobar la cuenta con el ejecutable nuevo, resolver positivamente ambas cuarentenas, verificar recibos de todas las partes y auditar conservación de originales, historiales y el artefacto de nombres. Los diagnósticos privados y snapshots de integridad están ignorados en server/test-runtime; no subirlos ni imprimir identidades. Nunca sobrescribir un original antiguo inválido ni fabricar una fase para desbloquear.

## Distribución y privacidad

No entregar el ZIP antiguo Wallet-Studio-0.6.22-Windows.zip de Documents/Codex: corresponde al primer candidato anterior al arreglo de alias. La entrega nueva es Wallet-Studio-0.6.23-Windows.zip, compilada, auditada y copiada en Documents/Codex. La rama remota coincidía con la base local antes de guardar; se publica el checkpoint de fuente y documentación segura en codex/recovery-onboarding-2026-10-24. No incluye diseños privados, sesiones ni copias bancarias. No modificar main ni hacer force-push; publicar solo fuente y documentación segura en una rama separada, conservando cambios de colaboradores. No se consumió crédito de reinicio.

## Ahorro adicional del servidor y evaluación de proveedores

Revisión posterior: server/TRAFFIC-AND-FREE-STORAGE.md contiene los resultados y las condiciones oficiales consultadas. Se preparó una caché RAM LRU de diseños de 64 MiB y 128 entradas, sin modificar PNG ni tocar copias bancarias. La segunda petición caliente del mismo diseño hace cero llamadas a Supabase; sesión, licencia y catálogo siguen comprobándose en cada ruta. Suite Python completa: 83 pruebas en total, cero fallos, tres omitidas; foco: seis correctas. Logs privados ignorados: design-cache-full-20261026.log y design-cache-focused-20261026.log. Revisión del responsable aceptada; un único worker crack_builder configurado, sin redes reales ni hardware.

El parche de servidor no está desplegado: Render tiene autoDeployTrigger desactivado y el bloqueo de Supabase sigue siendo un límite externo pendiente de verificar. No se han creado cuentas ni migrado datos. R2 Standard es la alternativa recomendada para objetos por sus condiciones de egress; Firebase Storage requiere Blaze con facturación, aunque ofrece tramos elegibles sin coste. La migración de archivos no sustituye la base de datos ni garantiza que el inicio de sesión funcione mientras esa dependencia siga restringida. No repetir pruebas reales conocidas como bloqueadas ni anunciar la cuenta sincronizada. La aplicación de escritorio sigue en 0.6.23; no se repitieron pruebas nativas porque no cambió código Rust ni la UI.
