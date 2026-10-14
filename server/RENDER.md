# Despliegue de cuentas en Render

El archivo `render.yaml` prepara una instancia Docker **Free** en Frankfurt. La API es efímera; las cuentas residen en PostgreSQL de Supabase y los paquetes originales en un bucket privado de Supabase Storage. `WALLET_STORAGE_MODE=remote` exige ambos servicios: nunca reemplazar una base de datos o copia remota ausente por archivos efímeros del servidor.

## Coste y estado

El propietario ha rechazado servicios de pago. No contratar disco Render, plan de pago, IPv4 adicional, dominio de API de pago ni mejoras automáticas. Crear los tres servicios en sus planes gratuitos: Render, Supabase y Brevo. Añadir este archivo no acredita un despliegue online ni la entrega de correos.

Límites consultados el 15/10/2026: Render Free ofrece 750 horas por workspace/mes y se detiene tras 15 minutos sin peticiones; el siguiente arranque puede tardar. Supabase Free incluye 500 MB de base de datos y 1 GB de archivos, con cuotas de transferencia y posibles pausas por inactividad. Brevo Free ofrece 300 correos diarios. Revisar cuotas, políticas y disponibilidad antes de vender acceso masivo. Fuentes: https://render.com/docs/free , https://supabase.com/docs/guides/platform/billing-on-supabase y https://help.brevo.com/hc/en-us/articles/208589409-About-Brevo-s-pricing-plans .

Los paquetes remotos se limitan a 32 MiB; un paquete mayor se rechaza sin borrar ni sustituir los originales locales. Superar las cuotas gratuitas exige reducir consumo o una decisión posterior del propietario; no habilitar pagos para resolverlo automáticamente.

El límite de peticiones simultáneas se reduce a una para limitar el uso de memoria al verificar ZIPs. Es una configuración inicial para validar con pocos usuarios; revisar latencia y capacidad antes de publicar para muchos clientes.

## Aplicación

1. En Render, crear un Blueprint desde `drp48-ua/wallet-studio-build`, usando la rama de trabajo hasta su revisión. No apuntar a `main` hasta integrar los cambios del colaborador. El fichero es `render.yaml` en la raíz.
2. Crear un proyecto Supabase Free en Europa. Introducir sus secretos únicamente en Render: `DATABASE_URL` del transaction pooler IPv4 (puerto 6543), `SUPABASE_URL`, `SUPABASE_SERVICE_ROLE_KEY`. El backend usa el esquema privado `wallet_private`, conexión TLS con verificación completa y un bucket privado `wallet-private`. Las credenciales de servicio nunca se compilan en el cliente. No usar la clave pública anon como clave del servidor.
3. Registrar Brevo Free, verificar el remitente y configurar `BREVO_API_KEY`, `BREVO_SENDER_EMAIL` y `BREVO_SENDER_NAME`. Se usa HTTPS: Render Free bloquea los puertos SMTP 25/465/587. Configurar también la firma de webhooks Shopify. `WALLET_AUTH_SECRET` lo genera Render; conservarlo entre despliegues y guardar copia privada. No introducir secretos en el chat ni en GitHub.
3. El producto vinculado es el borrador **Wallet Studio**, ID `10459984298313`, de `store-09240616prrz2-itsdpkjq.myshopify.com`. El borrador no está publicado y su precio todavía requiere configuración del propietario. No concede acceso comprar otro producto de la tienda.
4. En Shopify configurar `orders/paid`, `orders/updated` y `orders/cancelled` contra `https://DOMINIO-ASIGNADO/v1/shopify/webhook`. El secreto debe pertenecer al método con el que se crearon esas suscripciones. No usar el secreto del conector de Codex ni copiar sus cookies/tokens.
5. Comprobar una compra sintética/de prueba del producto y recepción del código, límite de tres equipos y revocación por devolución. El estado `/health` solo comprueba que el proceso responde; no certifica entrega de correo, compras ni restauración de copias.
6. Reconciliar compras anteriores al alta del webhook mediante la API autenticada antes de ofrecer acceso a esos compradores. No existe todavía una importación automática histórica.
7. Con la URL real y pruebas completadas, configurar la variable GitHub `WALLET_STUDIO_CLOUD_URL`, compilar el cliente y probar el inicio de sesión con HTTPS real. Habilitar el canal público solo tras validar el conjunto. El binario de revisión sin URL queda cerrado por diseño.
8. Crear copias externas privadas y verificadas de PostgreSQL y de los objetos. El plan Free no sustituye una política de backup; las copias bancarias nunca van a GitHub. SQLite sigue disponible para pruebas locales y despliegues con disco persistente administrado, no como almacenamiento de esta configuración gratuita.

Los despliegues automáticos del backend están desactivados inicialmente. Actualizar el programa desde `main` utiliza el workflow firmado existente; los cambios de esquema del servicio deben revisarse separadamente para conservar sesiones, equipos y originales.

Referencia de Blueprint: https://render.com/docs/blueprint-spec . Persistencia y límites: https://render.com/docs/disks .
