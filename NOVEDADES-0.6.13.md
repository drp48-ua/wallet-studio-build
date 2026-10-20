# Wallet Studio 0.6.13

## Cambios

- La vista «Prepara tu próxima tarjeta» utiliza el PNG preparado completo, sin reducirlo previamente a una miniatura de 500×320. El preparado mantiene el lienzo de 1536×969 y su reescalado Lanczos3.
- Durante la captura automática se muestra un indicador animado de guardado para la tarjeta correspondiente. Otras tareas no activan ese indicador.
- Se mantienen el relevo seguro entre escáner y captura, el guardado local sin exigir sincronización cloud y la comprobación de recuperaciones mediante diarios reales.
- Catálogo nativo 2K separado del catálogo compatible con clientes antiguos, con validación de dimensiones, tamaño y SHA-256. El catálogo incorpora los metadatos actualizados de los diseños revisados.
- Versión de interfaz, recurso Windows y paquete alineada en 0.6.13.

## Seguridad y alcance

Los originales no se sobrescriben. Una identidad incierta, un original corrupto o una recuperación pendiente bloquean nuevas operaciones hasta su revisión. Mantén el teléfono conectado durante una captura y conserva toda evidencia de recuperación.

Las pruebas automatizadas usan datos sintéticos y no acreditan por sí solas una transferencia física. Los GIF pueden reproducirse en el editor; no se anuncia animación en Apple Wallet. La ejecución en otros ordenadores depende de sus políticas de Windows: no se garantiza ausencia universal de avisos de Smart App Control.

El paquete no contiene imágenes privadas del catálogo, copias bancarias ni credenciales.
