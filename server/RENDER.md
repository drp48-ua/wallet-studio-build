# Despliegue de cuentas en Render

El archivo `render.yaml` prepara una única instancia Docker en Frankfurt, HTTPS gestionado por Render y disco persistente de 10 GB. SQLite y los archivos de originales residen juntos en `/data`. No ejecutar varias réplicas ni sustituir el disco por almacenamiento efímero.

## Coste y estado

La configuración selecciona `0.5c-512mb`: 7 USD/mes de servicio y 2,50 USD/mes por 10 GB de disco, aproximadamente **9,50 USD/mes** antes de impuestos, consumo adicional y correo. Revisar el presupuesto que muestre Render antes de aplicar. No hay contratación ni despliegue acreditados por añadir este archivo. Tarifas consultadas el 15/10/2026: https://render.com/pricing . El plan gratuito no admite el disco persistente necesario: https://render.com/docs/free .

El límite de peticiones simultáneas se reduce a una para limitar el uso de memoria al verificar ZIPs. Es una configuración inicial para validar con pocos usuarios; revisar latencia y capacidad antes de publicar para muchos clientes.

## Aplicación

1. En Render, crear un Blueprint desde `drp48-ua/wallet-studio-build`, usando la rama de trabajo hasta su revisión. No apuntar a `main` hasta integrar los cambios del colaborador. El fichero es `render.yaml` en la raíz.
2. Revisar coste y volumen. Introducir los secretos únicamente en el panel de Render: firma de webhooks Shopify y credenciales de envío SMTP con STARTTLS. `WALLET_AUTH_SECRET` lo genera Render. Conservarlo entre despliegues y guardar copia privada; no enviarlo por el chat ni incluirlo en GitHub.
3. El producto vinculado es el borrador **Wallet Studio**, ID `10459984298313`, de `store-09240616prrz2-itsdpkjq.myshopify.com`. El borrador no está publicado y su precio todavía requiere configuración del propietario. No concede acceso comprar otro producto de la tienda.
4. En Shopify configurar `orders/paid`, `orders/updated` y `orders/cancelled` contra `https://DOMINIO-ASIGNADO/v1/shopify/webhook`. El secreto debe pertenecer al método con el que se crearon esas suscripciones. No usar el secreto del conector de Codex ni copiar sus cookies/tokens.
5. Comprobar una compra sintética/de prueba del producto y recepción del código, límite de tres equipos y revocación por devolución. El estado `/health` solo comprueba que el proceso responde; no certifica entrega de correo, compras ni restauración de copias.
6. Reconciliar compras anteriores al alta del webhook mediante la API autenticada antes de ofrecer acceso a esos compradores. No existe todavía una importación automática histórica.
7. Con la URL real y pruebas completadas, configurar la variable GitHub `WALLET_STUDIO_CLOUD_URL`, compilar el cliente y probar el inicio de sesión con HTTPS real. Habilitar el canal público solo tras validar el conjunto. El binario de revisión sin URL queda cerrado por diseño.
8. Crear copias externas verificadas de SQLite mediante su API de backup y de los objetos privados. Mantener permisos y secretos fuera del repositorio. Los snapshots del disco no sustituyen una política de backup de la base de datos.

Los despliegues automáticos del backend están desactivados inicialmente. Actualizar el programa desde `main` utiliza el workflow firmado existente; los cambios de esquema del servicio deben revisarse separadamente para conservar sesiones, equipos y originales.

Referencia de Blueprint: https://render.com/docs/blueprint-spec . Persistencia y límites: https://render.com/docs/disks .
