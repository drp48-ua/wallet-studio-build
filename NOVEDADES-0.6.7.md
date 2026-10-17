# Wallet Studio 0.6.7

La biblioteca carga hasta cuatro miniaturas en segundo plano. Una imagen lenta no detiene las demás. Mantiene el espacio de cada imagen, conserva su proporción y suaviza su aparición. Cambiar de pestaña conserva las miniaturas ya cargadas. Los fallos tienen dos reintentos automáticos con espera y un reintento manual; ya no se guardan como imágenes vacías definitivas. La caché descargada se verifica y reemplaza de forma atómica, separada por cuenta y servicio.

Las imágenes se seleccionan pulsando su miniatura. Una vista ampliada muestra el diseño elegido y un único botón «Usar diseño seleccionado» sustituye los botones repetidos. La preparación se ejecuta en segundo plano. Solo cuando termina correctamente abre Tarjetas con la imagen cargada en el editor. El teléfono no se modifica al seleccionar o usar un diseño. Los errores conservan la selección y el diseño anterior.

La sesión cifrada de cada equipo se conserva cuando una conexión temporalmente fallida impide comprobarla al arrancar. La app muestra una conexión pendiente y permite seguir preparando diseños; reintenta automáticamente y ofrece «Sincronizar ahora» en Cuenta. La sesión pendiente no equivale a licencia verificada y bloquea nuevos diseños, teclados y capturas del teléfono. Un rechazo explícito sigue solicitando autenticación y retira únicamente el token local rechazado, sin borrar un inicio de sesión más reciente. Se mantienen revocación, recuperación de contraseña y caducidad de 30 días sin uso o 90 días totales.

Conserva las mejoras de recuperación de originales de la versión 0.6.6. La investigación de animación en Wallet queda fuera de esta versión.
