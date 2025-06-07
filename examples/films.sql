PREPARE list_films AS SELECT film_id, title FROM films;
PREPARE find_film AS SELECT film_id, title FROM films where film_id = $1;
PREPARE create_film AS INSERT INTO films(title) VALUES ($1) RETURNING film_id;
PREPARE update_user AS UPDATE films SET title = $2 WHERE film_id = $1 RETURNING film_id, title;
PREPARE delete_user AS DELETE FROM films WHERE film_id = $1 returning film_id, title
