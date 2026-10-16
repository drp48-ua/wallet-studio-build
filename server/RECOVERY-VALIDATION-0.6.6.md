# Recuperación 0.6.6 — validación y continuación

Cambio acotado: una exportación `export-uncertain` sin hash/copia retenida puede preparar el original desde un baseline completamente verificado del mismo dispositivo, tarjeta, sufijo `.pkpass` y archivo permitido con estado `Present`. La preparación es exclusivamente local: conserva los diarios anteriores, escribe procedencia explícita `verified-bank-baseline` y una copia nueva comprobada por lectura y SHA-256. El diario sigue pendiente.

La acción **Preparar recuperación desde mi original verificado** aparece tras la comprobación en segundo plano. No transfiere al teléfono. Después, **Recuperar original pendiente** reutiliza la devolución existente: preimagen de Libros, lectura protegida exacta de los bytes, devolución final y comprobación de Libros antes de completar. No certifica como original ninguna imagen personalizada actual del teléfono.

Pruebas sintéticas focalizadas: 3 aprobadas, 0 fallidas, 148 no seleccionadas. Verifican rechazo de otra identidad/ausencia/fase/hash existente, conservación de evidencia contradictoria y procedencia duradera sin completar el diario. Primera ejecución detectó una carpeta temporal no aceptada por la restricción de raíz de producción; se corrigió la parametrización privada de la prueba y se repitieron las tres. Registro privado ignorado: `server/test-runtime/baseline-recovery-tests.log`. No se repitieron las suites generales ya documentadas para 0.6.5.

Pendiente independiente: una importación cloud con historial local `prepared` mantiene la cuarentena y la marca de revisión. No se importa historia sobre un intento incierto. La restauración bancaria normal también respeta esa marca; resolver ese caso necesita un flujo de recuperación específico, verificación protegida de todos los originales/ausencias y revalidación del conjunto cloud, sin permitir aplicar diseños como excepción. No se ha implementado ni eludido ese bloqueo en 0.6.6.

No se ejecutó ninguna consulta o escritura al teléfono por el worker. La aceptación física/UI corresponde al lead. Según la comprobación física comunicada por el lead para 0.6.5, la detección finalmente seleccionó la tarjeta y terminó la búsqueda automáticamente; la captura sigue pendiente de recuperación/verificación.

Artefactos: compilación única de producción aprobada en 4m43s, fuente congelada en `22f82d399d78e9766ddfde7767aed37071eb556d`. Registro privado ignorado: `server/test-runtime/baseline-recovery-build.log`. Binario sin firma Authenticode; los PNG privados no se incluyen en el instalador.

Recepción final:

- EXE: `windows/Wallet Studio.exe`, versión de archivo/producto 0.6.6, 12.569.600 bytes; SHA-256 `ff1520265c20b286d6aaea7b7109a578e0c3d49de186576d1820063dd4cb5530`.
- ZIP: `dist/Wallet-Studio-0.6.6-Windows.zip`, 8.284.553 bytes; SHA-256 `b81ba0e15630cfe147f01ad931bbea27b91465b3022bd22c20fdf8debc2c7cfb`.
- CRC de todas las entradas, EXE empaquetado idéntico, LEEME/novedades 0.6.6: aprobados.
- Exclusión de los 50 hashes privados y sus nombres, carpetas de datos privados y diseños empaquetados limitados a README: aprobada.
- Recibo privado ignorado: `server/test-runtime/recovery-artifact-receipt-0.6.6.json`.
- No hay aceptación física de recuperación en esta recepción. El bloqueo cloud independiente descrito arriba permanece.
