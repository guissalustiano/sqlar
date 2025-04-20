PREPARE list_users AS SELECT id, name FROM users;
PREPARE find_user AS SELECT id, name FROM users where id = $1;
