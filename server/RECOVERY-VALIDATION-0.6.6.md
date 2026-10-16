# Recuperación 0.6.6 — validación y continuación

Cambio acotado: una exportación `export-uncertain` sin hash/copia retenida puede preparar el original desde un baseline completamente verificado del mismo dispositivo, tarjeta, sufijo `.pkpass` y archivo permitido con estado `Present`. La preparación es exclusivamente local: conserva los diarios anteriores, escribe procedencia explícita `verified-bank-baseline` y una copia nueva comprobada por lectura y SHA-256. El diario sigue pendiente.

La acción **Preparar recuperación desde mi original verificado** aparece tras la comprobación en segundo plano. No transfiere al teléfono. Después, **Recuperar original pendiente** reutiliza la devolución existente: preimagen de Libros, lectura protegida exacta de los bytes, devolución final y comprobación de Libros antes de completar. No certifica como original ninguna imagen personalizada actual del teléfono.

Pruebas sintéticas focalizadas: 3 aprobadas, 0 fallidas, 148 no seleccionadas. Verifican rechazo de otra identidad/ausencia/fase/hash existente, conservación de evidencia contradictoria y procedencia duradera sin completar el diario. Primera ejecución detectó una carpeta temporal no aceptada por la restricción de raíz de producción; se corrigió la parametrización privada de la prueba y se repitieron las tres. Registro privado ignorado: `server/test-runtime/baseline-recovery-tests.log`. No se repitieron las suites generales ya documentadas para 0.6.5.

Resolución de la dependencia circular (AC1 & AC2):
- Desacoplamiento estricto: la restauración del baseline bancario verificado (`restore_bank_wallet_original`) no se bloquea por la marca de revisión `card-review`. Si la tarjeta está bajo revisión, la interfaz mantiene `OriginalState::Blocked` (impidiendo terminantemente "Aplicar diseño"), pero habilita "Restaurar original" cuando existe un baseline íntegro y el intento local no ha alcanzado la fase `restored`.
- Reconciliación estricta de cuarentena: tras completar la devolución física, comprobar ausencias y realizar lecturas protegidas completas, el intento transiciona a `restored`. La función `reconcile_card_quarantine` compara la unión íntegra de recursos locales y del paquete en cuarentena. Únicamente si el conjunto se importa limpiamente sin discrepancias, se elimina el archivo en cuarentena y se libera la marca en `card-review/<card_key>`.

Verificación de catálogo y revocación real (AC4):
- Se comprobó la integridad de los 50 diseños en `server/design-catalog.json`: 50 identificadores (`ws-001` a `ws-050`), 50 hashes SHA-256 únicos de 64 caracteres hex, 320×200 píxeles, límites de tamaño cumplidos.
- Suite automatizada `server/tests/test_client_catalog_and_revocation.py`: 49 pruebas unitarias aprobadas en total (46 originales + 3 de catálogo/revocación, 1 omitida por PostgreSQL opcional).
- Se verificó la revocación inmediata ante webhook `refunds/create` de Shopify con HMAC-SHA256 válido: el catálogo pasa a `entitled: false`, la descarga devuelve HTTP 403 `design_pack_required` y los bundles devuelven HTTP 401.
- Sondeo en vivo al backend de Render (`https://wallet-studio-accounts.onrender.com`): `/health` responde HTTP 200 `{"status":"ok"}` y `/v1/designs` sin autorización responde HTTP 401 `{"error":"unauthorized"}`.

No se ejecutó ninguna consulta o escritura al teléfono por el worker. La aceptación física/UI corresponde al lead. Según la comprobación física comunicada por el lead para 0.6.5, la detección finalmente seleccionó la tarjeta y terminó la búsqueda automáticamente; la captura sigue pendiente de recuperación/verificación.

Artefactos: compilación única de producción aprobada en 4m43s, fuente congelada en `22f82d399d78e9766ddfde7767aed37071eb556d`. Registro privado ignorado: `server/test-runtime/baseline-recovery-build.log`. Binario sin firma Authenticode; los PNG privados no se incluyen en el instalador.

Recepción final:

- EXE: `windows/Wallet Studio.exe`, versión de archivo/producto 0.6.6, 12.569.600 bytes; SHA-256 `ff1520265c20b286d6aaea7b7109a578e0c3d49de186576d1820063dd4cb5530`.
- ZIP: `dist/Wallet-Studio-0.6.6-Windows.zip`, 8.284.553 bytes; SHA-256 `b81ba0e15630cfe147f01ad931bbea27b91465b3022bd22c20fdf8debc2c7cfb`.
- CRC de todas las entradas, EXE empaquetado idéntico, LEEME/novedades 0.6.6: aprobados.
- Exclusión de los 50 hashes privados y sus nombres, carpetas de datos privados y diseños empaquetados limitados a README: aprobada.
- Recibo privado ignorado: `server/test-runtime/recovery-artifact-receipt-0.6.6.json`.
- Desacoplamiento y reconciliación de recuperación implementados y validados sintéticamente. La aceptación física de la devolución en el dispositivo real se realiza en el host autorizado.
