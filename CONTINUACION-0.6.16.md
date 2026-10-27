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

Los doce objetos de contenido inmutable se subieron con autorización explícita al almacenamiento privado existente. Se conservaron los objetos anteriores y el catálogo legado. El servicio se desplegó correctamente en f5d15ee21f373b1ca30e06d58ae2ab4b4745f45b, conservando los ajustes existentes. GitHub Actions 39580308628 terminó correctamente para ese mismo commit.

El diagnóstico autenticado de los doce originales actualizados terminó con doce verificaciones correctas (hash, tamaño, PNG y dimensiones), sin errores, en 80,21 segundos. Un recorrido general anterior alcanzó su límite de 240 segundos tras verificar nueve originales y nueve vistas previas, sin errores; no se afirma haber repetido la descarga de los otros 35 originales sin cambios en esta ejecución. Las 24 copias originales locales comparadas antes y después de ese diagnóstico general conservaron sus hashes y tamaños. No hubo operaciones físicas en el iPhone.

La app nativa 0.6.16 se abrió y revisó en demostración aislada con los nuevos recortes. El ejecutable de entrega usa el mismo código de producción aceptado; el diagnóstico adicional solo existe en cfg(test). ZIP verificado: SHA256 3a7f2c3a8ab180b46b98649e05aca00eab2b9bd7644867607f9f9d906c1e5426, 8.372.096 bytes; ejecutable SHA256 da8f464432adea7f4d19edb8a1049ee0018ea8300c50c004300b0d69db966636. Archivo de entrega: Wallet-Studio-0.6.16-Windows.zip. La prueba física de captura sigue pendiente de una sesión autorizada con iPhone disponible. No cambiar credenciales, planes, políticas ni rama principal. No publicar imágenes ni datos del teléfono en Git.

La revisión principal aceptó los cambios del worker configurado crack_builder. El panel opcional de Crack no estaba disponible. Para continuar, comprobar primero límites y estado Git; conservar cambios de colaboradores y trabajar en rama separada.

La app de entrega se abrió después con el perfil habitual: v0.6.16, cinco tarjetas guardadas, sesión dentro del estudio y ningún dispositivo conectado. Mostró «Una copia descargada requiere revisión. No se sobrescribió el original local». No se resolvió ese conflicto de datos ni se inició una captura para sortearlo. Tras abrirla se volvieron a comparar las 24 copias: cero diferencias. Si se retoma la prueba física, revisar primero esa copia cloud conservando la evidencia y los originales locales.
