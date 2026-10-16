# Wallet Studio 0.6.2

## Acceso renovado

Paleta azul noche, cian y lavanda adaptada al logo. Presentación con panel de bienvenida y formulario de acceso; en ventanas estrechas se muestra una sola columna. Campos más contrastados, mensajes de estado visibles, opción para mostrar contraseña y aviso cuando las dos contraseñas no coinciden.

La activación y recuperación permiten reenviar el código desde el mismo formulario. El botón espera 60 segundos entre solicitudes y muestra una cuenta atrás de la validez de 10 minutos. El correo queda fijo durante la verificación; para cambiarlo se vuelve al paso anterior. El servidor sigue siendo la autoridad de caducidad, elegibilidad y límites. El código anterior deja de valer cuando el servidor emite uno nuevo.

## Copias y sincronización

La pestaña Cuenta incluye «Sincronizar ahora», contadores de subidas, descargas y copias verificadas, y mensajes específicos sobre la fase que necesita revisión. Una cuenta vacía no se presenta como si ya hubiera copias almacenadas.

Una copia local incompleta o con recuperación pendiente se conserva sin subirla y sin impedir que se suban otros originales verificados. El estado sigue indicando sincronización parcial y bloquea la captura de originales ausentes. Los conflictos de originales descargados siguen bloqueados y nunca se sobrescriben.

Una confirmación antigua local no impide volver a subir un paquete que ya no figure en el inventario remoto. Las nuevas subidas exigen confirmación del identificador, tamaño y SHA-256 antes de registrar el recibo.

Se mantienen el acceso por compra, el máximo de tres equipos, las sesiones protegidas y el almacenamiento privado por cuenta. No se habilita el canal automático ni se cambia main.

Las pruebas automáticas utilizan datos sintéticos. Una prueba real de copias requiere iniciar sesión con el propietario y disponer del ordenador que conserva sus originales. No se incluyen copias, sesiones ni secretos en el paquete.
