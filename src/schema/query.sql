PREPARE load_schema AS
SELECT
	c.oid,
	c.relname AS TABLE,
	ARRAY_AGG(a.attname) AS COLUMN,
	ARRAY_AGG(a.atttypid) AS type_oid,
	ARRAY_AGG(a.attnotnull) AS nullable
FROM
	pg_catalog.pg_attribute a
	JOIN pg_catalog.pg_class c ON a.attrelid = c.oid
	JOIN pg_catalog.pg_namespace n ON c.relnamespace = n.oid
WHERE
	a.attnum > 0 -- Exclude system columns
	AND NOT a.attisdropped -- Exclude dropped columns
	AND n.nspname NOT LIKE 'pg_%' -- Exclude system schemas
	AND n.nspname != 'information_schema' -- Exclude information_schema
	AND c.relkind = 'r' -- Only regular tables (r), exclude views (v), etc.
GROUP BY
	1;
