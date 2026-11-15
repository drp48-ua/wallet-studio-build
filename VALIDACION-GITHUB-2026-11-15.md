# Verificación de Wallet Studio del 15 de noviembre de 2026

Tras la actualización de los presupuestos realizada por el usuario, el segundo intento de Windows checks del commit d79ff74 se ejecutó correctamente. El bloqueo que impedía iniciar el trabajo no apareció en este intento.

Resultado: https://github.com/drp48-ua/wallet-studio-build/actions/runs/43727247106/attempts/2
Trabajo: https://github.com/drp48-ua/wallet-studio-build/actions/runs/43727247106/job/132854656437

Todos los pasos finalizaron con éxito: pruebas nativas sin hardware ni catálogo personal, compilación MSVC, pruebas del servidor con activación y separación de cuentas, sincronización del cliente con dos perfiles ficticios, preparación del paquete y subida del artefacto. El trabajo duró 15 min 29 s. Esto corresponde exactamente al commit indicado; no valida automáticamente los cambios posteriores de recuperación local.

El artefacto wallet-studio-windows está disponible en esa ejecución. Su digest publicado por GitHub es sha256:9fa64c2b7db217c69be7db05aee5ff48227bf87d372fb867461944c27db256b5. Es el digest del artefacto de pruebas, no una firma del actualizador ni el hash del EXE.

La comprobación remota de solo lectura confirmó dos depósitos R2 privados, lectura con integridad de las 15 copias indexadas y presencia de los 112 objetos de diseños y versiones anteriores. No modificó la base de datos ni los archivos remotos.

Con el iPhone preparado por el usuario, la consulta nativa recibió una entrada de usbmux y los componentes Apple enumeraron un dispositivo. Estas pruebas comprueban detección; no certifican una captura ni una transferencia de imagen.

La revisión local todavía distingue tarjetas verificadas de tarjetas pendientes de revisión. Resolver GitHub no retira por sí solo ese bloqueo. No se ha cambiado ninguna imagen del iPhone durante estas comprobaciones.

La captura de confirmación está guardada fuera de Git en C:/Users/danir/Documents/Codex/wallet-studio-github-pruebas-correctas-20261115.png. Las copias personales, identificadores, credenciales y registros privados permanecen excluidos del repositorio.
