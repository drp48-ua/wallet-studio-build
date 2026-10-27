# Wallet Studio 0.6.24

- El aviso de componentes Apple aparece al inicio, también antes de iniciar sesión. Distingue componentes ausentes de un servicio detenido o no consultable.
- El botón descarga el instalador oficial de iTunes para Windows de 64 bits y verifica su firma Apple antes de ofrecer la instalación. Conecta por USB, desbloquea el iPhone y acepta Confiar; la instalación requiere tu confirmación.
- Ayuda conserva la búsqueda de actualizaciones firmadas y elimina el enlace a GitHub. La demostración no inicia consultas de actualización.
- La compilación MSVC de distribución enlaza el runtime estáticamente y comprueba que el ejecutable no dependa de VCRUNTIME, MSVCP o CONCRT externos.

Las actualizaciones sólo se distribuyen después de integrar en main y configurar el canal de publicación aprobado y su clave de firma. La prueba física satisfactoria en el iPhone del usuario no certifica todos los modelos, versiones de iOS o instalaciones de Windows. Los componentes de Apple compatibles siguen siendo necesarios; no se incluyen sus DLL en el paquete.

Los componentes Apple se buscan en las carpetas de sistema indicadas por Windows, también en otras unidades. Una caché de instalador que no supera la firma se conserva antes de volver a descargar tras tu clic. La versión prevalece sobre el número de compilación al pasar de builds locales a publicaciones de main, manteniendo la protección frente a versiones anteriores y modificaciones de una misma publicación.
