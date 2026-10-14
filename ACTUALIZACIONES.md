# Actualizaciones de Wallet Studio

Desde 0.6.0 la app comprueba automáticamente GitHub Releases en segundo plano. Un paquete firmado y validado se prepara en una carpeta independiente de originales e historiales. Se activa la próxima vez que abras la app; no interrumpe una transferencia en curso.

El canal está fijado a `drp48-ua/wallet-studio-build`. Requiere integrar el workflow en main, configurar el secreto privado de firma del editor y habilitar la publicación tras validar la distribución. Cada push probado puede producir una nueva build, incluso sin cambiar el número de versión. Consulta CLIENTES-Y-NUBE.md.

La app rechaza firmas no autorizadas, descargas alteradas, retrocesos, rutas peligrosas y paquetes con datos privados. La ausencia del canal no elimina los originales. Desde 0.6.1 se exige validar online la sesión de comprador para acceder a la herramienta, incluso si no hay actualizaciones. Las versiones anteriores sin este mecanismo requieren instalación manual inicial; descargar una app no traslada por sí solo copias de otro ordenador.

La sincronización con cuentas necesita el servicio online desplegado. El ZIP no contiene originales, datos de clientes, sesiones ni claves privadas. La firma del canal de actualización es independiente de Authenticode, que sigue pendiente de configurar para la distribución comercial.
