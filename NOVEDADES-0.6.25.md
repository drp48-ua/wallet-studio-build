# Wallet Studio 0.6.25

La detección de una tarjeta nueva puede terminar mientras otras copias de la cuenta necesitan revisión. La versión anterior exigía que toda la cuenta estuviera sin incidencias antes de guardar un original nuevo, incluso cuando la incidencia pertenecía a otro iPhone.

Esta versión separa el resultado general de sincronización de la autorización de captura del iPhone seleccionado. Solo permite continuar después de terminar una comprobación autenticada y clasificar todas las copias. Una recuperación o incidencia del mismo iPhone sigue bloqueando nuevas capturas; una identidad desconocida, un inventario incompleto o una sesión no verificada también las bloquean. No borra ni declara resueltas las copias en revisión.

El mensaje del original distingue una comprobación en curso de una revisión terminada que todavía bloquea al dispositivo seleccionado. La búsqueda se detiene al detectar la tarjeta y el guardado automático continúa cuando se cumplen sus condiciones.

Se mantienen las verificaciones del original, la procedencia de la imagen, la conexión USB y la devolución comprobada. No se modifican los archivos del teléfono para resolver incidencias de otros dispositivos.
