# Wallet Studio: código y diagnósticos de compilación

Repositorio público de compilación separado, publicado con autorización del propietario. Esta rama contiene Windows 0.6.1: galería abierta, originales inmutables, sincronización privada, actualización firmada y acceso obligatorio de compradores con un máximo de tres equipos. También conserva dependencias fijadas, atribuciones y diagnósticos iOS sintéticos. No incluye copias bancarias, estado personal, diseños privados, conversaciones ni reportes de teléfono.

El servidor de cuentas requiere despliegue HTTPS, producto Shopify y envío de códigos por correo. No se activa por subir código. Consulta [despliegue de cuentas](server/ACCOUNTS-DEPLOYMENT.md), [Render](server/RENDER.md) y [continuación](CONTINUAR-CLIENTES-2026-10-14.md). Un binario sin servicio oficial configurado conserva las copias locales y bloquea el acceso; no debe distribuirse como producto terminado. La base macOS requiere portar y validar estas funciones.

Las pruebas Windows se ejecutan mediante el workflow Windows checks. Los ZIP portables estarán en los artefactos de las ejecuciones correctas. La publicación automática desde main permanece desactivada hasta configurar y aprobar el servicio y el canal firmado.

La animación en Wallet sigue sin demostrarse. Los diagnósticos iOS acceden exclusivamente a objetos sintéticos y al directorio de su propia aplicación.
