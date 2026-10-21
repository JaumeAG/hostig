# Caso reportado: XAMPP 7.4.33 / MariaDB 10.4.27 — multi-master y arranque detenido

- Clasificación: **caso reportado**
- Fecha de registro: 2026-10-21
- Alcance: síntomas descritos sobre un datadir de XAMPP; no es una inspección REA de este repositorio ni una reproducción ejecutada aquí.

## Entorno declarado

| Pieza | Valor reportado |
|---|---|
| Empaquetado | XAMPP 7.4.33 |
| Motor | MariaDB 10.4.27 |

No se citan rutas de usuario ni nombres de bases de datos.

## Síntomas reportados

1. **`multi-master.info` con líneas de log.** El archivo no contenía solo nombres de conexión. Incluía texto de log del servidor (marcas del estilo `[Note]` / `[ERROR]` y marcas de tiempo), en lugar de la lista corta de nombres que ese fichero admite.
2. **Archivos de replicación huérfanos.** Había del orden de **64** archivos de replicación (`master-*.info`, `relay-log-*.info`, `mysql-relay-bin-*` o equivalentes de esa versión) que no correspondían a conexiones válidas declaradas.
3. **Arranque detenido.** El error log avanzaba hasta una línea del tipo «Server socket created» y no llegaba a «ready for connections». El servidor no respondía al ping de administración.

## Qué no demuestra este documento

- No prueba el orden exacto de escritura de `multi-master.info` ni qué proceso mezcló log y fichero de conexiones.
- No afirma que todo XAMPP 7.4.33 presente el mismo estado.
- No incluye volcados completos del error log ni listados con rutas.

## Relación con Hostig

El [ADR 0012](../adr/0012-preflight-mariadb.md) (propuesto) usa este caso como motivación del preflight (`multi-master.info`, huérfanos de replicación, cuarentena) y del plazo de cuelgue cuando el log deja de avanzar sin ready ni ping.
