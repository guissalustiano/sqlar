PREPARE load_cols AS
SELECT
    c.oid,
    c.relname AS TABLE,
    ARRAY_AGG(a.attname) AS COLUMN,
    ARRAY_AGG(a.atttypid) AS type_oid,
    ARRAY_AGG(not a.attnotnull) AS nullable,
    ARRAY_AGG(a.attnum) AS column_position,
    ARRAY_AGG(
        EXISTS (
            SELECT 1
            FROM pg_index ix
            WHERE ix.indrelid = c.oid
            AND ix.indisunique = true
            AND a.attnum = ANY(ix.indkey)
        )
    ) AS has_unique_index
FROM
    pg_attribute a
    JOIN pg_class c ON a.attrelid = c.oid
WHERE
    a.attnum > 0 -- Exclude system columns
    AND NOT a.attisdropped -- Exclude dropped columns
    AND c.relkind = 'r' -- Only regular tables (r), exclude views (v), etc.
GROUP BY
    1;

PREPARE load_funcs AS
SELECT
    p.proname AS function_name,
    p.prorettype AS return_type
FROM
    pg_proc p
