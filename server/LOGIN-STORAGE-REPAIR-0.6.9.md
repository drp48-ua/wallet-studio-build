# Reparación de acceso y almacenamiento — 0.6.9

## Causa comprobada

La versión 0.6.8 usaba `create_new` sobre `.vault-operation.lock`. Conservaba el handle durante toda la operación y borraba el marcador al salir normalmente. Terminar el proceso antes de ese borrado dejaba un marcador sin propietario; la siguiente preparación fallaba antes de migrar o verificar las copias. La interfaz descartaba la causa y seguía describiendo el estado como preparación en curso, dejando activación, contraseña y recuperación deshabilitadas.

El diagnóstico de host fue solo de lectura: marcador antiguo presente y apertura exclusiva de lectura disponible; no había marcadores globales de migración, importación o conflicto. No se leyeron credenciales, identidades ni contenido de originales para ese diagnóstico. No se ejecutó ningún script externo para desbloquear el perfil.

## Contrato de la corrección

- La app adquiere el mismo archivo mediante un handle exclusivo de Windows. También excluye una versión anterior que todavía conserva su handle abierto.
- Antes de recuperar un token antiguo válido, conserva sus bytes exactos en `lock-recovery/<sha256>.legacy-lock`, mediante creación exclusiva, escritura duradera y lectura de comprobación. Nunca reemplaza un archivo de evidencia diferente.
- Solo después solicita borrado al cerrar **ese mismo handle**. Windows mantiene la exclusión hasta que cierra y elimina el archivo, también si el proceso termina inesperadamente. No existe una ventana de borrar-después-de-liberar que pueda borrar el bloqueo de otra instancia. Tras cerrar, una versión anterior puede volver a adquirir el archivo con `create_new`.
- Si el registro es malformado, contiene un enlace o no se puede conservar/verificar la evidencia, no se arma el borrado y se conserva el marcador. Los permisos y errores se mantienen cerrados de forma segura.
- La comprobación de disponibilidad solo prueba exclusión; no elimina ni archiva el archivo. La migración y las verificaciones de originales/diarios siguen ejecutándose antes de habilitar acciones de cuenta.
- Los marcadores de migración/importación/conflicto, la revisión de tarjetas, los diarios pendientes, la separación de cuentas, la compra obligatoria y el máximo de tres equipos no se eluden.
- La interfaz distingue operación en curso de preparación terminada con error; permite reintentar y muestra una categoría acotada sin rutas o datos privados.

La implementación utiliza `DELETE` y `FileDispositionInfo` según [SetFileInformationByHandle de Microsoft](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-setfileinformationbyhandle), con el `BOOLEAN` definido en [FILE_DISPOSITION_INFO](https://learn.microsoft.com/en-us/windows/win32/api/winbase/ns-winbase-file_disposition_info). No se usa `CREATE_ALWAYS`, truncado ni un borrado externo del marcador.

## Pruebas y recepción

Resultado: pruebas requeridas, integración sintética Windows y compilación/paquete 0.6.9 aprobados. Los registros detallados se guardan bajo `server/test-runtime/`, ignorado por Git. La primera ejecución Python en sandbox falló por permisos de temporales y sockets; se corrigió el entorno de las pruebas, sin cambiar ni debilitar los tests.

Pruebas añadidas: recuperación de marcador antiguo con copia exacta; exclusión de cliente antiguo activo y readquisición posterior; terminación de proceso hijo libera bloqueo; anidamiento; fallo de evidencia/marcador malformado; conservación de marcadores y de diarios válidos e inválidos; acciones de cuenta solo tras preparación correcta; diagnóstico sin información privada.

La aceptación real de arranque, botones y conservación de hashes de originales corresponde al lead. El worker no inicia sesión con credenciales del usuario, no consulta el teléfono y no realiza transferencias.

### Resultados finales del worker

- Rust: 164 pruebas unitarias y 1 prueba de motor aprobadas; 4 pruebas de hardware omitidas según AGENTS. Dos entradas ignoradas por el runner general: ayudante de proceso hijo (invocado por su prueba) e integración e2e (ejecutada explícitamente después).
- Integración Windows de nube/cuenta con dos perfiles sintéticos, WinHTTP y DPAPI: aprobada.
- Servidor: 61 pruebas ejecutadas, 2 omisiones opcionales, sin errores en el entorno adecuado.
- Compilación única GNU Windows de producción: aprobada en 3m48s. Versión de archivo y producto: 0.6.9. Sin firma Authenticode.
- CRC de todas las entradas ZIP, EXE empaquetado idéntico, manifiesto de distribución y LEEME/novedades actuales: aprobados.
- Los 56 hashes de diseños privados, sus nombres y los datos del perfil están excluidos del ZIP. La carpeta `designs` solo contiene README.
- `git diff --check`: aprobado. Ningún cambio de código después de congelar la fuente revisada por el lead; solo se completó esta recepción.

### Artefactos para aceptación del lead

- EXE: `windows/Wallet Studio.exe`, 12.650.496 bytes. SHA-256 `284550b091e0f81e20e3b9f14d3b623f451c3726c048652548276638059e9787`.
- ZIP: `dist/Wallet-Studio-0.6.9-Windows.zip`, 8.318.814 bytes. SHA-256 `d668552ffc4b376ceb8ef38bfdd988280c9405da7429c7421d0250f040e17d57`.
- Recibo privado ignorado: `server/test-runtime/login-storage-artifact-receipt.json`.
- Registros privados ignorados: `login-storage-required-tests.log`, `login-storage-desktop-integration.log`, `login-storage-server-tests.log` y `login-storage-build.log`, bajo `server/test-runtime/`.

Pendiente: el lead debe aceptar el arranque del candidato en el perfil real, comparar los hashes originales capturados antes y comprobar las acciones de cuenta sin exponer credenciales. El worker no ha modificado el perfil real, ejecutado el candidato contra ese perfil ni consultado/escrito el teléfono. No se ha hecho commit, push, publicación o despliegue por el worker.
