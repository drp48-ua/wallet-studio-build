# Wallet Studio 0.6.27

- Una importación interrumpida durante su preparación ya no deja un marcador global huérfano que impida el siguiente inicio.
- Antes de escribir originales importados se conserva el archivo exacto y una intención duradera. Una escritura parcial o una comprobación fallida mantiene el bloqueo y la evidencia para revisión.
- Las importaciones idénticas comprueban los archivos existentes sin crear otra barrera de escritura.
- El servidor conserva un código de activación nuevo aunque termine fallando un envío anterior del mismo correo. Este cambio requiere desplegar el servidor.
- Las pruebas del recorrido de compra, activación, biblioteca nativa y copia entre equipos utilizan cuentas ficticias y almacenamiento temporal.

No se eliminan bloqueos antiguos ni se declara recuperada una escritura parcial. La compatibilidad física depende del equipo, la versión de iOS y la tarjeta comprobados.
