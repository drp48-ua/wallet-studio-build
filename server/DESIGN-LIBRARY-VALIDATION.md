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
- Rust requerido sobre la fuente final:143unit+1engine,0fallos,1ignorada (integración localhost optativa),4pruebas de hardware filtradas segúnAGENTS. Build/paquete: consultar apartado final de artefactos y logs.

Logs detallados privados/ignorados: `server/test-runtime/library-server-final.log`, `library-rust-final.log`, `library-build-final.log`, `library-package.log`. Los fallos iniciales de sandbox en TEMP/sockets se resolvieron ejecutando los mismos checks fuera del sandbox; no se debilitaron tests ni validación. No se afirma verificaciónUSB, Supabase/PostgreSQL real, despliegue o aceptación visual desde estas pruebas.

Riesgos/resume:

- `refunds/create` revoca preventivamente todo el pedido; solo un pedido completo estrictamente posterior puede reconciliar una devolución parcial. A igual fecha prevalece revocar.
- Una copia que el usuario ya haya guardado/exportado como propia no se puede retirar remotamente. Los derechos para nuevas descargas sí se comprueban siempre.
- El lead debe revisar el candidato en ventanas pequeñas/grandes, demo vacía/con imágenes, y ejecutar la pruebaUSB autorizada después de preparar nube/originales. La implementación no se marca aceptada.
- Runbook: `server/DESIGN-LIBRARY-DEPLOYMENT.md`. No hubo commit/push/deploy.

Archivos de implementación modificados: render.yaml; scripts/package-windows.ps1; server/.env.example; server/Dockerfile; server/RENDER.md; server/accounts.py; server/app.py; server/remote.py; server/tests/test_postgres_integration.py; wallet-studio.rc; windows-source/src/app.rs; windows-source/src/cloud_sync.rs; windows-source/src/design_library.rs; windows-source/src/main.rs; windows-source/src/scanner.rs; windows-source/src/update_info.rs; windows/designs/README.md.

Archivos nuevos propios: NOVEDADES-0.6.5.md; server/DESIGN-LIBRARY-DEPLOYMENT.md; server/DESIGN-LIBRARY-VALIDATION.md; server/designs.py; server/tests/test_designs.py; server/upload_designs.py. Nuevos del lead preservados: BIBLIOTECA-50-2026-10-16.md; server/design-catalog.json. El ejecutable y ZIP son artefactos locales ignorados.
