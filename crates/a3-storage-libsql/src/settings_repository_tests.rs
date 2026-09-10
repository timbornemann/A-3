use super::*;

type TestResult = Result<(), Box<dyn std::error::Error>>;

// Each fixture is a real SQLite catalog with the production schema/transactions.
// Invalid rows are inserted as new snapshots; immutable history is never edited.
async fn fixture() -> Result<(libsql::Database, Connection), Box<dyn std::error::Error>> {
    let database = libsql::Builder::new_local(":memory:").build().await?;
    let connection = database.connect()?;
    connection.execute("PRAGMA foreign_keys = ON", ()).await?;
    crate::migration::migrate_catalog(&connection).await?;
    let endpoint = ConfiguredModelEndpoint::from_validated_adapter(
        ModelProviderId::try_from_string("ollama".into())?,
        "http://127.0.0.1:11434".into(),
        ModelEndpointScope::LocalLoopback,
    )?;
    let settings = DesktopSettings::unconfigured()
        .with_endpoint(Some(endpoint))
        .with_provider_connection_verified(
            ModelProviderKind::Ollama,
            SettingsTimestamp::from_unix_millis(10)?,
        )?
        .with_provider_enabled(ModelProviderKind::Ollama, true)?;
    let tx = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .await?;
    append_in_transaction(&tx, DesktopSettingsStoreVersion::initial(), &settings).await?;
    tx.commit().await?;
    Ok((database, connection))
}

async fn profiles(
    connection: &Connection,
    coding: &str,
    mapping: &str,
    embedding: &str,
) -> TestResult {
    for (role, model) in [("coding", coding), ("mapping", mapping)] {
        connection.execute("INSERT INTO desktop_llm_profiles VALUES
            (1, ?1, 'ollama', ?2, 8192, 2048, 1, 0, 1000, 'repeat_in_prompt', 'verified', 'native_reported', 10)",
            params![role, model]).await?;
    }
    connection
        .execute(
            "INSERT INTO desktop_embedding_profiles VALUES (1, 'ollama', ?1, 768, 8, 10)",
            [embedding],
        )
        .await?;
    Ok(())
}

#[test]
fn recovery_preserves_valid_roles_and_history_and_is_atomic() -> TestResult {
    crate::run_native_libsql_test(async {
        for (coding, mapping, embedding, expected_roles) in [
            (
                "invalid model",
                "mapper",
                "embed",
                vec![DesktopSettingsProfileRole::Coding],
            ),
            (
                "coder",
                "mapper",
                "invalid model",
                vec![DesktopSettingsProfileRole::Embedding],
            ),
            (
                "invalid model",
                "invalid model",
                "embed",
                vec![
                    DesktopSettingsProfileRole::Coding,
                    DesktopSettingsProfileRole::Mapping,
                ],
            ),
        ] {
            let (_database, connection) = fixture().await?;
            profiles(&connection, coding, mapping, embedding).await?;
            assert!(matches!(
                load_from_connection(&connection).await,
                Err(SettingsRepositoryError::InvalidStoredData)
            ));
            let (candidate, invalid) = load_with_invalid_profiles(&connection).await?;
            assert_eq!(invalid, expected_roles);
            let old_providers = candidate.settings().providers().to_vec();
            // A stale confirmation cannot mutate the current snapshot.
            assert!(matches!(
                recover_from_connection(&connection, DesktopSettingsStoreVersion::initial()).await,
                Err(SettingsRepositoryError::VersionConflict)
            ));
            // Force a write failure after the new parent row: all writes roll back.
            connection
                .execute(
                    "CREATE TRIGGER fixture_fail_append BEFORE INSERT ON desktop_provider_settings
                WHEN NEW.revision = 2 BEGIN SELECT RAISE(ABORT, 'fixture'); END",
                    (),
                )
                .await?;
            assert!(
                recover_from_connection(&connection, candidate.version())
                    .await
                    .is_err()
            );
            assert_eq!(load_latest_version(&connection).await?, candidate.version());
            connection
                .execute("DROP TRIGGER fixture_fail_append", ())
                .await?;
            let recovered = recover_from_connection(&connection, candidate.version()).await?;
            assert_eq!(recovered.version().get(), 2);
            assert_eq!(recovered.settings(), candidate.settings());
            assert_eq!(recovered.settings().providers().as_slice(), old_providers);
            assert_eq!(load_from_connection(&connection).await?, recovered);
            let mut history = connection
                .query(
                    "SELECT model_id FROM desktop_llm_profiles WHERE revision=1 AND role='coding'",
                    (),
                )
                .await?;
            assert_eq!(
                history
                    .next()
                    .await?
                    .ok_or("missing history")?
                    .get::<String>(0)?,
                coding
            );
            assert!(matches!(
                recover_from_connection(&connection, recovered.version()).await,
                Err(SettingsRepositoryError::InvalidStoredData)
            ));
            assert_eq!(load_latest_version(&connection).await?, recovered.version());
        }
        Ok(())
    })
}

#[test]
fn invalid_provider_anchors_and_partial_groups_are_not_recoverable() -> TestResult {
    crate::run_native_libsql_test(async {
        let (_database, connection) = fixture().await?;
        profiles(&connection, "invalid model", "mapper", "embed").await?;
        // Test-only damage to provider anchors must not become a role reset.
        connection
            .execute("DROP TRIGGER desktop_provider_settings_update_guard", ())
            .await?;
        connection.execute("UPDATE desktop_provider_settings SET credential_state='configured', credential_generation=1 WHERE provider_kind='ollama'", ()).await?;
        assert!(load_with_invalid_profiles(&connection).await.is_err());
        assert!(
            recover_from_connection(&connection, DesktopSettingsStoreVersion::new(1)?)
                .await
                .is_err()
        );
        connection.execute("UPDATE desktop_provider_settings SET credential_state='not_required', credential_generation=0 WHERE provider_kind='ollama'", ()).await?;
        connection
            .execute("DROP TRIGGER desktop_provider_settings_delete_guard", ())
            .await?;
        connection
            .execute(
                "DELETE FROM desktop_provider_settings WHERE provider_kind='gemini'",
                (),
            )
            .await?;
        assert!(load_with_invalid_profiles(&connection).await.is_err());
        assert!(
            recover_from_connection(&connection, DesktopSettingsStoreVersion::new(1)?)
                .await
                .is_err()
        );
        assert_eq!(load_latest_version(&connection).await?.get(), 1);
        Ok(())
    })
}

#[test]
fn legacy_snapshots_remain_strict() -> TestResult {
    crate::run_native_libsql_test(async {
        let (_database, connection) = fixture().await?;
        profiles(&connection, "coder", "mapper", "embed").await?;
        connection
            .execute("DROP TRIGGER desktop_provider_settings_delete_guard", ())
            .await?;
        connection
            .execute("DELETE FROM desktop_provider_settings", ())
            .await?;
        assert!(load_from_connection(&connection).await.is_ok());
        connection
            .execute("DROP TRIGGER desktop_llm_profiles_update_guard", ())
            .await?;
        connection
            .execute(
                "UPDATE desktop_llm_profiles SET provider_id='openai' WHERE role='coding'",
                (),
            )
            .await?;
        assert!(load_from_connection(&connection).await.is_err());
        assert!(load_with_invalid_profiles(&connection).await.is_err());
        assert!(
            recover_from_connection(&connection, DesktopSettingsStoreVersion::new(1)?)
                .await
                .is_err()
        );
        Ok(())
    })
}

#[test]
fn unreadable_role_tables_are_not_repairable_and_invalid_bindings_cannot_be_written() -> TestResult
{
    crate::run_native_libsql_test(async {
        let (_database, connection) = fixture().await?;
        profiles(&connection, "coder", "mapper", "embed").await?;
        let settings = load_from_connection(&connection).await?.settings().clone();
        let slot = settings.provider(ModelProviderKind::Ollama).clone();
        // A domain value reconstructed with a disabled slot but retained profiles
        // must be rejected before append, not become the next unreadable snapshot.
        let invalid = settings.with_stored_provider_state(
            slot.kind(),
            slot.endpoint().cloned(),
            false,
            slot.configuration_revision(),
            slot.credential(),
            slot.health(),
            slot.connection_verified_at(),
        )?;
        assert!(validate_persistable(&invalid).is_err());
        connection
            .execute("DROP TABLE desktop_llm_profiles", ())
            .await?;
        assert!(matches!(
            load_with_invalid_profiles(&connection).await,
            Err(SettingsRepositoryError::Read(_))
        ));
        assert!(
            recover_from_connection(&connection, DesktopSettingsStoreVersion::new(1)?)
                .await
                .is_err()
        );
        assert_eq!(load_latest_version(&connection).await?.get(), 1);
        Ok(())
    })
}

#[test]
#[ignore = "explicit local read-only catalog verification; never probes models or reads keys"]
fn existing_user_catalog_loads_read_only() -> TestResult {
    let path =
        std::env::var_os("A3_SETTINGS_READ_ONLY_CATALOG").ok_or("catalog path is required")?;
    futures::executor::block_on(async {
        let stored = read_only_snapshot(std::path::Path::new(&path)).await?;
        assert!(
            stored
                .settings()
                .llm_profile(LlmModelRole::Coding)
                .is_some()
        );
        assert!(
            stored
                .settings()
                .llm_profile(LlmModelRole::Mapping)
                .is_some()
        );
        eprintln!(
            "Read-only settings revision {}: Coding and Mapping restored successfully",
            stored.version().get()
        );
        Ok(())
    })
}

#[test]
fn newer_schemas_and_missing_v8_provider_tables_are_not_legacy_recovery() -> TestResult {
    crate::run_native_libsql_test(async {
        let (_database, connection) = fixture().await?;
        profiles(&connection, "invalid model", "mapper", "embed").await?;
        connection.execute("PRAGMA user_version = 9", ()).await?;
        assert!(matches!(
            load_from_connection(&connection).await,
            Err(SettingsRepositoryError::Open(
                CatalogOpenError::NewerSchema { .. }
            ))
        ));
        assert!(
            recover_from_connection(&connection, DesktopSettingsStoreVersion::new(1)?)
                .await
                .is_err()
        );
        connection.execute("PRAGMA user_version = 8", ()).await?;
        connection
            .execute("DROP TABLE desktop_provider_settings", ())
            .await?;
        assert!(load_with_invalid_profiles(&connection).await.is_err());
        assert!(
            recover_from_connection(&connection, DesktopSettingsStoreVersion::new(1)?)
                .await
                .is_err()
        );
        assert_eq!(load_latest_version(&connection).await?.get(), 1);
        Ok(())
    })
}
