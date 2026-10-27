# Recuperación de un diseño propio verificado en la nube

Estado: implementado en la fuente 0.6.20 y validado con 218 pruebas nativas, ninguna fallida. La devolución física del original autorizado terminó y quedó verificada el 25 de octubre. La sincronización del perfil sigue pendiente por otra tarjeta con un original remoto legado sin procedencia válida. La retirada genérica conserva su control estricto; el flujo explícito nuevo exige prueba autenticada y diario durable.

## Hallazgo y límite de esta revisión

El responsable de la sesión confirmó que el contenido observado difiere del intento local preparado, pero coincide con un diseño aplicado conservado en una copia remota. Esta revisión del código no ha leído perfiles privados, archivos del teléfono ni servicios remotos. La coincidencia comunicada explica por qué el guardado local no basta para autorizar la retirada actual; no sustituye una prueba autenticada ejecutada por la aplicación.

Se mantiene intacta la protección de `wallet_export::remove_verified_override`: baseline exacto con ausencia original, intento local durable no restaurado y bytes esperados ligados a ese intento. Los bytes diferentes se devuelven mediante el diario y la operación falla. No se acepta contenido desconocido ni se cambia una fase para forzar compatibilidad.

## Interfaces existentes que deben conservarse

- `cloud_sync::require_owner_at` vincula servidor y propietario guardados con la sesión. `validate_session` valida la sesión real. El inventario autenticado `/v1/bundles` declara identificador, SHA-256 y tamaño; actualmente el identificador debe ser el SHA-256. Una recepción histórica `cloud-seen` o un ZIP local sin consulta vigente no acredita propiedad actual de cuenta.
- `cloud_sync::parse_bundle` limita tamaño y cantidad, rechaza nombres peligrosos, duplicados y entradas no declaradas, y verifica cada checksum. La validación del baseline y del historial es adicional; parsear el ZIP no la reemplaza.
- `bank_backup::validate_bundle_directory` verifica el baseline. `Baseline` admite igualdad exacta de todos sus campos, incluido manifiesto, recursos del emisor y las tres ausencias/presencias originales.
- `bank_changes::validate_cloud_chain` verifica identidad e historial usando validadores de producción. `cloud_operation_dirs` rechaza fases inciertas. El cargador privado `load_latest_at` impone una única operación sin restaurar, pero todavía no expone una selección tipada de operación remota activa para este flujo.
- `flasher::restore_wallet_original` mantiene una concesión exclusiva de almacenamiento; la restauración bancaria conserva las lecturas protegidas, retirada de overrides originalmente ausentes, checkpoint de originales verificados y comprobación de caché antes de marcar restaurado.
- `wallet_export::remove_verified_override` conserva diario antes de mover, copia exportada, comparación exacta, devolución ante discrepancia y comprobación protegida de ausencia posterior. AFC denegado no significa ausencia.
- `recovery_cli::candidate` exige una sola coincidencia USB, un único intento interrumpido, identidad literal, baseline exacto, versión compatible y ninguna exportación pendiente para ese dispositivo.

## Contrato propuesto

1. Ofrecer exclusivamente una acción explícita de devolver el original. No añadir esta autorización al cambio de diseño, captura automática, sincronización general ni retirada genérica.
2. Bajo la misma concesión exclusiva de operación, fijar la identidad exacta elegida, propietario local y baseline inmutable. Mantener los bloqueos globales, diarios pendientes, identidad y versión. Un bloqueo de revisión por conflicto no se borra como preparación.
3. Validar sesión y propietario, consultar el inventario real autenticado y exigir HTTPS y endpoint autorizado según la política vigente. Rechazar entradas duplicadas, no canónicas, tamaños excesivos, respuesta incompleta o fallo de autenticación. No registrar credenciales, identidades ni rutas privadas.
4. Usar solamente un ZIP retenido cuyo tamaño y SHA-256 coincidan exactamente con una entrada del inventario vigente. Si hace falta descargarlo, aplicar los mismos límites de descarga. Una coincidencia de nombre de archivo no basta.
5. Validar ZIP, baseline e historial remoto aislados del historial local preparado. El staging existente puede contener archivos locales añadidos por un intento de importación: no se debe considerar remoto todo lo presente en esa carpeta. Verificar que cada archivo usado está declarado en el ZIP autenticado y coincide con él. Si se necesita materializar, usar evidencia privada create-only; nunca un directorio público, Git o un reemplazo del baseline local.
6. Exigir identidad exacta de dispositivo y tarjeta, igualdad completa del baseline remoto y local, historial remoto válido y una sola operación remota activa en fase aplicada. Varias instantáneas de la misma operación no cuentan como operaciones diferentes; historias contradictorias o distintas operaciones activas bloquean. No seleccionar el primer ZIP, el último de la lista ni la fecha más reciente. Fechas de dos equipos no prueban orden causal.
7. Crear una capacidad privada sin constructor público, por ejemplo `VerifiedCloudRecoveryOwnership`, producida solamente por el verificador anterior. Debe vincular propietario, identidad exacta, digest del ZIP, baseline, operación remota y bytes autorizados por cada ruta. No aceptar booleanos, mapas arbitrarios, una ruta aportada por CLI ni un `Intent` fabricado por el llamador.
8. Antes del primer movimiento, persistir y leer de vuelta el ZIP exacto y la evidencia necesaria en almacenamiento privado durable. El registro de autorización no debe contener el token de sesión. Una referencia a un archivo mutable o evidencia solo en RAM no es suficiente para recuperar tras cierre o pérdida de corriente.
9. Introducir una vía separada y explícita en el flasher y removedor, conservando la función genérica estricta. Al entrar, volver a validar evidencia, baseline y objetivo bajo la concesión. Mantener la comparación exacta de los bytes realmente exportados con los bytes acreditados. Cualquier otro contenido debe devolverse con el diario normal y detener la operación.
10. Conservar todos los pasos físicos existentes: diario previo al movimiento, copia recuperable, devolución verificable ante discrepancia, metadatos protegidos del padre y manifiesto, ausencia posterior inequívoca de las rutas originalmente ausentes, devolución y readback de las presentes, y caché comprobada. No marcar originales verificados ni restaurado antes de completar sus respectivas pruebas.
11. Conservar el intento local preparado y el historial remoto originales. Definir y revisar un registro durable de recuperación que vincule ambos con las pruebas físicas, sin fabricar estados anteriores ni borrar evidencia. La conciliación posterior debe reconocer esa recuperación física explícita; importar la cadena remota sin resolver esta relación seguiría creando dos operaciones abiertas. Esta parte necesita diseño y aceptación antes de habilitar la vía.
12. Fallo, desconexión, timeout, firma de evidencia inválida o discrepancia mantienen recuperación pendiente y permiten revisión explícita; nunca reintento automático ni mensaje de éxito por tener una copia válida en el PC.

## Pruebas necesarias antes de habilitarlo

Fixtures completamente sintéticas y backend local simulado:

- Aceptación de una única operación remota aplicada, propietario e identidad exactos, baseline idéntico y bytes observados idénticos; todo el flujo físico simulado debe completar antes de éxito.
- Rechazo de sesión caducada, otro propietario/servidor, inventario antiguo sin consulta, ZIP ausente del inventario, SHA/tamaño distintos y entradas duplicadas.
- Rechazo de dispositivo o tarjeta diferentes, hash no canónico y cualquier campo diferente del baseline.
- Rechazo de ZIP peligroso, estado corrupto, revisión alterada, múltiples operaciones activas, fases preparadas/originales verificados y cadenas contradictorias. No confiar en fecha o posición del inventario.
- Staging contaminado con un intento local preparado ausente del ZIP: ese archivo no puede ampliar la propiedad remota.
- Evidencia durable ausente, modificada, con enlace simbólico o fallo de lectura de vuelta: cero movimientos del dispositivo.
- Prueba de que un llamador normal no puede construir la capacidad; la función genérica sigue rechazando bytes distintos al intento local.
- Bytes exportados diferentes de los acreditados: devolución exacta y fallo; nunca desaparición aceptada. Devolución incierta conserva diario y bloquea nuevos movimientos.
- Metadatos denegados/presentes después de retirar, error de Books, fallo de readback o caché: evidencia retenida y ninguna fase de restauración final.
- Interrupción simulada en cada frontera de persistencia/movimiento/retorno; recuperación posterior usa únicamente evidencia durable validada.
- La conciliación conserva ambos historiales y no inventa aplicación o restauración anterior. Otros dispositivos/tarjetas no cambian.

Después: suite nativa GNU requerida completa, regresiones de diario/retorno/concurrencia y revisión del responsable. La prueba física requiere autorización y revisión propia, separada de los tests sintéticos.

## Punto de continuación

Implementación en `bank_recovery.rs`, selección remota tipada en `cloud_sync.rs`, operaciones exactas en `bank_changes.rs` y vía explícita en `wallet_export.rs`/`flasher.rs`. El token de comprobación física retiene la concesión exclusiva de almacenamiento hasta cerrar el diario. La publicación de una operación nueva usa rename de un directorio privado completo y validado. El CLI ofrece revisión y devolución separadas; «Restaurar original» usa el mismo flujo y exige USB literal. La prueba física autorizada terminó con diario completo, evidencia íntegra, cinco operaciones vinculadas restauradas y ninguna exportación pendiente. Comprobar por separado el conflicto de la otra tarjeta y la sincronización antes de anunciar resuelto el perfil real. Véase CONTINUACION-COPIAS-2026-10-24.md para el estado vigente.
