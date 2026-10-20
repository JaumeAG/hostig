# ADR 0007 — Copias de las bases antes de parar o actualizar

- Estado: Aceptado
- Fecha: 2026-10-20

## Contexto

Parar MariaDB, aunque sea con `mariadb-admin shutdown`, y cambiar el binario por otra versión son los momentos en los que un datadir deja de abrirse con el servidor anterior. El análisis de XAMPP mostró un directorio de datos real, con tablas de aplicación, entrando en recuperación con el redo por detrás del tablespace. Una copia previa no arregla esa corrupción ya ocurrida; evita que el gesto normal de «parar» o «cambiar de versión» sea el único momento en el que existían los datos.

El backup tiene que ocurrir dentro del supervisor, antes de la parada, no como un recuerdo en la UI.

## Decisión

Antes de `stop` de una instancia MariaDB, y antes de sustituir el binario o el datadir por otra versión, el supervisor ejecuta `mariadb-dump` de cada esquema de proyecto de esa instancia.

- Flags: `--single-transaction --routines --events --triggers`, contra `127.0.0.1` y el puerto de la instancia, con el usuario de administración.
- Destino: `{datos}/backups/{instancia}/{esquema}/{timestamp}.sql.gz`.
- Se conserva el fichero solo si `gzip` termina con código 0 y el tamaño es mayor que el de un volcado vacío de ese servidor (cabecera de dump presente). Si no, el fichero se descarta y la operación que iba a seguir no empieza.
- Retención: las 10 copias más recientes por esquema. Las más viejas se borran después de que la nueva haya pasado la comprobación.

El volcado no se hace en cada `stop`. Se hace solo si hubo escrituras desde la última copia correcta, o si esa copia tiene más de 24 horas. Las 24 horas son el defecto y se cambian en el manifiesto global. No haber copia anterior cuenta como copia vieja: el primer `stop` sí vuelca.

Los contadores `Innodb_rows_inserted`, `Innodb_rows_updated` e `Innodb_rows_deleted` no bastan: se reinician con el servidor, no ven tablas de otro motor y no ven un cambio de esquema. Se considera que hubo escrituras, y por tanto hace falta volcar, en cualquiera de estos casos:

- El servidor se reinició desde la última copia correcta. Al guardar la copia se anota el identificador de arranque: el instante de arranque derivado de `Uptime` (el reloj del supervisor menos `Uptime`) o el identificador de arranque que esa versión exponga. Si el arranque actual no es el mismo, el servidor se reinició. `Uptime` y los contadores de estado vuelven a cero en cada arranque, así que un contador que no crece después de un reinicio no demuestra que no hubiera escrituras.
- Existe alguna tabla de usuario con motor distinto de InnoDB. Los contadores de filas de InnoDB no ven esas tablas. Los esquemas internos del servidor (`mysql`, `sys`, `information_schema`, `performance_schema`) no cuentan como tablas de usuario.
- Aumentó alguno de los contadores de escritura o de DDL desde la copia: `Com_insert`, `Com_update`, `Com_delete`, `Com_create_table`, `Com_alter_table`, `Com_drop_table`, y los equivalentes de escritura o de DDL que esa versión exponga (entre ellos `Com_replace`, `Com_truncate` y `Com_rename_table`). El supervisor guarda el valor de cada uno al cerrar la copia correcta.

Ante la duda, se vuelca. No se detectan las escrituras copiando el datadir.

Durante el apagado del sistema, el cierre de sesión o la suspensión no se hace volcado, aunque haya escrituras o la copia tenga más de 24 horas. Ese camino es el [ADR 0011](0011-apagado-del-sistema.md).
Si el volcado hace falta y falla, **no se para** y **no se actualiza**. El error incluye la causa (servidor ya caído, disco lleno, permiso) y la acción. Existe un override explícito, `stop --without-backup`, que el log marca como cierre sin copia. La UI no lo ofrece como botón primario: hace falta escribir la confirmación. Si el servidor ya no responde al ping, no hay dump posible; el camino es el de recuperación del [ADR 0003](0003-supervision-de-procesos.md), y el error lo dice en vez de fingir una copia.

Actualizar de versión es: copia, apagado ordenado, arranque con el binario nuevo sobre el mismo datadir solo si la documentación de esa pareja de versiones MariaDB lo permite (actualización hacia una versión mayor soportada). Un salto no soportado se rechaza. Bajar de versión mayor no se intenta sobre el mismo datadir: haría falta un datadir nuevo y una restauración, y eso no entra en el MVP.

Las copias no se suben a ningún sitio. Son archivos locales del usuario.

El MVP incluye la copia en el `stop` de la única instancia, aunque todavía no haya proyectos con nombre: se vuelca el conjunto de esquemas de usuario, excluyendo los esquemas internos del servidor (`mysql`, `sys`, `information_schema`, `performance_schema`).

## Consecuencias

- Parar una base grande tarda lo que tarde el dump cuando el dump hace falta. Si no hubo escrituras y la copia tiene menos de 24 horas, el `stop` no espera un volcado. Sigue siendo el precio de no parar a ciegas cuando sí hay que copiar.
- Un disco lleno bloquea el botón de parar. El mensaje tiene que decir cuánto espacio hacía falta y cómo usar el override.
- `mariadb-dump` no es una copia física de InnoDB. No captura un datadir ya incoherente de forma fiable. Si el health check de LSN ha fallado, el backup se intenta igual y, si el servidor no puede leer tablas, el fallo detiene el `stop` normal.
- Diez copias por esquema pueden ocupar mucho. La retención es un número del manifiesto global, con 10 como defecto.

## Alternativas descartadas

- Copiar la carpeta del datadir en caliente. Sin un backup en caliente de MariaDB (`mariabackup`) eso rompe la pareja redo/tablespace, el mismo fallo visto en XAMPP.
- Avisar y parar igual cuando el dump falla. Se pierde el dato justo en el gesto que prometía guardarlo. El override existe para el caso consciente.
- Backup periódico en segundo plano en el MVP. El tope de 24 horas cubre una base que no recibe escrituras. Un programado distinto puede añadirse después sin cambiar esta decisión.
- Volcar también durante el apagado del sistema. El plazo no da, y un volcado cortado no es una copia.

