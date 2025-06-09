use std::sync::{Arc, OnceLock, Weak};

use testcontainers_modules::{
    postgres::Postgres,
    testcontainers::{ContainerAsync, ImageExt},
};
use tokio::{io::AsyncWriteExt, sync::Mutex};

use crate::translate_file;

pub(crate) async fn db_transaction() -> (
    Arc<ContainerAsync<Postgres>>,
    tokio_postgres::Transaction<'static>,
) {
    use testcontainers_modules::testcontainers::runners::AsyncRunner;
    // https://github.com/testcontainers/testcontainers-rs/issues/707#issuecomment-2248314261
    static C: OnceLock<Mutex<Weak<ContainerAsync<Postgres>>>> = OnceLock::new();

    let mut guard = C.get_or_init(|| Mutex::new(Weak::new())).lock().await;
    let c = if let Some(c) = guard.upgrade() {
        c
    } else {
        let c = testcontainers_modules::postgres::Postgres::default()
            .with_tag("16-alpine")
            .with_container_name("pg-sqlc-test")
            .start()
            .await
            .unwrap();
        let c = Arc::new(c);
        *guard = Arc::downgrade(&c);

        c
    };
    let host_port = c.get_host_port_ipv4(5432).await.unwrap();
    let url = format!("postgres://postgres:postgres@localhost:{host_port}/postgres");

    let (client, connection) = tokio_postgres::connect(&url, tokio_postgres::NoTls)
        .await
        .unwrap();

    tokio::spawn(async move {
        if let Err(e) = connection.await {
            eprintln!("connection error: {}", e);
        }
    });
    // TODO: client pool
    let client = Box::leak(Box::new(client));
    let t = client.transaction().await.unwrap();

    (c, t)
}

const SEED_TABLES: &str = "
CREATE TABLE films(
    film_id INT PRIMARY KEY GENERATED ALWAYS AS IDENTITY,
    title TEXT NOT NULL,
    description TEXT,
    language_id integer NOT NULL,
    original_language_id integer
);

CREATE TABLE languages (
    language_id INT PRIMARY KEY GENERATED ALWAYS AS IDENTITY,
    name text NOT NULL
);
";

async fn e2e(ps: &str) -> String {
    let (_c, t) = db_transaction().await;
    t.batch_execute(SEED_TABLES).await.unwrap();

    let mut sql = std::io::Cursor::new(ps);
    let mut rs = std::io::Cursor::new(Vec::new());

    translate_file(&t, &mut sql, &mut rs).await.unwrap();
    String::from_utf8(rs.into_inner()).unwrap()
}

macro_rules! t {
    ($fname:ident, $arg:literal) => {
        #[tokio::test]
        async fn $fname() {
            let rs = crate::test::e2e($arg).await;
            insta::assert_snapshot!(rs);
        }
    };
}

mod select {
    t!(
        without_input,
        "PREPARE list_films AS SELECT film_id, title FROM films;"
    );

    t!(
        with_input,
        "PREPARE find_film AS SELECT film_id, title FROM films where film_id = $1;"
    );

    t!(
        with_input_right,
        "PREPARE find_film AS SELECT film_id, title FROM films where $1 = film_id;"
    );

    t!(
        with_multiple_inputs,
        "PREPARE find_film AS SELECT film_id, title FROM films WHERE film_id > $1 AND title LIKE $2;"
    );

    // t!(
    //     full_qualify_from,
    //     "PREPARE a AS SELECT title FROM public.films;"
    // );
    // t!(
    //     full_qualify_projection,
    //     "PREPARE a AS SELECT public.films.title FROM films;"
    // );

    t!(
        multiple_tables,
        "PREPARE a AS SELECT title, name FROM films, languages;"
    );
    t!(
        qualify_projection,
        "PREPARE a AS SELECT films.title FROM films;"
    );
    t!(alias_simple, "PREPARE a AS SELECT f.title FROM films as f;");

    t!(
        alias_unused,
        "PREPARE a AS SELECT title, name FROM films as f, languages as l;"
    );
    t!(
        alias_used,
        "PREPARE a AS SELECT f.title, l.name FROM films as f, languages as l;"
    );
    mod join {
        t!(
            left_join_on,
            "PREPARE a AS SELECT f.title, l.name FROM films as f LEFT JOIN languages as l on f.language_id = l.language_id;"
        );

        t!(
            left_join_using,
            "PREPARE a AS SELECT title, name FROM films LEFT JOIN languages using (language_id);"
        );

        t!(
            join_using,
            "PREPARE a AS SELECT title, name FROM films JOIN languages using (language_id);"
        );
        t!(
            inner_join_using,
            "PREPARE a AS SELECT title, name FROM films INNER JOIN languages using (language_id);"
        );
    }

    mod cast {
        t!(
            double_column,
            "PREPARE a AS SELECT language_id::text from films"
        );
        t!(
            cast_as,
            "PREPARE a AS SELECT CAST(language_id AS text) from films"
        );
    }

    mod _const {
        t!(basic, "PREPARE a AS SELECT 1");
        t!(alias, "PREPARE a AS SELECT 2 as two");
        t!(null, "PREPARE a AS SELECT NULL");
    }

    mod func {
        t!(pi, "PREPARE a AS SELECT pi()");
    }

    mod aggregations {
        t!(
            count,
            "PREPARE a AS SELECT language_id, count(1) from films group by 1"
        );
        t!(
            count_windows,
            "PREPARE a AS SELECT language_id, count(1) OVER () from films group by 1"
        );
    }
    mod common_table_expressions {}
    mod subquery {}
    mod case {}
}

mod insert {
    t!(
        basic,
        "PREPARE create_film AS INSERT INTO films(title) VALUES ($1);"
    );

    t!(
        with_returning,
        "PREPARE create_film AS INSERT INTO films(title) VALUES ($1) RETURNING film_id;"
    );
}

mod update {
    t!(
        basic,
        "PREPARE update_user AS UPDATE films SET title = $2 WHERE film_id = $1;"
    );

    t!(
        with_return,
        "PREPARE update_user AS UPDATE films SET title = $2 WHERE film_id = $1 RETURNING film_id, title;"
    );
}

mod delete {

    t!(
        basic,
        "PREPARE delete_user AS DELETE FROM films WHERE film_id = $1"
    );

    t!(
        with_return,
        "PREPARE delete_user AS DELETE FROM films WHERE film_id = $1 returning film_id, title"
    );
}

t!(
    multiple_prepare,
    "PREPARE list_films AS SELECT film_id, title FROM films;
     PREPARE find_user AS SELECT film_id, title FROM films where film_id = $1;"
);

#[tokio::test]
async fn fill_example() {
    let (_c, t) = db_transaction().await;
    t.batch_execute(SEED_TABLES).await.unwrap();

    let mut sql = std::io::Cursor::new(include_str!("../examples/films.sql"));
    let mut rs = tokio::fs::File::create("./examples/films.rs")
        .await
        .unwrap();
    translate_file(&t, &mut sql, &mut rs).await.unwrap();
    rs.write_all(
        b"\n// The main is not autogenerated, but is needed to example folder to compile\n",
    )
    .await
    .unwrap();
    rs.write_all(
        stringify! {
            fn main() {}
        }
        .as_bytes(),
    )
    .await
    .unwrap();
}
