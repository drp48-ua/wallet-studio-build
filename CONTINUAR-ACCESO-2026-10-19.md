# Acceso y almacenamiento — 19 de octubre de 2026

## Base y trabajo aislado

Base de la nueva corrección: b5ce005e4fed1fa956ce32ba8d922ff550ed3f19, Wallet Studio 0.6.8, rama remota codex/shared-library-six-designs-2026-10-17. Worktree dedicado wallet-studio-login-repair; rama codex/login-storage-repair-2026-10-19. No se cambiaron otros worktrees ni main.

El propietario confirmó que no puede acceder, activar cuenta ni recuperar contraseña. Las capturas mostraban información de autenticación: no se utiliza, reproduce ni guarda en código, pruebas, documentos o Git. Cualquier entrada real de credenciales corresponde al propietario.

## Causa y contrato revisado

Diagnóstico local limitado a metadatos: quedó un archivo de exclusión de una ejecución interrumpida. El cliente antiguo usa create_new y conserva el handle durante toda la operación; al interrumpirse queda el archivo, y cualquier preparación posterior falla. El arranque convertía el fallo en un estado perpetuo de preparación que deshabilitaba acciones de cuenta.

La corrección debe excluir por handle real de Windows, preservar los bytes antiguos en auditoría privada duradera y solo después marcar el mismo archivo abierto como delete-pending. Liberar o terminar el proceso debe retirar ese archivo activo sin carrera de borrado posterior. Los clientes antiguos y nuevos deben excluirse durante la operación y poder volver a adquirir al terminar. Un archivo malformado, enlace o fallo de auditoría no autoriza desbloquear. Los diarios de recuperación, revisión por tarjeta, identidad exacta y originales conservan sus reglas.

El estado de acceso debe distinguir preparación en curso, preparación terminada y fallo concreto. No habilitar operaciones bancarias ni eludir licencia, sesión o límite de equipos por un error local. No consultar ni escribir el teléfono durante esta reparación.

## Aceptación pendiente

Implementación y pruebas del mismo crack_builder, revisión del lead, build y paquete 0.6.9; después iniciar el candidato con acceso normal al perfil Windows, verificar preparación y acciones de cuenta, y dejar la entrada de contraseña/códigos al propietario si la sesión guardada no se recupera. No se ha probado aún el candidato ni se afirma que el acceso funcione.

## Revisión del lead antes de abrir el candidato

El lead revisó el diff congelado: exclusión del handle, permisos DELETE, validación del marcador, auditoría duradera previa al borrado del mismo handle, compatibilidad con create_new anterior y conservación de todos los controles bancarios. No pidió correcciones. Se comunicaron 164 pruebas Rust correctas más una del motor, dos ignoradas y cuatro de hardware filtradas; integración real WinHTTP/DPAPI con dos perfiles sintéticos correcta; 61 pruebas del servicio correctas y dos integraciones opcionales omitidas. Compilación única 0.6.9 en curso.

Se cerró normalmente la 0.6.8 bloqueada antes de probar el reemplazo. Se registraron huellas de los 20 archivos de bank-backups en un recibo privado ignorado para comprobar que el arranque corregido no altera originales. No se ha accedido al teléfono ni utilizado información de autenticación del propietario. La rama fuente remota sigue en b5ce005.

## Aceptación real del acceso y ampliación de alcance

El candidato 0.6.9 se abrió desde el ZIP final con acceso normal al perfil Windows. La preparación terminó y recuperó la sesión guardada: se mostró el estudio con Tarjetas, Biblioteca y Cuenta, sin usar credenciales del propietario ni enviar códigos. Comparación privada después del arranque: los 20 archivos originales permanecen presentes, mismo tamaño y SHA256; cero diferencias. Existe una copia auditada del bloqueo anterior. La interfaz dejó atrás el error de almacenamiento.

Paquete de entrega Documents/Codex/Wallet-Studio-0.6.9-Windows.zip; SHA256 d668552ffc4b376ceb8ef38bfdd988280c9405da7429c7421d0250f040e17d57. EXE SHA256 284550b091e0f81e20e3b9f14d3b623f451c3726c048652548276638059e9787.

El propietario añadió después un defecto de biblioteca: imágenes no disponibles y baja nitidez tanto en app como al transferir al móvil. Pide resolución del archivo original. El diagnóstico de diseño/servidor/descarga/caché/exportación se hará sobre esta rama, conservando la reparación de acceso y sin atribuir la causa a designs.py antes de verificarla. No degradar el original comercial a 320×200 ni confundir reescalado con detalle nativo. Las imágenes comerciales y los tokens permanecen fuera de Git.
