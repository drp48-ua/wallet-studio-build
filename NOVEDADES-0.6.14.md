# Wallet Studio 0.6.14

- Recuperada la base visual nativa de la versión anterior: degradados continuos, botones legibles, galería adaptable y fondos glass.
- Restaurado el cliente de catálogo native-2k-v2 con caché verificada y reintentos. La previsualización detallada mantiene resolución alta y filtrado Lanczos3.
- Conservadas las mejoras de captura automática, relevo seguro del escáner, recuperación desde diarios y spinner de la tarjeta que se está capturando.
- «Mis diseños» guarda solamente importaciones personales. Los diseños del catálogo se conservan para transferencia en una caché independiente, sin duplicarlos en la biblioteca personal.
- Las copias automáticas antiguas que coincidan con imágenes verificadas del catálogo se filtran de la lista sin borrar los archivos. Las nuevas importaciones explícitas conservan su procedencia incluso si coinciden con el catálogo.
- Recuperadas las comprobaciones de almacenamiento y sesión de la base anterior, sin eliminar copias bancarias ni modificar el baseline original.

La transferencia sigue siendo estática en Apple Wallet. Las pruebas automatizadas no acreditan operaciones físicas de USB ni inmunidad universal frente a Smart App Control.
