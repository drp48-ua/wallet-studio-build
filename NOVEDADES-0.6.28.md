# Wallet Studio 0.6.28

El acceso a la cuenta y la biblioteca ya no queda bloqueado por una revisión pendiente de las copias locales cuando este PC y el propietario del perfil pueden verificarse.

- La sesión de biblioteca y su caché se guardan aparte de las copias pendientes.
- Si falta esa sesión separada, «Recuperar mi sesión guardada» verifica la cuenta anterior sin cambiar el propietario ni los originales.
- Recuperar una sesión no sobrescribe un acceso más reciente ni anula un cierre de sesión realizado durante la comprobación.
- Los mensajes distinguen una sesión ausente de una sesión que no se pudo leer.
- Se conserva la identidad del mismo PC; no se ocupa otra plaza de equipo para utilizar este acceso.
- Las opciones de solicitar activación o recuperar acceso permanecen visibles y disponibles durante la revisión local.
- La biblioteca explica el acceso limitado y permite volver a comprobar el almacenamiento.
- Sincronizar copias, capturar y cambiar imágenes del iPhone siguen requiriendo un almacenamiento verificado. No se eliminan marcadores antiguos ni se sustituyen originales para permitir el acceso.

Una incidencia del servicio de cuentas o de su base de datos aún puede impedir verificar la licencia. La app no presenta una sesión sin verificar como autorización para modificar el iPhone.
