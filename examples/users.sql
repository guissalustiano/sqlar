PREPARE list_users AS SELECT u.id, u.name FROM users u;
PREPARE find_user AS SELECT u.id, u.name FROM users u where u.id = $1;
