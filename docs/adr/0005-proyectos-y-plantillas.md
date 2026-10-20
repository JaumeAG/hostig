# ADR 0005 — Proyectos y configuración generada

- Estado: Aceptado. MariaDB compartida por versión mayor, con un esquema por proyecto.
- Fecha: 2026-10-20

## Contexto

Cada proyecto debe poder usar su versión de PHP, su base de datos, un dominio `.test` y SSL local. Apache y PHP no se configuran a mano: un edit suelto en `httpd.conf` fue, en XAMPP, otro archivo más que se desalinea de `properties.ini` y del `my.ini` que vive dentro de `data`.

Varias versiones de PHP a la vez y un solo Apache encajan con FastCGI. El módulo `mod_php` carga una sola versión dentro del proceso del servidor. En Windows, el ZIP oficial NTS es el build pensado para FastCGI; el TS es el del módulo.

Un proceso MariaDB por proyecto aísla mejor y gasta más RAM. Una sola instancia con un esquema por proyecto es más eficiente y basta mientras la versión mayor del servidor sea la misma.

## Decisión

Un proyecto es un manifiesto, no una copia del stack. El código del sitio está en la carpeta que la persona elija. El manifiesto, la config generada y el esquema viven bajo la carpeta de datos.

```text
{datos}/projects/{slug}/project.toml
{datos}/projects/{slug}/generated/   # solo salida del generador
```

`project.toml` declara, como mínimo: nombre, slug, ruta del código, versión de PHP, nombre de la base, versión mayor de MariaDB, dominio, si quiere HTTPS, y puertos si no valen los de por defecto.

### Un Apache, pools de PHP, MariaDB según versión mayor

Queda confirmado:

- Un proceso Apache para todos los proyectos.
- Cada proyecto tiene su pool PHP en la versión pedida. En macOS y Linux es `php-fpm` con un socket Unix en `{datos}/projects/{slug}/generated/php.sock`.
- En Windows el ZIP oficial no trae `php-fpm`. Un solo `php-cgi.exe -b` atiende una petición cada vez. El mecanismo es un pool de N procesos `php-cgi.exe -b 127.0.0.1:{puerto}`, con N = 4 por defecto, configurable en el manifiesto. Apache reparte con `mod_proxy_fcgi` y `mod_proxy_balancer`. Así hay N peticiones concurrentes.
- `PHP_FCGI_MAX_REQUESTS` es la variable de entorno con la que `php-cgi` se recicla. El pool la exporta con un valor explícito, 500 por defecto del producto, para no depender del valor que traiga el binario. Al llegar a ese número el proceso sale con código 0. El supervisor arranca el reemplazo y no lo cuenta como caída para el backoff del [ADR 0003](0003-supervision-de-procesos.md). Una salida distinta de 0, una señal, o una muerte antes de ese cupo, sí es caída y sí gasta el backoff. `FcgidMaxRequestsPerProcess` sería el equivalente si el pool lo gestionara `mod_fcgid`; no es el mecanismo de esta decisión.
- `mod_fcgid` queda como alternativa posterior, solo si su módulo entra en el catálogo con checksum. No es la vía del MVP: el reciclado ocurriría dentro de Apache y el supervisor no podría separar, con la misma prueba, un reciclado de una caída.
- Una instancia MariaDB por versión mayor en uso (por ejemplo una 10.11 y una 11.4), no una por proyecto. Cada proyecto recibe un esquema y un usuario limitados a ese esquema, en la instancia de la versión que pidió. Compartir la base queda confirmado.
- Si dos proyectos piden la misma versión mayor, comparten el proceso y no el esquema.

Así el disco puede guardar muchas versiones ([ADR 0002](0002-gestor-de-runtimes.md)) y la RAM solo paga los servidores que están en marcha.

### Plantillas

`templates/apache/` y `templates/php/` son la única fuente de los archivos de configuración. El generador escribe `generated/` con una cabecera fija que indica que el archivo se regenera y que las ediciones se pierden. El supervisor no lee un `httpd.conf` que alguien haya retocado fuera de ese directorio: antes de arrancar, vuelve a generar y compara. Si el archivo en disco no coincide con la salida, lo sustituye.

Los ajustes del usuario entran por `project.toml` (o por un manifiesto global del stack), nunca pegando directivas en el conf generado. Si hace falta una directiva que el manifiesto no conoce, se amplía el manifiesto y la plantilla en el producto, con un test del texto generado.

Apache incluye un vhost por proyecto. El document root es la ruta del código del manifiesto. PHP llega por `mod_proxy_fcgi` al socket o al puerto de ese pool. El health check del [ADR 0003](0003-supervision-de-procesos.md) es un location aparte, no un archivo dentro del código del usuario.

### Dominio `.test` y SSL

El nombre preferido es `{slug}.test`. Resolverlo exige una línea en el archivo hosts o un DNS local, y ambas cosas pasan por el helper del [ADR 0006](0006-privilegios.md).

Sin helper, el mismo proyecto responde en `{slug}.localhost` y en un puerto alto. Los navegadores resuelven `*.localhost` a loopback (RFC 6761) sin tocar el archivo hosts. `.test` también está reservado, y los navegadores no lo envían a loopback solos: hace falta hosts o DNS. Por eso el modo sin privilegios no promete `.test`.

SSL local: el generador crea una CA en `{datos}/ca/` y un certificado por dominio, con SAN para el `.test` y el `.localhost`. La clave de la CA no sale de esa carpeta (`0600`). Confiar la CA en el almacén del sistema es trabajo del helper. Sin esa confianza, el modo sin privilegios sirve HTTP en el puerto alto y deja el HTTPS apagado para ese proyecto, con la causa a la vista.

### MVP

El MVP de la arquitectura arranca un solo Apache, un solo PHP y una sola MariaDB, con un vhost de prueba generado. El manifiesto de proyectos y el `.test` llegan en la fase 2. Las plantillas existen desde el MVP para que ese vhost único ya salga del generador y no de un conf escrito a mano.

## Verificación de la Fase 1

Con el ZIP oficial NTS de Windows, antes de cerrar la fase 1:

- Al llegar a `PHP_FCGI_MAX_REQUESTS` el `php-cgi` sale con código 0. Si ese build sale con otro código, el reciclado no cuenta como éxito y hay que revisar el backoff del [ADR 0003](0003-supervision-de-procesos.md).
- Durante ese reciclado, `mod_proxy_balancer` no deja el miembro marcado como caído más de un segundo. Cada miembro lleva `retry=1`. En `mod_proxy`, `retry` es el tiempo en segundos que Apache espera antes de volver a usar un worker en error (<https://httpd.apache.org/docs/2.4/mod/mod_proxy.html#balancermember>). La prueba pide las suficientes peticiones para cruzar el límite y comprueba que el miembro vuelve a recibir tráfico dentro de ese segundo.

## Consecuencias

- Cambiar la versión de PHP de un proyecto regenera su pool y recarga Apache. No reinstala el runtime si ya está en disco.
- Compartir MariaDB entre proyectos exige cuentas separadas. Un fallo de `DROP DATABASE` debe acotarse al esquema del proyecto.
- Dos versiones mayores de MariaDB significan dos puertos y dos datadirs. El módulo de puertos los trata como instancias distintas.
- El modo `.localhost` puede chocar con otro software que también use ese nombre. El slug incluye un sufijo si `check` de nombre lo detecta en la config generada.

## Alternativas descartadas

- `mod_php`. Una versión de PHP por proceso Apache.
- Un datadir MariaDB por proyecto como defecto. Aísla más y multiplica recuperación, backups y RAM. Queda disponible el día en que un proyecto pida otra versión mayor, que ya es otro proceso.
- Editar `httpd.conf` «solo esta vez». Es el comienzo del árbol de XAMPP.

## Confirmado

Una MariaDB por versión mayor, con un esquema y un usuario por proyecto. Un servidor entero por proyecto no es el defecto: aísla más y multiplica recuperación, backups y RAM. Sigue existiendo un proceso distinto cuando la versión mayor es otra.
