# Wallet Studio 0.5.2

El editor incorpora «Recortar imagen…». Muestra la imagen original y un marco con proporción de tarjeta: se puede mover, ajustar desde las esquinas y controlar con los deslizadores. «Aplicar recorte» prepara los píxeles seleccionados para la vista previa, la exportación PNG y la transferencia PNG/PDF. Cancelar conserva el diseño anterior; restablecer recupera la selección inicial. También sirve para el fotograma pausado de un GIF. La fuente completa permanece disponible para volver a editar.

Al aplicar o sustituir un diseño, Wallet Studio guarda automáticamente el PNG preparado exacto en `designs`, junto al ejecutable, antes de comenzar la transferencia. Comprueba los bytes guardados, reutiliza una copia idéntica y nunca sustituye un archivo existente. Si no puede guardar la copia, no inicia la transferencia. El diseño también se conserva si el intento de transferencia posterior falla. La biblioteca se actualiza automáticamente y muestra hasta cien diseños recientes; los demás archivos permanecen en la carpeta.

Los controles de identidad, copias originales, diario y recuperación siguen vigentes. Esta versión prepara imágenes estáticas para Apple Wallet; la investigación de animación nativa permanece en la rama de diagnóstico.
