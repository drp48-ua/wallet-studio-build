# Wallet Studio 0.6.23

La reutilización de un diseño comprado conserva el PNG original y su resolución. Antes de aplicarlo, el cliente comprueba el permiso vigente y los metadatos del catálogo. Solo reutiliza el archivo local si su SHA-256, tamaño y dimensiones coinciden con la versión autorizada.

Un permiso retirado, una sesión sin validar, un catálogo inválido o una versión distinta del diseño bloquean la aplicación del archivo antiguo. Los originales ausentes o dañados siguen requiriendo descarga autenticada y verificación completa.

Incluye los arreglos anteriores de sincronización por partes de hasta 32 MiB, revisión por tarjeta y conservación de archivos antiguos de nombres. No elimina originales, historiales ni evidencia de recuperación. El cambio de caché reduce transferencias futuras; no restablece una cuota de Supabase ya agotada.

Validación nativa local: 234 pruebas correctas, cero fallidas, siete ignoradas y cuatro exclusiones de hardware. La prueba de caché reutilizada hizo una consulta de metadatos y ninguna descarga de PNG; conservó exactamente los bytes y dimensiones del original. Se comprobaron también permisos retirados, metadatos cambiados, fallos de autenticación, ausencia de conexión y alteración del archivo durante la consulta. No se probó la conexión real con Supabase, que sigue restringida por cuota.
