# Wallet Studio 0.5.3 para Windows

Paquete portable: extrae todos los archivos en una carpeta de tu usuario con permiso de escritura (por ejemplo, `%LOCALAPPDATA%\Programs\Wallet Studio`). Ejecuta `Wallet Studio.exe` o `ABRIR-WALLET-STUDIO.cmd`. No lo ejecutes dentro del ZIP ni desde Program Files: la biblioteca `designs` guarda tus diseños junto al ejecutable.

Lee `INSTALACION.md` antes de conectar un iPhone y `ACTUALIZACIONES.md` antes de sustituir el programa. El paquete no incluye controladores Apple.

Condiciones comerciales previstas: 6,99 €, pago único, dos ordenadores Windows por compra y actualizaciones incluidas indefinidamente, sin suscripción. Soporte: javicarmelosomanager@gmail.com. Esta preparación todavía no implementa el control de activaciones. Un archivo VERSION.json con `activationEnforced: false` declara esa limitación; no acredita una licencia ni controla equipos.

## Uso

1. Instala los componentes oficiales indicados en INSTALACION.md. Conecta el iPhone por USB, desbloquéalo y acepta «Confiar». Cierra Wallet y Libros antes de transferir.
2. Añade o selecciona tu tarjeta y espera a que termine la captura verificable del original. No apliques cambios si hay captura o recuperación pendiente.
3. Carga una imagen propia o un diseño que tengas derecho a utilizar. Ajusta el recorte y pulsa **Aplicar diseño**. La biblioteca guarda el PNG preparado antes de transferirlo.
4. Comprueba el resultado en el iPhone. La galería representa el último diseño verificado localmente, no una lectura en vivo.
5. Para volver al original, conecta el mismo iPhone, usa **Restaurar copia original** y espera la confirmación. Conserva todos los diarios y copias incluso si una operación falla; no intentes otra escritura para sortear una recuperación pendiente.

Los GIF se reproducen dentro del editor; la transferencia habitual sigue generando un fotograma estático PNG/PDF. No se garantiza animación en Apple Wallet. Los historiales de diagnóstico de esquema 2 requieren un cliente compatible incluso tras restaurar; no retrocedas a versiones antiguas.

La carpeta designs solo contiene material expresamente aprobado para la distribución. Actualmente no hay imágenes comerciales aprobadas; puedes cargar tus propios archivos. Nunca envíes al soporte copias bancarias, identificadores de tarjetas o iPhone, registros sin revisar ni diseños privados.

La licencia MIT del código y las licencias de terceros acompañan la aplicación. Las condiciones comerciales no revocan los derechos de esas licencias.
