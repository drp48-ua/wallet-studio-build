# Wallet Studio 0.6.16 — continuación

## Base verificada

Se revisaron todas las ramas de wallet-studio-build y los repositorios privados wallet-studio y wallet-studio-glassmorphism. La aplicación nativa más reciente al comenzar era a3af18d239891e123788b6def6c9b291326564dd, versión 0.6.15. Esta rama conserva su historial completo, captura automática, retiro del canal del escáner, overlays, biblioteca personal y vistas previas Lanczos3 de 768×485.

## Correcciones

- Captura directa, automática y reintento tras recuperación requieren licencia activa de la app, sesión resuelta y reconciliación cloud terminada. La solicitud detectada espera sin consumir su intento.
- El servidor libera conexiones terminadas y mantiene el GET exacto /health disponible bajo saturación; las operaciones protegidas siguen limitadas.
- Cinco nuevos recortes de presentación: ws-001, ws-002, ws-012, ws-015 y ws-053. Las otras 51 entradas de la última 0.6.15 no cambian. Se conservan cenefas, animales, coches y retratos.
- PNG RGB 1536×969, sin cuantización ni miniaturas como originales. Esta es la resolución efectiva heredada del catálogo llamado 2K; el recorte no crea detalle nuevo. scripts/prepare-library-presentation-crops.py reproduce los cinco resultados con entradas y salidas fijadas por SHA256. Requiere los originales privados; no están en Git ni en el ejecutable.

## Validación realizada

179 pruebas de app y una de motor; fixture Windows/WinHTTP/DPAPI; 77 pruebas de servidor (dos omisiones opcionales); 18 pruebas de catálogo repetidas tras editar sus metadatos. Se comprobaron los doce PNG que hay que añadir respecto al catálogo previamente desplegado y se reprodujeron los cinco recortes. Las comprobaciones del paquete excluyen imágenes privadas y datos de ejecución. No se realizó captura ni transferencia física al iPhone.

## Publicación y continuación

Los doce objetos de contenido inmutable se subieron con autorización explícita al almacenamiento privado existente. Se conservaron los objetos anteriores y el catálogo legado. Antes del despliegue, el servicio seguía en c651e49. Pendiente al crear este checkpoint: desplegar esta rama por commit, verificar GET autenticado y hashes de originales, finalizar revisión de interfaz y entregar el ZIP 0.6.16. No cambiar credenciales, planes, políticas ni rama principal. No publicar imágenes ni datos del teléfono en Git.

La revisión principal aceptó los cambios del worker configurado crack_builder. El panel opcional de Crack no estaba disponible. Para continuar, comprobar primero límites y estado Git; conservar cambios de colaboradores y trabajar en rama separada.
