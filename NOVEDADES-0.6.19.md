# Wallet Studio 0.6.19

- La tarjeta original distingue copia en espera, guardado en curso, revisión y original verificado. Se explica qué requisito falta y no se certifica un original solo porque exista una imagen.
- La pérdida del proceso de captura ya no deja la interfaz ocupada sin resultado. Se conservan originales y diario, y se revisan las recuperaciones antes de habilitar un reintento. La supervisión detiene y recoge el transporte también si falla una función interna.
- El guardado muestra el tiempo transcurrido y su fase. Si tarda demasiado deja de presentarse como una espera normal; el diario y los bloqueos se conservan hasta conocer el resultado real.
- Los componentes Apple necesarios se explican desde la app. La descarga del instalador oficial de iTunes de 64 bits se inicia desde su botón, con límite de tamaño/plazo, HTTPS, orígenes Apple y validación Authenticode del editor Apple antes de ofrecer la instalación. Windows y el usuario completan el instalador; no se usa instalación silenciosa.
- Se añade una vía de soporte limitada para revisar y devolver el original de una única escritura interrumpida ya registrada. Exige original local válido, identidad exacta, un único iPhone USB y lectura protegida posterior. No admite rutas, diseños ni identidades por argumentos.

Se mantiene la descarga de originales con plazo de cinco minutos y comprobación de tamaño/SHA-256. Una recuperación sin confirmar conserva las copias; esperar no la completa ni autoriza una escritura nueva.

Limitación conocida: si el iPhone contiene un diseño propio de una operación remota posterior al intento local interrumpido, la recuperación conserva el bloqueo. Esta versión no elimina ese diseño usando una excepción de propiedad ni declara completada su restauración.
