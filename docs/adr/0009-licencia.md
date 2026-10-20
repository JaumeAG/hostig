# ADR 0009 — Licencia del proyecto y de terceros

- Estado: Aceptado. Licencia Apache-2.0.
- Fecha: 2026-10-20

## Contexto

BITXMP se quiere publicar como alternativa open source y gratuita a XAMPP. El instalador no debe redistribuir los binarios de Apache, PHP ni MariaDB ([ADR 0002](0002-gestor-de-runtimes.md)). Esos proyectos tienen licencias distintas, y una de ellas (MariaDB Server) es copyleft.

Hechos de licencia relevantes en la fecha de esta decisión:

| Pieza | Licencia habitual del upstream | Qué implica si nosotros publicamos el binario |
|---|---|---|
| Código de BITXMP (Rust, React, plantillas, catálogo) | Apache-2.0 | Es la licencia del repositorio. Está en `LICENSE` y el aviso en `NOTICE` |
| Apache HTTP Server y gran parte de su árbol | Apache-2.0 | Conservar avisos y `NOTICE` |
| PHP | Licencia PHP 3.01 | Redistribuir el binario exige incluir esa licencia. No es compatible con la GPL. El nombre PHP tiene restricciones de marca en la licencia |
| MariaDB Server | GPLv2 | Quien distribuye el binario debe acompañar el código fuente correspondiente o una oferta escrita de entregarlo |
| Apache Lounge (ZIP de Windows) | Binario de terceros construido con fuente ASF | No es un release de la ASF. Su checksum y su licencia viajan con el archivo descargado |
| Tauri y dependencias Rust/npm | Varias, casi todas permisivas | Hay que generar un inventario al publicar |

MIT es corta y conocida. No incluye una concesión explícita de patentes. Apache-2.0 sí, y es la licencia de Tauri y de httpd, así que el aviso combinado es más simple cuando el código propio y Apache se nombran juntos.

## Decisión

**Apache-2.0** para el código de BITXMP. El repositorio incluye `LICENSE` y un `NOTICE` que enumera los componentes de terceros que el usuario descarga.

Política de binarios:

1. El instalador y las releases de la app contienen el código nuestro compilado (app, CLI, supervisor, helper) y las plantillas. No contienen `httpd`, `php` ni `mariadbd`.
2. El catálogo apunta a la URL upstream o, en las fases que lo necesitan, a un artefacto nuestro construido a partir del tarball oficial verificado ([ADR 0002](0002-gestor-de-runtimes.md)). Ese artefacto lleva dentro `LICENSE` y `NOTICE` de cada componente, y para MariaDB se publica al lado el tarball de fuente exacto que se compiló (cumplimiento de GPLv2).
3. Al descomprimir un runtime, el supervisor comprueba que el paquete trae un archivo de licencia. Si no está, lo anota en `{runtime}/LICENSE.upstream` con el SPDX del catálogo y la URL del archivo descargado. No se reescribe la licencia del upstream.
4. La UI tiene una pantalla de licencias que lee esos archivos, incluida la de Apache Lounge cuando el runtime de Windows venga de allí.
5. No se usa el nombre «XAMPP» ni «Apache Friends» como nombre del producto. «Apache», «PHP» y «MariaDB» se usan como nombres de los componentes, con el aviso de que son marcas de sus titulares.
6. Las dependencias de Rust y npm se inventarían en cada release (`cargo deny` / un informe equivalente de npm) y el release falla si aparece una licencia copyleft dentro del binario de la app. El copyleft de MariaDB se queda en el runtime descargado, que es un proceso separado, no un enlace estático con el supervisor.

El fichero `LICENSE` del repositorio es la Apache License 2.0, y `NOTICE` acompaña al código.

## Consecuencias

- Apache-2.0 obliga a conservar avisos de modificaciones en los archivos que incorporemos de otros proyectos Apache-2.0. MIT no.
- Publicar builds de MariaDB hechos por nosotros es distribución GPL aunque el instalador de la app no los lleve dentro. El fuente exacto tiene que seguir disponible en la misma release.
- La licencia PHP 3.01 en un binario que nosotros compilemos hay que incluirla tal cual. No se «relicencia» PHP bajo Apache-2.0.
- Un contribuidor que envíe código lo hace bajo la licencia del repo. Conviene decirlo en el futuro `CONTRIBUTING`, no en este ADR.

## Alternativas descartadas

- GPL para BITXMP, para «coincidir» con MariaDB. Arrastra la app entera a copyleft sin necesidad: el servidor de base de datos ya va en su propio proceso y con su propia licencia.
- Redistribuir los ZIP upstream dentro del instalador «por comodidad del primer arranque». Rompe la política pedida y mezcla titulares en un solo archivo.

## Confirmado

1. La licencia del código es Apache-2.0, no MIT.
2. Los binarios compilados por el CI a partir del fuente oficial se publican en una release distinta del instalador, por fases, como dice el [ADR 0002](0002-gestor-de-runtimes.md).
