# Análisis de la instalación XAMPP en `C:\xampp_8_2`

Fecha de inspección: 2026-10-20. Alcance: la instalación de escritorio pedida, con el panel `xampp-control.exe` como artefacto principal. No hay código de producto en este informe.

## Cómo se obtuvo la evidencia

| Tipo | Qué es | Qué no demuestra |
|---|---|---|
| Evidencia REA | Salida de `rea-agents` 6.7.0 por CLI. Identidad del PE, clasificación managed y árbol de recursos. | No es ejecución del programa. |
| Observación de shell | Lectura de configuración, registros, tabla de imports del PE y cadenas UTF-16 del mismo archivo que REA identificó. | Una cadena o un import no prueban el orden de ejecución. |
| Inferencia | Relación entre piezas observadas cuando el flujo no se decompiló. | Queda marcada como inferencia. |
| No resuelto | Pregunta que sigue abierta. | No se rellena con el código fuente público del panel. |

REA en esta sesión:

- `rea doctor --client cursor` clasificó el registro MCP de Cursor como ausente (`C:\Users\Usuario\.cursor\mcp.json`, estado `missing`). No se escribió esa configuración: el plan en seco solo añadiría el servidor MCP y no instala un desensamblador.
- Ghidra, Hopper, IDA y Kuna no están disponibles. `rea search` sobre `xampp-control.exe` terminó en `provider_unavailable`. Hopper, además, no admite este host Windows.
- Sí se ejecutaron `inspect-artifact`, `inspect-managed-artifact` e `inspect-pe-resources`. La decompilación del flujo de arranque y parada queda sin hacer.

Evidencia REA de `C:\xampp_8_2\xampp-control.exe`:

- SHA-256 `ed4d6fdb6248bcff64e5652cd0c9d79c483bace94c1120dc3128645f00a5e5c4`, 3 368 448 bytes.
- `inspect_artifact`: `ev_c1a0d0a46690f977459f0008af2b991aee6e98b96ce31d6e02762e3efb597b44`. Inventario: `ev_7b8074a494b6c4c5643d3cf113f942bf746db8637154aa3647cef285ebe57548`. Un solo nodo PE, sin contenedor hijo.
- `inspect_managed_artifact`: `ev_ce3ff0684d46c79d54dc219671929d4fb8c37170aabd2b47dd52b83f61c6e61a`. PE32, máquina `0x014c` (x86), sin directorio CLI. Clasificación `not-managed`.
- `inspect_pe_resources`: `ev_af7d5125ca5a894db5bce196f37a63f2edf0a3e1d8c3220ea4398e02b82d5259`. 102 recursos, cobertura completa dentro del límite del parser. Los payloads que no son grupos de iconos quedan opacos para REA; las cadenas citadas más abajo salen de un lector local sobre esos rangos y sobre el archivo, no de un descodificador REA.

Copias de trabajo en `docs/research/evidence/`. No se arrancó ni se detuvo ningún servicio para esta inspección.

## 1. Inventario

### Confirmado: identidad del paquete

`properties.ini` declara `base_stack_name=XAMPP`, `base_stack_version=8.2.0-0`, `base_stack_platform=windows-x64`, `installdir=C:\xampp_8_2`. `readme_en.txt` repite «ApacheFriends XAMPP Version 8.2.0» y la misma lista de componentes. Las versiones de binario se contrastaron aparte; donde el readme no se ejecutó, se marca.

| Componente | Versión confirmada | Dónde |
|---|---|---|
| Panel de control | 3.3.0.0. El propio EXE contiene la cadena `Apr 6th 2021` junto a `Control Panel Version:` | `xampp-control.exe`, versión de archivo y cadenas |
| Apache | 2.4.54 Win64, construido el 22 Jun 2022 (Apache Lounge VS16) | `apache\bin\httpd.exe -v`. El EXE pesa 30 720 bytes; el servidor real está en las DLL de `apache\bin` |
| MariaDB | 10.4.27 Win64, «mariadb.org binary distribution», revisión `0946c99e7d6f7ac9dfcf3e60dae6ae85161d5ef2` | `mysql\bin\mysqld.exe --version` y `mysql.exe --version`. La carpeta sigue llamándose `mysql` |
| PHP | 8.2.0 CLI, ZTS, Visual C++ 2019 x64, construido el 6 Dic 2022. Zend Engine 4.2.0 | `php\php.exe -v` |
| phpMyAdmin | 5.2.0 | `phpMyAdmin\README` y `RELEASE-DATE-5.2.0` |
| Tomcat | 8.5.79 | `tomcat\RELEASE-NOTES` |
| Perl | 5.32.1 MSWin32-x64 | `perl\bin\perl.exe -v` |
| OpenSSL 1.1.1p, ADOdb 518a, Webalizer 2.23-04, mailToDisk 1.0 | Solo el texto de `readme_en.txt` | No se contrastó el binario |
| FileZilla y Mercury | El readme dice que no van en la versión portable. En este disco sí están `FileZillaFTP\FileZilla Server.exe` y `MercuryMail\mercury.exe` | Existencia confirmada; versión de archivo no extraída |
| `xampp_start.exe` / `xampp_stop.exe` | Versión de archivo 1.0, producto `XAMPPcli`, 118 784 bytes cada uno, hashes SHA-256 distintos | No se abrieron con REA |
| `service.exe` | 60 928 bytes, sin versión de archivo | No se abrió con REA |

El panel es de 32 bits y los servidores son de 64 bits. El EXE importa en retardo `IsWow64Process`, así que el binario contempla esa diferencia; no se observó cómo la usa.

### Confirmado: el panel es una aplicación Delphi nativa

Recursos PE nombrados: `DVCLAL`, `PACKAGEINFO`, `TFCONFIG`, `TFMAIN` (102 337 bytes), `TFNETSTAT`, `TFSERVICESETTINGS`, `TFLOGOPTIONS`, `TFLANGUAGE`, `TFHELP`, más tablas `UNICODEDATA` y tablas de cadenas RT_STRING. La tabla de tipos de Delphi incluye `System.IniFiles`. Hay rutas de compilación embebidas, entre ellas `C:\xampp\src\xampp-control-panel\gnugettext.pas` y fuentes JCL 2.7. Esas rutas son del equipo que compiló el EXE, no de esta máquina.

`inspect_artifact` dejó como hipótesis de baja confianza que el PE podría ser managed o nativo. `inspect_managed_artifact` lo cerró: no hay metadatos CLI.

### Estructura de carpetas

Raíz observada:

```text
C:\xampp_8_2\
  apache\  cgi-bin\  contrib\  FileZillaFTP\  htdocs\
  install\ licenses\ locale\   mailoutput\    mailtodisk\
  MercuryMail\ mysql\ perl\ php\ phpMyAdmin\
  sendmail\ src\ tmp\ tomcat\ webalizer\ webdav\
  anonymous\ img\
  apache_start.bat  apache_stop.bat
  mysql_start.bat   mysql_stop.bat
  killprocess.bat   ctlscript.bat  setup_xampp.bat
  xampp-control.exe xampp-control.ini xampp-control.log
  xampp_start.exe   xampp_stop.exe  service.exe
  properties.ini    readme_en.txt   readme_de.txt
  passwords.txt     uninstall.exe   uninstall.dat
```

Piezas que importan para arranque y datos:

- `apache\bin`, `apache\conf` (`httpd.conf`, `extra\httpd-ssl.conf`), `apache\logs`.
- `mysql\bin\my.ini` es la configuración que usan los scripts de arranque. `mysql\data` es el directorio de datos, ya poblado: 332 archivos `.ibd` (InnoDB), 24 pares `.MAI`/`.MAD` (Aria), 359 `.frm`, `ibdata1` (79 691 776 bytes), `ib_logfile0` y `ib_logfile1` (5 242 880 bytes cada uno, igual que `innodb_log_file_size=5M`), `aria_log_control`, `mysql.pid`, `mysql_error.log` y `mysqld.dmp` (168 960 bytes, 2026-10-20 11:12:10). Hay bases de aplicación además de `mysql`, `performance_schema`, `phpmyadmin` y `test`. Este informe no lista sus nombres.
- `php\`, `php\ext`. `properties.ini` apunta `php_binary_directory` y `php_extensions_directory` ahí.

### Confirmado: puertos declarados, y no coinciden con un único archivo

| Origen | Apache | MySQL / MariaDB |
|---|---|---|
| `properties.ini` | 80 y 443 | 3306, host `localhost` |
| `apache\conf\httpd.conf` | `Listen 80`, `ServerRoot "C:/xampp_8_2/apache"`, `DocumentRoot "C:/xampp_8_2/htdocs"` | |
| `apache\conf\extra\httpd-ssl.conf` | `Listen 443` | |
| `mysql\bin\my.ini` | | `port=3306` en cliente y servidor, `datadir="C:/xampp_8_2/mysql/data"`, `bind-address` comentado |
| `mysql\data\my.ini` | | Solo `datadir=c:/xampp/mysql/data`. Esa ruta no es esta instalación |

En el momento de la inspección no había un listener TCP en 80, 443 ni 3306. El único `mysqld.exe` vivo pertenecía a otra instalación: `C:\xampp_7_4\mysql\bin\mysqld.exe --defaults-file=c:\xampp_7_4\mysql\bin\my.ini --standalone`, escuchando en `0.0.0.0:3307`. Había un `xampp-control.exe` de `C:\xampp_8_2` y dos de `C:\xampp_7_4`. No había servicios de Windows cuyo nombre coincidiera con apache, mysql, maria, filezilla, tomcat o xampp. `properties.ini` deja vacíos `apache_unique_service_name` y `mysql_unique_service_name`.

## 2. Panel de control

### Qué hace al abrirse

Confirmado por cadenas del EXE, en el mismo orden en que aparecen en el binario alrededor del offset 2 604 716, y por el registro `xampp-control.log` (última escritura 2026-02-03 18:27:55; el archivo no lleva fecha en cada línea):

1. `Initializing Control Panel`
2. `Windows Version: %s %s %s` y un aviso si el sistema es antiguo.
3. `XAMPP Version:` y `Control Panel Version:` con `Compiled: %s`.
4. Comprueba derechos de administrador. El texto distingue `Running with Administrator rights - good!` y un aviso de que, sin ellos, los servicios mostrarán un diálogo de seguridad o fallarán. El registro de febrero dice que aquella ejecución no tenía derechos de administrador. Imports estáticos relacionados: `OpenProcessToken`, `GetTokenInformation`.
5. `XAMPP Installation Directory: "%s"`. El registro de febrero guardó `c:\xampp_8_2\`.
6. Avisa si la ruta tiene espacios o caracteres especiales.
7. `Checking for prerequisites`. La cadena de requisito es `x86_microsoft.vc90.crt` y el mensaje habla del runtime de Visual C++ 2008. PHP 8.2 de esta instalación está construido con Visual C++ 2019. El registro de febrero dice `All prerequisites found`.
8. `Initializing Modules`, `Starting Check-Timer`, `Control Panel Ready`.
9. Al salir: `Deinitializing Modules`, `Deinitializing Control Panel`.

Inferencia: el temporizador de comprobación es el que vuelve a mirar procesos y puertos. La cadena `Check-Timer` y el registro coinciden; el intervalo no está en una cadena y no se midió.

### Dónde guarda la configuración

Confirmado, archivo `xampp-control.ini` junto al EXE:

```ini
[Common]
Edition=
Editor=notepad.exe
Browser=
Debug=0
Debuglevel=0
TomcatVisible=1
Language=en
[EnableModules]
Apache=1
MySQL=1
FileZilla=1
Mercury=1
Tomcat=1
```

Esas claves existen como texto UTF-16 dentro del EXE (`Editor`, `Debuglevel`, `TomcatVisible`, `Language`, y también `Autostart`, `WindowSettings`, `EnableModules`, `EnableServices`, `CheckDefaultPorts`, `UserConfigs`, `UserLogs`, `ServiceNames`, `ServicePorts`, `BinaryNames`). El literal `xampp-control.ini` no está en el EXE. `System.IniFiles` sí está en la información de tipos.

Inferencia: el panel construye el nombre del INI a partir del nombre del ejecutable. Encaja con el archivo que hay al lado y con las claves embebidas. No se decompiló `TIniFile`.

El literal `xampp-control.log` tampoco está. Los mensajes del registro coinciden, carácter a carácter, con cadenas del EXE. Inferencia: el registro se nombra igual, cambiando la extensión.

`properties.ini` no aparece como cadena en el panel. Los puertos que el panel conoce salen de otros archivos. Cadenas confirmadas:

- `apache/conf/httpd.conf`, `apache/conf/extra/httpd-ssl.conf`, `apache/conf/extra/httpd-xampp.conf`
- `mysql\bin\my.ini`
- Nombres de servicio embebidos: `Apache2.4`, `mysql`, `FileZillaServer`
- Binarios embebidos: `httpd.exe`, `mysqld.exe`, `filezillaserver.exe`, `mercury.exe`, `tomcat8.exe`

`ctlscript.bat` no lo usa el panel. Busca `mysql\scripts\ctl.bat` y `apache\scripts\ctl.bat`, que no existen en esta instalación, y también rutas de productos que no están (PostgreSQL, Ingres, Jetty).

### Servicios: detectar, arrancar, parar

Imports estáticos de `advapi32.dll` presentes en la tabla de importación: `OpenSCManagerW`, `OpenServiceW`, `QueryServiceStatus`, `QueryServiceStatusEx`, `QueryServiceConfigW`, `EnumServicesStatusW`, `CloseServiceHandle`.

No están, ni como import ni como cadena, `StartService`, `ControlService`, `CreateService` ni `DeleteService`. La tabla de imports en retardo tampoco los añade.

Cadenas del grupo de cada módulo, incluidas las de MySQL (offsets de archivo aproximados 2 309 248–2 310 996):

- `Checking for service (name="%s"): %s`
- `Service installed` / `Service not installed`
- `Service Path: %s`
- `Expected Path: %smysql\bin\%s --defaults-file=%smysql\bin\my.ini %s`
- Para Apache: `Expected Path: "%sapache\bin\%s" -k runservice`
- `%s Service detected with wrong path`
- `Attempting to start %s service...` seguido de `start "%s"`, `Executing "%s"`, `net `, `Return code: %d`
- `Attempting to stop %s service...` seguido de `stop "%s"`, `Executing "%s"`, `net `

Inferencia, no decompilada: el modo servicio consulta el SCM con las APIs importadas y arranca o para pidiendo a `net` el `start` o el `stop` del nombre de servicio. Los fragmentos están contiguos; la línea de comandos exacta no se reconstruyó instrucción a instrucción.

En esta instalación ese modo no es el que está en uso: no hay servicio registrado y los nombres únicos de `properties.ini` están vacíos.

### Modo aplicación: procesos, no servicios

Cadenas confirmadas para el arranque como aplicación:

- Apache: `Attempting to start %s app...` y `apache\bin\`
- MySQL: `Attempting to start %s app...`, `mysql\bin\`, `" --defaults-file="`, `mysql\bin\my.ini" --standalone`

Eso coincide con `mysql_start.bat`, que ejecuta:

```text
mysql\bin\mysqld --defaults-file=mysql\bin\my.ini --standalone
```

`CreateProcessW` y `ShellExecuteW` / `ShellExecuteExW` están importados. Inferencia: el modo aplicación crea el proceso del binario con esa línea. No se vio la llamada.

La parada como aplicación no es la misma cadena para los dos módulos.

Apache, offsets 2 296 612–2 296 868:

- `Attempting to stop`
- `(PID: %d)`
- `Problem killing PID %d`
- `Check that you have the proper privileges`
- `No PIDs found?!`

MySQL, offsets 2 310 996–2 311 068:

- `Attempting to stop %s app...`
- y, inmediatamente después en el archivo, `No PIDs found?!`

El registro del 3 de febrero usa las dos plantillas: `Attempting to stop Apache (PID: 30212)` y otro PID, y `Attempting to stop MySQL app...`. Un segundo después el registro dice `Status change detected: stopped` para los dos. Ese registro no dice con qué API murió el proceso.

`TerminateProcess` está importado desde `kernel32.dll`. No hay cadena ni import de `GenerateConsoleCtrlEvent`, `AttachConsole`, `mysqladmin`, `mysqladmin.exe`, `taskkill`, `mysql_stop.bat` ni `apache_stop.bat`.

Inferencia: la parada de Apache en modo aplicación mata PIDs que el panel tiene anotados (`Creating PID-entry %d: %s`, `Deleting PID-entry %d: %s`, símbolos `lApachePIDs` y `lMySQLPIDs`). La parada de MySQL en modo aplicación no deja en su propio bloque de cadenas un comando de apagado ordenado. Puede compartir la rutina que mata PIDs, o puede limitarse a mirar si hay PIDs. Sin decompilación las dos lecturas siguen abiertas. Lo que sí queda cerrado es que el EXE no contiene `mysqladmin`.

### Cómo mira los puertos

Confirmado:

- Carga dinámica: cadenas `iphlpapi.dll` y `GetExtendedTcpTable` (offset 2 268 104). Si falla: `Problem loading IP Library`.
- Tipos y símbolos: `MIB_TCPTABLE_OWNER_PID`, `PMIB_TCPTABLE_OWNER_PID`, `GetPorts4PID`, `GetPortCount4PID`, `isPortInUsePID`, `NetStatTable`.
- Mensajes: `Checking default ports...`, `Port %d in use by "%s" with PID %d!`, `XAMPP %s is already running on port %d`, `%s WILL NOT start without the configured ports free!`, `Updating Networking Table...`.
- La palabra `netstat` está entre `New listening socket: %s:%d` y `Listening socket closed: %s:%d`. No aparece `netstat.exe`.

Inferencia: el temporizador lee la tabla TCP de Windows con el PID dueño, y `netstat` es la etiqueta de ese subsistema dentro del panel. No se observó una llamada real a `GetExtendedTcpTable` ni el código de filtro (IPv4/IPv6, listening frente a established).

`GetTcpTable` no está. El panel de 32 bits pregunta por servidores de 64 bits; no se comprobó si alguna fila de la tabla le sale con PID 0 por WOW64.

## 3. Cierre de MariaDB y corrupción

Hay tres caminos distintos. Solo uno es un apagado que MariaDB entiende como tal.

### Camino A — script `mysql_stop.bat`

Texto confirmado del script:

```bat
cmd.exe /C start "" /MIN call "@@BITROCK_INSTALLDIR@@\killprocess.bat" "mysqld.exe"
if not exist mysql\data\%computername%.pid GOTO exit
del mysql\data\%computername%.pid
```

`killprocess.bat` hace `taskkill.exe /F` de todos los PID que `tasklist` asocie al nombre de imagen recibido.

Confirmado, y encadenado:

- `@@BITROCK_INSTALLDIR@@` sigue sin sustituir. `setup_xampp.bat` llama a `install\install.php` y no contiene ese token. `apache_stop.bat` tiene el mismo token. El `start` no espera (`/WAIT` no está).
- Aunque la ruta se arreglara, `taskkill /F` termina el proceso sin pasar por el manejador de apagado de MariaDB. El criterio es el nombre `mysqld.exe`, así que también alcanzaría al `mysqld` de `C:\xampp_7_4` que estaba vivo durante la inspección.
- El PID que borra es `%COMPUTERNAME%.pid`. El archivo que hay en datos es `mysql.pid`.
- El panel no contiene la cadena `mysql_stop`. Este script no es el camino del botón del panel.

### Camino B — modo servicio del panel

Si el servicio existiera, las cadenas apuntan a `net` + `stop "%s"`. El servicio de MariaDB para Windows ejecuta su propio apagado cuando el SCM le pide parar. Inferencia a partir de esas cadenas y de la ausencia de `ControlService` en imports. En este disco el servicio no está instalado, así que este camino no es el que ha estado ocurriendo.

### Camino C — modo aplicación del panel, que es el que coincide con los scripts y con el registro

Arranque confirmado por cadenas y por `mysql_start.bat`: `mysqld --defaults-file=mysql\bin\my.ini --standalone`.

Parada: no hay `mysqladmin shutdown` en el EXE. La plantilla de log del modo aplicación es `Attempting to stop %s app...`. `TerminateProcess` sí está importado. La semántica de un `TerminateProcess` sobre `mysqld` es la de una caída: el proceso no ejecuta `innodb_fast_shutdown`.

### Qué haría un apagado limpio en este binario

`mysqld.exe --no-defaults --verbose --help` de esta misma instalación (salida guardada en `docs/research/evidence/mysqld-help.txt`):

- `innodb-fast-shutdown` por defecto `1`. El texto de ayuda dice que acelera el cierre y que los valores posibles son 0, 1 (más rápido), 2 (parecido a una caída) y 3 (el cierre limpio más rápido). `my.ini` no define esta variable, así que un apagado iniciado por el propio servidor usaría 1.
- `innodb-flush-log-at-trx-commit` por defecto `1`, y `my.ini` lo fija también en `1`: vuelca el redo en cada commit. La ayuda dice que 1 y 3 garantizan que, tras una caída, las transacciones confirmadas no se pierden y quedan alineadas con el binlog y con los otros motores transaccionales. 0 no da esa garantía. 2 puede perderlas si falla la alimentación o el núcleo.
- `innodb-doublewrite` por defecto `TRUE`. `my.ini` no lo desactiva.
- `innodb-flush-method` por defecto `unbuffered`.

Esas tres defensas cubren una caída en la que el redo y el tablespace siguen siendo la misma pareja de archivos. No convierten `taskkill /F` o `TerminateProcess` en un cierre limpio: el siguiente arranque tiene que hacer recuperación. Con el valor 1 de `innodb_fast_shutdown`, un cierre que sí llega a completar también deja trabajo de purga para el arranque siguiente; el texto de ayuda no detalla más que «más rápido».

Aria es otro dominio. Hay 24 tablas Aria y `aria_log_control`. `my.ini` no define recuperación Aria. No se buscaron errores Aria en el registro, así que su estado queda sin resolver.

### Lo que el registro de esta instalación ya muestra

`mysql\data\mysql_error.log` no contiene, entre los arranques recientes, una línea `Shutdown complete` ni `ready for connections` después del 15 de septiembre de 2026. Los últimos «ready for connections» son del 10 y del 15 de septiembre.

Patrón repetido el 19 y el 20 de octubre de 2026: el proceso escribe `Server socket created on IP: '::'.` y la siguiente entrada ya es otro arranque con `Starting crash recovery from checkpoint`. No hay línea de apagado en medio. El 20 de octubre hubo arranques a las 11:12:07, 11:12:14 y 11:12:32. `mysqld.dmp` se escribió a las 11:12:10, durante el primero. El archivo de log no creció después de las 11:12:32. A la hora de la inspección no había un `mysqld.exe` de `C:\xampp_8_2`.

En esos arranques InnoDB deja el mismo diagnóstico. Cita del 20 de octubre, 11:12:07:

- Checkpoint y redo alrededor de LSN 231 416 435.
- Cabecera del tablespace de sistema en LSN 257 372 914.
- Páginas con LSN «en el futuro» (por ejemplo la página 395 del espacio 0, LSN 250 746 461).
- Texto del servidor: la base puede estar corrupta, o el tablespace se copió sin sus archivos de log.

El mismo LSN de página 250 746 461 ya aparece el 19 de octubre a las 15:32 y a las 17:19, mientras el redo sigue cerca de 231 millones. La recuperación no cierra esa brecha: el servidor aun así anota `InnoDB: 10.4.27 started` y crea el socket. `innodb_flush_log_at_trx_commit=1` no impide este síntoma. El síntoma es redo por detrás del tablespace, que es la situación que el propio mensaje atribuye a una pareja de archivos que ya no corresponde, o a corrupción.

`mysql.pid` contiene `38840` y su fecha es 2026-10-20 11:12:32. Ese PID no era un proceso vivo. `apache\logs\httpd.pid` contiene `32248`, fecha 2026-10-19 15:32:48, y `httpd.exe` no estaba en ejecución. Los dos archivos de PID estaban atrasados.

Inferencia: los cortes entre «socket creado» y el arranque siguiente, sin línea de apagado y con `Starting crash recovery`, son cierres bruscos o caídas. El volcado de las 11:12:10 apoya que al menos un arranque de hoy no fue limpio. La causa concreta de la brecha de LSN (copia de `ibdata1` sin sus redo, recreación de los logs de 5 MB, o una escritura a medias) no queda distinguida: el servidor enumera las dos primeras y el disco no aporta una tercera prueba.

Riesgo que sí se puede afirmar: un gestor que mate `mysqld` por nombre de imagen, o que dé por bueno el PID file, opera sobre un directorio de datos que ya arranca en recuperación y con páginas por delante del redo. El buffer pool está en 16 MB y cada redo en 5 MB (`innodb_log_buffer_size=8M` es mayor que el archivo de log). Eso hace la recuperación más frecuente y el margen de checkpoint más estrecho; no es, por sí solo, la prueba de la brecha de LSN.

## 4. Puntos de fallo, por prioridad

Cada punto dice qué se observó y qué debería hacer la herramienta. Son propuestas, no implementación.

### P0 — Parar MariaDB con una orden que el servidor registre

Evidencia: el panel no contiene `mysqladmin`; importa `TerminateProcess`; el bloque de parada en modo aplicación no tiene un comando de apagado; `mysql_stop.bat` usa `taskkill /F`; el error log encadena arranques con `Starting crash recovery` y sin `Shutdown complete`.

Propuesta: el supervisor de procesos pide el apagado al propio servidor (`mysqladmin --protocol=tcp shutdown` contra el puerto de esa instancia, o el control del servicio de Windows si el proceso es un servicio) y espera a que el PID termine y a que el log escriba el cierre. Si el plazo vence, el registro estructurado debe decir la causa y la acción, y solo entonces se escala a terminar el PID concreto que el supervisor creó. El éxito no es «el puerto ya no responde».

### P0 — No matar por nombre de imagen

Evidencia: `killprocess.bat` pasa el nombre `mysqld.exe` a `taskkill /F`. Durante la inspección había un `mysqld` de `C:\xampp_7_4` en el puerto 3307, y dos paneles de esa otra instalación abiertos a la vez que el de 8.2.

Propuesta: la identidad de un proceso es el PID y el manejador que devolvió el arranque, comprobados contra la ruta del ejecutable y el `--defaults-file`. Parar una instancia no puede seleccionar «cualquier `mysqld.exe`».

### P0 — Tratar como rotos los scripts de parada de esta instalación

Evidencia: `@@BITROCK_INSTALLDIR@@` sin sustituir en `mysql_stop.bat` y `apache_stop.bat`; `start` sin espera; borrado de `%COMPUTERNAME%.pid` cuando el archivo real es `mysql.pid`.

Propuesta: la herramienta no debe delegar en esos `.bat`. Si se conservan como referencia, el supervisor tiene que detectar el token sin sustituir y el PID con otro nombre, y negarse a usarlos.

### P0 — No dar por sano un arranque que solo «llega a escuchar»

Evidencia: InnoDB escribe `10.4.27 started` y `Server socket created` el mismo segundo en que reporta páginas con LSN futuro y la cabecera del tablespace (257 372 914) muy por encima del redo (231 416 435). La brecha se repite el 19 y el 20 de octubre. Hay un `mysqld.dmp` de hoy.

Propuesta: después de arrancar, el módulo de base de datos lee el tramo nuevo del error log. `Starting crash recovery` puede ser normal tras un cierre rápido. `log sequence number is in the future`, «may be corrupt» o un `.dmp` nuevo son fallo, con causa y acción en el log estructurado (por ejemplo: no reutilizar estos redo; hacer copia de `ibdata1` e `ib_logfile*` antes de cualquier recuperación forzada). La recuperación forzada no entra en este informe.

### P1 — El archivo PID no es el estado del proceso

Evidencia: `mysql.pid` = 38840 y `httpd.pid` = 32248, ambos sin proceso. El panel, por sus cadenas, lleva su propia lista (`Creating PID-entry`, `OldPIDs`, `lMySQLPIDs`) y también puede decir `No PIDs found?!`.

Propuesta: el estado lo guarda el supervisor (PID, ruta, puerto, instante de arranque). Un PID file del servidor solo se acepta si el proceso sigue vivo y su línea de comandos apunta a esta instalación. Al parar, no se borra el PID file hasta que el proceso ha salido.

### P1 — Dueño del puerto, y no tocar al dueño ajeno

Evidencia: el panel ya resuelve PID con `GetExtendedTcpTable` y puede negarse a arrancar (`WILL NOT start without the configured ports free`). Esta raíz quiere 3306; la otra instalación estaba en 3307. `properties.ini`, `my.ini` y `httpd.conf` coinciden en 80/443/3306, pero `mysql\data\my.ini` apunta a `c:/xampp/mysql/data`.

Propuesta: un módulo de puertos, separado del supervisor, consulta la tabla TCP y exige que el dueño sea el PID esperado o que el puerto esté libre. Si el dueño es otro directorio de instalación, se informa y no se mata. Una sola configuración manda; un `my.ini` dentro de `data` con otra ruta se señala como contradicción, no se sigue.

### P1 — Un solo modo de vida por instancia

Evidencia: el panel tiene modo servicio (`net` start/stop, ruta esperada con `-k runservice` o `--defaults-file`) y modo aplicación (`--standalone`). Aquí no hay servicio de Windows, y el registro de febrero paró MySQL como aplicación y Apache por PID. `OpenSCManagerW` falla sin admin, y el propio panel lo avisa.

Propuesta: cada instancia se registra como consola o como servicio, no las dos. El modo consola no pide administrador. El modo servicio falla con un error explícito si el SCM no abre, en vez de aparentar que el botón de parar ha funcionado.

### P2 — El control de prerrequisitos del panel no corresponde a este PHP

Evidencia: el EXE busca `x86_microsoft.vc90.crt` (Visual C++ 2008). `php.exe -v` dice Visual C++ 2019 x64. El readme pide el redistribuible de 2019.

Propuesta: el chequeo de runtime tiene que leer el requisito del PHP y del Apache de esa versión, no una lista fija de 2008. Un prerrequisito que falta es un error con acción (instalar el runtime concreto), no un aviso genérico.

### P2 — Redo de 5 MB y buffer pool de 16 MB en un datadir ya inconsistente

Evidencia: `my.ini` fija `innodb_buffer_pool_size=16M`, `innodb_log_file_size=5M`, `innodb_log_buffer_size=8M`. Los `ib_logfile*` del disco miden 5 MB. La ayuda del servidor describe el cierre rápido y la durabilidad del commit; no absuelve un redo que no alcanza al tablespace.

Propuesta: el módulo de configuración rechaza un log buffer mayor que el archivo de log y dimensiona el redo para el uso previsto. Cambiar el tamaño de los log en este directorio, hoy, mezclaría archivos nuevos con un tablespace que ya va por delante: primero hay que tratar el P0 del log de InnoDB.

### P2 — Dos instalaciones y varios paneles a la vez

Evidencia: procesos simultáneos de `C:\xampp_8_2\xampp-control.exe` y de `C:\xampp_7_4\xampp-control.exe`. MariaDB 10.4.27 está en las dos. El panel compara la ruta del servicio (`wrong path`), pero el script de parada no.

Propuesta: cada proyecto lleva su raíz, su puerto y su directorio de datos. La interfaz llama al supervisor; no edita `my.ini` ni mata PID por su cuenta. El registro de la herramienta identifica la raíz en cada línea.

### P3 — Escucha en todas las interfaces y raíz sin contraseña

Evidencia: `bind-address` está comentado en `mysql\bin\my.ini` y el log escribe `Server socket created on IP: '::'.`. El valor compilado de `bind-address` es «sin valor por defecto». `readme_en.txt` documenta el usuario `root` de MySQL sin contraseña. Existe `passwords.txt` en la raíz; no se copia aquí.

Propuesta: el valor por defecto de una instancia local es `127.0.0.1` y una contraseña generada por instalación, guardada fuera del directorio de datos y fuera del repositorio. El paquete XAMPP no sirve de plantilla para ese defecto.

## Preguntas que siguen abiertas

- El flujo exacto de «Stop» en modo aplicación para MySQL: si llama a `TerminateProcess`, si solo observa PIDs, o si hay una tercera vía resuelta con `GetProcAddress`. Hace falta un proveedor nativo (Ghidra 12.1 en `GHIDRA_INSTALL_DIR`) y volver a abrir el mismo SHA-256. Hasta entonces la parada limpia no está demostrada dentro del EXE, y la ausencia de `mysqladmin` sí lo está.
- El código de filtro de `GetExtendedTcpTable` (familia de direcciones y estado listening).
- Si el INI y el log se nombran por `ChangeFileExt` del ejecutable. La inferencia es fuerte y el literal no está; la instrucción no se leyó.
- El estado de las 24 tablas Aria y la causa única de la brecha de LSN. El servidor deja dos explicaciones y no se forzó recuperación.
- `xampp_start.exe`, `xampp_stop.exe` y `service.exe` solo tienen versión de archivo y hash. No cambiaban la conclusión sobre el panel y sobre `mysql_stop.bat`.
