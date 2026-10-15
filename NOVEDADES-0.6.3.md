# Wallet Studio 0.6.3

Incluye las mejoras de acceso, reenvío de código y sincronización de la 0.6.2.

Sincronizar ya no devuelve al formulario por un fallo temporal de conexión si la cuenta se verificó previamente en esta ejecución. Una respuesta HTTP 401/403 o una identidad distinta sí exige iniciar sesión de nuevo. Mantener la pantalla de cuenta no concede acceso al teléfono: cada operación sigue requiriendo su comprobación en línea.

Se elimina la comprobación duplicada de sesión en cada sincronización. El arranque y la sincronización muestran la fase real: preparación local, conexión, inventario, descarga o subida. Se puede reintentar la conexión con la sesión guardada sin volver a escribir la contraseña. Los errores del servicio muestran el código HTTP sin incluir tokens ni cuerpos de respuesta.

Las copias locales pendientes incluyen un resumen de motivos: original incompleto o no verificable, conflicto local, recuperación pendiente, historial pendiente o almacenamiento ocupado/protegido. No se cambia ninguna evidencia para darla por válida. Las copias sanas continúan sincronizándose.

La compilación y las pruebas utilizan perfiles y originales sintéticos. Las tres copias pendientes notificadas por el usuario requieren comprobar el nuevo motivo en el ordenador que las conserva; no se consideran reparadas.
