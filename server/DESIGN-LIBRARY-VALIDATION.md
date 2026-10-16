# Validación de implementación0.6.5 — revisión pendiente del lead

Modelo final: un producto10459984298313,6,99EUR, app+50diseños. APP_PRODUCT_IDS conserva eseID; BUNDLE_PRODUCT_IDS/PACK_PRODUCT_IDS vacíos. Toda licencia app activa incluye catálogo, incluidas compras anteriores. Los derechos históricos de diseños no conceden app. No se cambió ningún producto, secreto, pedido, despliegue ni teléfono desde esta implementación.

Cambios implementados:

- Biblioteca visual con diseños propios persistentes por servidor/cuenta, importación y guardado verificado; copia no destructiva de `guardado-*.png` anteriores. Se elimina el selector antiguo por nombre. Usar diseño carga el editor/Tarjetas en cuentas app.
- Metadatos autenticados y miniaturas visibles en worker; caché local separada por cuenta y cachéGPU acotada, fallos incluidos. Actualizar permite reintento. Cada uso pagado exige nueva descarga autenticada/verificada; un fallo no autoriza desde caché.
- Bucket privado wallet-designs, catálogo de50hashes/PNG320x200/1MiB máximo, descargas sin URL pública. Aprovisionamiento por entorno sin imprimir credenciales, sin sobrescritura y con readback.
- HMAC, idempotencia, cancelación/devolución, tombstones y reconciliación por pedido. `/v1/auth/session` sigue siendo app-only para clientes antiguos; `/v1/auth/account` distingue derechos actuales/históricos. Equipos/OTP/originales/diarios mantienen sus reglas.
- Búsqueda termina con primera tarjeta exacta; libera relay y sesión de dispositivo antes de notificar. Botón detener habilitado durante búsqueda; busy solo termina tras cierre del worker. Captura automática existente mantiene nube,USB,identidad y recuperación como requisitos.
- Versión0.6.5 y empaquetado que copia únicamente README en designs. Modo demo aislado para revisión visual, sin red/teléfono.

Checks reales completados:

- `python -m unittest discover -s server/tests -v`:46pruebas,OK,1omitida (PostgreSQL real requiere base sintética dedicada). Incluye configuración de producto único, propietarios app anteriores con50diseños, derechos históricos sin app, devoluciones/cancelación, aislamiento, revocación, PNG corruptos, bucket público e upload409/inmutable/idempotente.
- Validación local de los50archivos privados reales contra catálogo, hashes, dimensiones y decodificación PNG acotada:OK. Los binarios permanecen fuera de Git.
- `git diff --check`:OK.
- Suite Rust antes del último ajuste de carga automática:143unit+1engine,0fallos,1ignorada (integración localhost optativa),4pruebas de hardware filtradas segúnAGENTS. El helper final se verificó mediante compilación nativa; no se repitió la suite amplia después de extraerlo, para reservar la cuota solicitada. Build/paquete: consultar el recibo siguiente.

Logs detallados privados/ignorados: `server/test-runtime/library-server-final.log`, `library-rust-final.log`, `library-build-final.log`, `library-package.log`. Los fallos iniciales de sandbox en TEMP/sockets se resolvieron ejecutando los mismos checks fuera del sandbox; no se debilitaron tests ni validación. No se afirma verificaciónUSB, Supabase/PostgreSQL real, despliegue o aceptación visual desde estas pruebas.

Riesgos/resume:

- `refunds/create` revoca preventivamente todo el pedido; solo un pedido completo estrictamente posterior puede reconciliar una devolución parcial. A igual fecha prevalece revocar.
- Una copia que el usuario ya haya guardado/exportado como propia no se puede retirar remotamente. Los derechos para nuevas descargas sí se comprueban siempre.
- El lead debe revisar el candidato en ventanas pequeñas/grandes, demo vacía/con imágenes, y ejecutar la pruebaUSB autorizada después de preparar nube/originales. La implementación no se marca aceptada.
- Runbook: `server/DESIGN-LIBRARY-DEPLOYMENT.md`. No hubo commit/push/deploy.

Archivos de implementación modificados: render.yaml; scripts/package-windows.ps1; server/.env.example; server/Dockerfile; server/RENDER.md; server/accounts.py; server/app.py; server/remote.py; server/tests/test_postgres_integration.py; wallet-studio.rc; windows-source/src/app.rs; windows-source/src/cloud_sync.rs; windows-source/src/design_library.rs; windows-source/src/main.rs; windows-source/src/scanner.rs; windows-source/src/update_info.rs; windows/designs/README.md.

Archivos nuevos propios: NOVEDADES-0.6.5.md; server/DESIGN-LIBRARY-DEPLOYMENT.md; server/DESIGN-LIBRARY-VALIDATION.md; server/designs.py; server/tests/test_designs.py; server/upload_designs.py. Nuevos del lead preservados: BIBLIOTECA-50-2026-10-16.md; server/design-catalog.json. El ejecutable y ZIP son artefactos locales ignorados.


## Recibo del candidato corregido

Fuente revisada: commit `3c860715721c0ba947c03950ebb3a94e2ace89ce`, inicialmente sin cambios locales. Incluye `start_purchased_refresh`, su llamada automática al ver por primera vez el propietario autenticado de Biblioteca, reinicio seguro de receptores anteriores, reintento manual y subtítulo del producto único. No hubo ediciones de código en esta comprobación; solo se actualiza este documento.

El log privado `server/test-runtime/library-build-final.log` termina con compilación release optimizada correcta en **3m15s**; se cerró el16/10/2026 a las14:00:05(+02:00). El EXE se escribió a las14:00:03 y el ZIP a las14:00:28. El binario contiene el subtítulo corregido y el EXE incluido en el ZIP coincide byte por byte con el candidato, acreditando que no es el anterior al ajuste de carga automática.

- Ejecutable: `windows/Wallet Studio.exe`, versión Windows **0.6.5**, **12.554.240bytes**. SHA256: `f2488161a094d92957b7556a0fa0c3a69d5d0cf346b61a26478c2c4570743184`.
- Paquete: `dist/Wallet-Studio-0.6.5-Windows.zip`, **8.277.242bytes**. SHA256: `0f4109cc24ce11727f7508bd550303d24453c645e9197ded313f5f63289b5255`.
- CRC completo del ZIP: **correcto**. EXE embebido: **coincidencia exacta**. Manifiesto de release: **versión/tamaño/hash coincidentes**.
- Exclusión privada: ningún nombre `ws-001.png`..`ws-050.png`, ningún hash de los50PNG privados y ninguna carpeta de runtime bancario/biblioteca privada en el ZIP. En `designs` solo existe `README.md`.
- Estado Authenticode registrado por el empaquetador: **NotSigned**. Es un candidato local de revisión; no se afirma publicación ni instalación firmada.

Recibo JSON detallado, ignorado en Git: `server/test-runtime/library-artifact-receipt.json`. La aceptación visual, el despliegue y la prueba física autorizada corresponden al lead y siguen sin acreditarse por este recibo. No se recompiló ni se repitieron suites amplias durante esta comprobación.


## Reempaquetado documental y diagnóstico de lanzamiento

Se actualizó únicamente `LEEME.md`: título0.6.5, referencia a NOVEDADES-0.6.5.md presente en el ZIP y nota de Biblioteca/diseños incluidos. Se volvió a ejecutar el empaquetador existente. El EXE conserva exactamente el hash anterior; no hubo cambio de código ni recompilación. El hash y tamaño del ZIP del recibo anterior se han actualizado a este reempaquetado, posterior a su fecha original14:00:28. CRC completo, EXE embebido, manifiesto, README actualizado y exclusión de los50nombres/hashes privados se volvieron a verificar correctamente.

El lead confirmó que una sesión de lanzamiento retenida muestra la ventana normal: no se acreditó un crash de la app. Los hijos lanzados con procesos padres breves habían terminado por su ciclo de vida de host/job. Los logs stderr/stdout observados estaban vacíos; no hubo evidencia para atribuir un fallo a Apple DLLs. No se modificó esa carga ni sus comprobaciones de seguridad.

Diagnóstico de almacenamiento en lectura: en el perfil host consultado no existen los marcadores globales `.vault-operation.lock`, `.migration.lock`, `.cloud-import.lock` ni `storage-blocked.json`. No se borró ni se omitió ningún bloqueo. La hipótesis pendiente es fallo de permisos antes de crear el lock si el programa hereda el sandbox de ejecución; debe contrastarse mediante lanzamiento host autorizado, conservando toda evidencia. No se inspeccionó el contenido de originales, diarios, sesiones ni credenciales.
