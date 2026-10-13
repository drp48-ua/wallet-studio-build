# Wallet Studio

Aplicación de escritorio que personaliza imágenes de tarjetas existentes en Apple Wallet. El código Windows está en `windows-source`; `macos-source` conserva una base macOS que todavía requiere portar y validar las mejoras funcionales Windows.

## Desarrollo

- Mantén la interfaz en español y la identidad visible Wallet Studio.
- Conserva las licencias y atribuciones de los proyectos originales y dependencias.
- Windows: `cargo test --release --locked --manifest-path windows-source/Cargo.toml -- --skip test_usbmux_query --skip test_list_connected_devices --skip test_syslog_service_receive --skip test_saved_cards_purging`.
- Compila con `scripts/build-windows.ps1` desde una consola con herramientas MSVC/Windows SDK y Rust estable.
- En un entorno Linux cloud, usa `cargo metadata --locked --no-deps --manifest-path windows-source/Cargo.toml` para inspección. Las pruebas y build nativos Windows corresponden al workflow Windows; no presentes una comprobación Linux como validación USB o Windows.
- Los cambios de interfaz deben revisarse con la app abierta: ventanas pequeñas y grandes, estados vacío/desconectado/conectado, botones habilitados y deshabilitados.

## Integridad de tarjetas

- No modifiques ni sustituyas el baseline original al cambiar diseños.
- Exige identidad exacta de dispositivo/tarjeta, copia verificable, diario duradero y lectura protegida después de transferir.
- Los estados inciertos deben conservar evidencia y bloquear una nueva escritura hasta resolver la recuperación.
- La galería muestra el último diseño aplicado y verificado localmente. No debe afirmar que lee el estado actual del iPhone en tiempo real.
- Un GIF completo se puede reproducir dentro del editor Windows. La transferencia sigue generando un fotograma estático PNG/PDF. No anuncies animación en Apple Wallet sin evidencia y soporte específico. Conserva junto al código `test-assets/rotating-earth.gif`, requerido por las pruebas, y su licencia independiente.
- El diagnóstico APNG 0.5 registra dos PNG y ausencia explícita de PDF (esquema 2), solo cuando el baseline prueba PDF originalmente ausente. No retires un PDF original para ampliar el experimento. El lector nuevo conserva compatibilidad con esquema 1; clientes antiguos no leen historial 2, incluso restaurado. Conserva un binario compatible y toda la evidencia.
- Las pruebas físicas requieren autorización humana y un iPhone disponible. Los entornos cloud no tienen acceso al USB local.
- Nunca subas copias bancarias, manifiestos personales, identificadores de dispositivos/tarjetas, registros de teléfono, secretos ni diseños privados.

