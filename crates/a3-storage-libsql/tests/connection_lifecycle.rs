//! Native lifetime regression. Deliberately no crash retry or success-before-drop marker.
use futures::executor::block_on;
use std::error::Error;

#[test]
fn native_connection_ownership_survives_repeated_drop_and_live_rows() -> Result<(), Box<dyn Error>>
{
    block_on(async {
        let database = libsql::Builder::new_local(":memory:").build().await?;
        for iteration in 0..1024i64 {
            let connection = database.connect()?;
            connection
                .execute("CREATE TABLE lifetime(value INTEGER)", ())
                .await?;
            {
                let transaction = connection.transaction().await?;
                transaction
                    .execute("INSERT INTO lifetime VALUES (?1)", [iteration])
                    .await?;
                transaction.commit().await?;
            }
            let clone = connection.clone();
            drop(connection);
            let mut rows = clone.query("SELECT value FROM lifetime", ()).await?;
            let row = rows.next().await?.ok_or("missing committed value")?;
            assert_eq!(row.get::<i64>(0)?, iteration);
            // Row/statement ownership must keep the native handle valid after its
            // public Connection goes away; final destruction must close only once.
            drop(clone);
            assert_eq!(row.get::<i64>(0)?, iteration);
            drop(row);
            assert!(rows.next().await?.is_none());
            drop(rows);
            let empty = database.connect()?;
            empty
                .execute("CREATE TABLE empty(value INTEGER)", ())
                .await?;
            drop(empty);
        }
        Ok(())
    })
}
