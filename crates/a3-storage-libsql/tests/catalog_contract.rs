//! Contract tests for opening, migrating, and rejecting unsafe catalog databases.

mod support;

use a3_application::{
    AgentWorkspaceLayout, ConfiguredModelEndpoint, DesktopSettings, DesktopSettingsStore,
    ModelEndpointAccess, ModelEndpointScope, ProviderApiKey, ProviderCredential,
    ProviderCredentialRequirement, ProviderCredentialStore, ProviderCredentialStoreFuture,
    SetDesktopProviderCredential, UiPreferencesError, UiPreferencesStore,
};
use a3_domain::ModelProviderId;
use a3_storage_libsql::{
    CatalogDatabase, CatalogOpenError, CatalogSchemaVersion, LibsqlKnowledgeStore, StorageLayout,
};
use futures::executor::block_on;
use std::fs;
use std::sync::{Arc, Mutex, MutexGuard};
use support::TempDirectory;

#[derive(Debug, Default)]
struct MemoryCredentialStore(Mutex<Option<(u64, Vec<u8>)>>);

impl MemoryCredentialStore {
    fn lock(&self) -> MutexGuard<'_, Option<(u64, Vec<u8>)>> {
        match self.0.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }
}

impl ProviderCredentialStore for MemoryCredentialStore {
    fn load<'a>(
        &'a self,
        _provider_id: &'a ModelProviderId,
    ) -> ProviderCredentialStoreFuture<'a, Option<ProviderCredential>> {
        Box::pin(async { Ok(None) })
    }

    fn store<'a>(
        &'a self,
        _provider_id: &'a ModelProviderId,
        credential: &'a ProviderCredential,
    ) -> ProviderCredentialStoreFuture<'a, ()> {
        Box::pin(async move {
            *self.lock() = Some((
                credential.generation().get(),
                credential.secret().as_bytes().to_vec(),
            ));
            Ok(())
        })
    }

    fn delete<'a>(
        &'a self,
        _provider_id: &'a ModelProviderId,
    ) -> ProviderCredentialStoreFuture<'a, ()> {
        Box::pin(async move {
            *self.lock() = None;
            Ok(())
        })
    }
}

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
fn configured_model_snapshot_never_creates_migrates_or_changes_settings()
-> Result<(), Box<dyn std::error::Error>> {
    block_on(async {
        let temporary = TempDirectory::new()?;
        let absent = temporary.path().join("absent.db");
        assert!(
            LibsqlKnowledgeStore::read_settings_snapshot(&absent)
                .await
                .is_err()
        );
        assert!(!absent.exists());
        let layout = StorageLayout::prepare(temporary.path().join("app-data"))?;
        let store = LibsqlKnowledgeStore::open(&layout).await?;
        let before = DesktopSettingsStore::load(&store).await?;
        let bytes = fs::read(layout.catalog_path())?;
        assert_eq!(
            LibsqlKnowledgeStore::read_settings_snapshot(layout.catalog_path()).await?,
            before
        );
        assert_eq!(DesktopSettingsStore::load(&store).await?, before);
        assert_eq!(fs::read(layout.catalog_path())?, bytes);
        drop(store);
        set_user_version(&layout, CatalogSchemaVersion::CURRENT.get() + 1).await?;
        let bytes = fs::read(layout.catalog_path())?;
        assert!(
            LibsqlKnowledgeStore::read_settings_snapshot(layout.catalog_path())
                .await
                .is_err()
        );
        assert_eq!(fs::read(layout.catalog_path())?, bytes);
        assert_eq!(
            read_user_version(&layout).await?,
            CatalogSchemaVersion::CURRENT.get() + 1
        );
        Ok::<(), Box<dyn std::error::Error>>(())
    })
}

#[test]
fn ui_preferences_are_append_only_conflict_checked_and_reopenable()
-> Result<(), Box<dyn std::error::Error>> {
    block_on(async {
        let temporary = TempDirectory::new()?;
        let layout = StorageLayout::prepare(temporary.path().join("app-data"))?;
        let store: Arc<dyn UiPreferencesStore> =
            Arc::new(LibsqlKnowledgeStore::open(&layout).await?);
        let initial = store.load().await?;
        assert_eq!(initial.version().get(), 0);
        assert_eq!(initial.agent_workspace(), AgentWorkspaceLayout::DEFAULT);
        let changed = AgentWorkspaceLayout::new(300, 520, true, false)?;
        let stored = store.append(initial.version(), changed).await?;
        assert_eq!(stored.version().get(), 1);
        assert_eq!(stored.agent_workspace(), changed);
        assert_eq!(
            store
                .append(initial.version(), AgentWorkspaceLayout::DEFAULT)
                .await,
            Err(UiPreferencesError::Conflict)
        );
        drop(store);

        let reopened: Arc<dyn UiPreferencesStore> =
            Arc::new(LibsqlKnowledgeStore::open(&layout).await?);
        assert_eq!(reopened.load().await?, stored);
        Ok::<(), Box<dyn std::error::Error>>(())
    })
}

#[test]
fn provider_api_key_never_appears_in_catalog_files() -> Result<(), Box<dyn std::error::Error>> {
    block_on(async {
        let temporary = TempDirectory::new()?;
        let app_data = temporary.path().join("app-data");
        let layout = StorageLayout::prepare(&app_data)?;
        let store: Arc<dyn DesktopSettingsStore> =
            Arc::new(LibsqlKnowledgeStore::open(&layout).await?);
        let credentials: Arc<dyn ProviderCredentialStore> =
            Arc::new(MemoryCredentialStore::default());
        let endpoint = ConfiguredModelEndpoint::from_validated_adapter_with_security(
            ModelProviderId::try_from_string("gemini".to_owned())?,
            "https://generativelanguage.googleapis.com".to_owned(),
            ModelEndpointScope::Remote,
            ModelEndpointAccess::ExplicitUserInitiatedRemote,
            ProviderCredentialRequirement::ApiKey,
        )?;
        let configured = store
            .append(
                a3_application::DesktopSettingsStoreVersion::initial(),
                &DesktopSettings::unconfigured().with_endpoint(Some(endpoint)),
            )
            .await?;
        let secret = b"a3-test-key-that-must-never-enter-libsql";
        SetDesktopProviderCredential::new(Arc::clone(&store), credentials)
            .execute(
                configured.version(),
                ProviderApiKey::from_bytes(secret.to_vec())?,
            )
            .await?;
        drop(store);

        for entry in fs::read_dir(&app_data)? {
            let path = entry?.path();
            if path.is_file() {
                let bytes = fs::read(&path)?;
                assert!(
                    !bytes.windows(secret.len()).any(|window| window == secret),
                    "provider key leaked into a catalog file"
                );
            }
        }
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
