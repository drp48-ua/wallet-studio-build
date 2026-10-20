# Wallet Studio 0.6.8

Prepara la biblioteca compartida de 56 diseños para todos los compradores con derecho al catálogo. Conserva intactos los 50 diseños anteriores y añade Sakura Fuji plata, Telaraña roja y oro, Mármol blanco y oro, Topografía esmeralda, La gran ola japonesa y Clásica negra y oro. Los seis nuevos PNG tienen 320 × 200 píxeles; el catálogo declara su tamaño y SHA-256 exactos. Las imágenes permanecen en almacenamiento privado y no se incluyen en el instalador ni en Git.

El cliente solicita expresamente el catálogo ampliado y muestra el número de diseños recibido. Acepta también los 50 diseños de un servidor anterior. El servidor conserva una respuesta de exactamente 50 diseños para las versiones 0.6.5–0.6.7; la ampliación requiere la capacidad autenticada `X-Wallet-Studio-Catalog: expanded-v1`. La cabecera no concede acceso: la sesión y los derechos se comprueban igual que antes en catálogo y descarga.

Para activar los seis diseños en todas las cuentas autorizadas hay que verificar sus objetos privados sin sobrescribir los anteriores y desplegar el servidor con el catálogo ampliado. Preparar este código o compilar la app no publica las imágenes ni despliega el servicio. Se mantienen las mejoras de biblioteca y sesión de 0.6.7; la investigación de GIF continúa aplazada.
