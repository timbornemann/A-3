//! Contract tests for opening, migrating, and rejecting unsafe catalog databases.

mod support;

use a3_storage_libsql::{CatalogDatabase, CatalogOpenError, CatalogSchemaVersion, StorageLayout};
use futures::executor::block_on;
use std::fs;
use support::TempDirectory;

#[test]
fn empty_catalog_migrates_and_reopens_at_current_version() -> Result<(), Box<dyn std::error::Error>>
{
    block_on(async {
        let temporary = TempDirectory::new()?;
        let layout = StorageLayout::prepare(temporary.path().join("app-data"))?;

        let first = CatalogDatabase::open(&layout).await?;
        assert_eq!(first.path(), layout.catalog_path());
        assert_eq!(first.schema_version(), CatalogSchemaVersion::CURRENT);
        assert_eq!(
            first.verify().await?.schema_version(),
            CatalogSchemaVersion::CURRENT
        );
        drop(first);

        let reopened = CatalogDatabase::open(&layout).await?;
        assert_eq!(reopened.schema_version(), CatalogSchemaVersion::CURRENT);
        reopened.verify().await?;
        Ok::<(), Box<dyn std::error::Error>>(())
    })
}

#[test]
fn catalog_rejects_a_newer_schema_without_modifying_it() -> Result<(), Box<dyn std::error::Error>> {
    block_on(async {
        let temporary = TempDirectory::new()?;
        let layout = StorageLayout::prepare(temporary.path().join("app-data"))?;
        set_user_version(&layout, CatalogSchemaVersion::CURRENT.get() + 1).await?;
        let content_before = fs::read(layout.catalog_path())?;

        let result = CatalogDatabase::open(&layout).await;
        assert!(matches!(
            result,
            Err(CatalogOpenError::NewerSchema { found, supported })
                if found.get() == CatalogSchemaVersion::CURRENT.get() + 1
                    && supported == CatalogSchemaVersion::CURRENT
        ));

        let observed = read_user_version(&layout).await?;
        assert_eq!(observed, CatalogSchemaVersion::CURRENT.get() + 1);
        assert_eq!(fs::read(layout.catalog_path())?, content_before);
        Ok::<(), Box<dyn std::error::Error>>(())
    })
}

#[test]
fn catalog_rejects_tampered_migration_history() -> Result<(), Box<dyn std::error::Error>> {
    block_on(async {
        let temporary = TempDirectory::new()?;
        let layout = StorageLayout::prepare(temporary.path().join("app-data"))?;
        drop(CatalogDatabase::open(&layout).await?);

        let database = libsql::Builder::new_local(layout.catalog_path())
            .build()
            .await?;
        let connection = database.connect()?;
        connection
            .execute(
                "UPDATE schema_migrations SET checksum = zeroblob(32) WHERE version = 1",
                (),
            )
            .await?;
        drop(connection);
        drop(database);

        assert!(matches!(
            CatalogDatabase::open(&layout).await,
            Err(CatalogOpenError::MigrationHistoryMismatch { version })
                if version == CatalogSchemaVersion::new(1)
        ));
        Ok::<(), Box<dyn std::error::Error>>(())
    })
}

#[test]
fn catalog_rejects_non_database_content_as_corrupt() -> Result<(), Box<dyn std::error::Error>> {
    block_on(async {
        let temporary = TempDirectory::new()?;
        let layout = StorageLayout::prepare(temporary.path().join("app-data"))?;
        fs::write(layout.catalog_path(), b"this is not a database")?;

        assert!(matches!(
            CatalogDatabase::open(&layout).await,
            Err(CatalogOpenError::CorruptDatabase)
        ));
        Ok::<(), Box<dyn std::error::Error>>(())
    })
}

#[test]
fn layout_rejects_a_directory_at_the_catalog_path() -> Result<(), Box<dyn std::error::Error>> {
    let temporary = TempDirectory::new()?;
    let app_data = temporary.path().join("app-data");
    fs::create_dir_all(app_data.join("catalog.db"))?;

    assert!(matches!(
        StorageLayout::prepare(app_data),
        Err(a3_storage_libsql::StorageLayoutError::CatalogNotRegularFile(_))
    ));
    Ok(())
}

#[cfg(unix)]
#[test]
fn catalog_rejects_a_symlink_to_a_file_outside_app_data() -> Result<(), Box<dyn std::error::Error>>
{
    use std::os::unix::fs::symlink;

    block_on(async {
        let temporary = TempDirectory::new()?;
        let layout = StorageLayout::prepare(temporary.path().join("app-data"))?;
        let outside = temporary.path().join("outside.db");
        fs::write(&outside, b"outside")?;
        symlink(outside, layout.catalog_path())?;

        assert!(matches!(
            CatalogDatabase::open(&layout).await,
            Err(CatalogOpenError::Layout(
                a3_storage_libsql::StorageLayoutError::CatalogIsSymbolicLink(_)
            ))
        ));
        Ok::<(), Box<dyn std::error::Error>>(())
    })
}

async fn set_user_version(
    layout: &StorageLayout,
    version: u32,
) -> Result<(), Box<dyn std::error::Error>> {
    let database = libsql::Builder::new_local(layout.catalog_path())
        .build()
        .await?;
    let connection = database.connect()?;
    connection
        .execute_batch(&format!("PRAGMA user_version = {version}"))
        .await?;
    Ok(())
}

async fn read_user_version(layout: &StorageLayout) -> Result<u32, Box<dyn std::error::Error>> {
    let database = libsql::Builder::new_local(layout.catalog_path())
        .build()
        .await?;
    let connection = database.connect()?;
    let mut rows = connection.query("PRAGMA user_version", ()).await?;
    let row = rows
        .next()
        .await?
        .ok_or(libsql::Error::QueryReturnedNoRows)?;
    let version: i64 = row.get(0)?;
    Ok(u32::try_from(version)?)
}
