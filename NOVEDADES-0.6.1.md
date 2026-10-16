# Wallet Studio 0.6.1 — acceso de compradores

- Icono con esquinas transparentes y marco metálico exterior, sin cuadrado negro alrededor.
- Inicio de sesión obligatorio. Primera activación con correo de compra, código enviado al correo y contraseña repetida.
- Compra activa del producto específico Wallet Studio comprobada por el servidor. Los webhooks se verifican antes de conceder o revocar licencias.
- Máximo tres equipos registrados por cuenta. Sesiones Windows protegidas con DPAPI, revalidación al abrir y caducidad por inactividad.
- Gestión de equipos y recuperación por correo, incluida liberación expresa de plazas cuando se han perdido los equipos anteriores.
- Los originales permanecen inmutables, vinculados a su cuenta, iPhone y tarjeta. La recuperación de diarios pendientes sigue disponible aunque caduque la sesión.

Esta compilación es de revisión previa al despliegue. Necesita servicio HTTPS, SMTP y webhooks reales configurados para permitir el acceso. No sustituye todavía una instalación de trabajo: no se incluye un acceso alternativo sin compra ni un servidor ficticio. Las pruebas automatizadas usan exclusivamente datos sintéticos.

La galería abierta, el editor y las actualizaciones firmadas de 0.6.0 se conservan. No se ofrece animación demostrada en Apple Wallet ni paridad macOS validada.
