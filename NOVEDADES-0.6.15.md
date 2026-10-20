# Wallet Studio 0.6.15

- Corregida la nitidez en todas las previsualizaciones de la aplicación (tarjeta activa, diseño preparado, biblioteca y fotogramas GIF).
- Previsualizaciones prefiltradas a resolución HiDPI exacta (768×485 para tarjetas principales, 384×242 para miniaturas de cuadrícula) utilizando remuestreo Lanczos3 de alta fidelidad.
- Se elimina el aliasing por submuestreo bilineal en egui que provocaba aspecto borroso al pasar texturas de 1536×969 o 2048×2048 directamente a recuadros de 420 px.
- Se mantiene el tamaño nativo intacto de `skin.png` a 1536×969 para transferencias y exportaciones a Apple Wallet.
- Conservadas todas las funcionalidades previas: captura automática instantánea, aislamiento de «Mis diseños», base visual glass nativa y catálogo 2K de 56 diseños.
