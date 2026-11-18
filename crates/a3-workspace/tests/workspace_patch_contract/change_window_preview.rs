//! Focused E3 preview uses the same indexed bytes and action as ordinary preview.
use super::*;
use a3_application::{
    AgentDiffLineEnding, AgentDiffRow, AgentInspectionContext, AgentPatchInspection,
};
use a3_domain::{PatchPreview, TaskId};

fn single_update(
    root: &Path,
    old: &[u8],
    new: &[u8],
) -> Result<(ProjectIdentity, PublishedIndex, PatchAction), Box<dyn Error>> {
    let revision = write_revision(root, b"source.txt", old)?;
    let project = project(root)?;
    let snapshot = SnapshotId::from_bytes([6; 32]);
    let published = published_index(snapshot, vec![revision.clone()])?;
    let action = patch_action(
        project.worktree().id(),
        snapshot,
        vec![PatchOperation::Update(PatchUpdate::new(
            revision,
            PatchFileContent::try_from_bytes(new.to_vec())?,
        )?)],
    )?;
    Ok((project, published, action))
}

fn inspect(
    action: &PatchAction,
    preview: &PatchPreview,
) -> Result<AgentPatchInspection, Box<dyn Error>> {
    Ok(AgentPatchInspection::from_preview(
        AgentInspectionContext::new(
            TaskId::from_bytes([19; 32]),
            action.run_id(),
            action.task_step_id(),
            action.verification_spec_id(),
            action.snapshot_id(),
        ),
        preview,
    )?)
}

#[test]
fn large_middle_update_shows_exact_unicode_crlf_and_absolute_lines() -> Result<(), Box<dyn Error>> {
    let fixture = TempDirectory::new()?;
    let root = fixture.path();
    let old_line = "name = 'caf\u{e9}'\r\n";
    let new_line = "name = 'caf\u{f6}'\r\n";
    let guard = "unchanged guard line with enough bytes to exceed the legacy prefix\r\n";
    let prefix = format!("\u{feff}{old_line}{}", guard.repeat(1_800));
    let suffix = format!("{}{old_line}", guard.repeat(1_800));
    let old = format!("{prefix}{old_line}{suffix}");
    let new = format!("{prefix}{new_line}{suffix}");
    assert!(old.len() > 200 * 1_024);
    let (project, published, action) = single_update(root, old.as_bytes(), new.as_bytes())?;
    let control = Active::default();

    let preview = futures::executor::block_on(
        WorkspacePatchAdapter::with_change_window_preview()
            .preview(&project, &published, &action, &control),
    )?;
    assert_eq!(preview.action_digest(), action.digest());
    assert_eq!(preview.snapshot_id(), action.snapshot_id());
    assert_eq!(preview.entries().len(), 1);
    let entry = &preview.entries()[0];
    let before = entry.before().ok_or("missing before excerpt")?;
    let after = entry.after().ok_or("missing after excerpt")?;
    let line = 1_802;
    assert_eq!(before.bytes(), old_line.as_bytes());
    assert_eq!(after.bytes(), new_line.as_bytes());
    assert_eq!(before.first_line(), line);
    assert_eq!(after.first_line(), line);
    for (side, full) in [(before, old.as_bytes()), (after, new.as_bytes())] {
        assert_eq!(side.content_hash(), hash(full));
        assert_eq!(side.total_bytes(), u64::try_from(full.len())?);
        assert_eq!(side.encoding(), PatchTextEncoding::Utf8Bom);
        assert_eq!(side.line_endings(), PatchLineEndings::Crlf);
        assert!(side.truncated());
    }

    let inspection = inspect(&action, &preview)?;
    let [hunk] = inspection.files()[0].hunks() else {
        return Err("expected one changed hunk".into());
    };
    assert_eq!((hunk.before_start(), hunk.after_start()), (line, line));
    assert_eq!((hunk.before_count(), hunk.after_count()), (1, 1));
    let [
        AgentDiffRow::Removed {
            before_line,
            line: old_row,
        },
        AgentDiffRow::Added {
            after_line,
            line: new_row,
        },
    ] = hunk.rows()
    else {
        return Err("expected exact removal and insertion".into());
    };
    assert_eq!((*before_line, *after_line), (line, line));
    assert_eq!(old_row.text(), "name = 'caf\u{e9}'");
    assert_eq!(new_row.text(), "name = 'caf\u{f6}'");
    assert_eq!(old_row.ending(), AgentDiffLineEnding::Crlf);
    assert_eq!(new_row.ending(), AgentDiffLineEnding::Crlf);
    assert_eq!(fs::read(root.join("source.txt"))?, old.as_bytes());

    let legacy = futures::executor::block_on(
        WorkspacePatchAdapter::new().preview(&project, &published, &action, &control),
    )?;
    let legacy_before = legacy.entries()[0]
        .before()
        .ok_or("missing legacy before")?;
    let legacy_after = legacy.entries()[0].after().ok_or("missing legacy after")?;
    assert_eq!(legacy_before.first_line(), 1);
    assert_eq!(legacy_after.first_line(), 1);
    assert_eq!(legacy_before.bytes(), &old.as_bytes()[..16 * 1_024]);
    assert_eq!(legacy_before.bytes(), legacy_after.bytes());
    assert!(inspect(&action, &legacy)?.files()[0].hunks().is_empty());
    Ok(())
}

#[test]
fn eof_insertion_uses_an_empty_before_side_at_the_absolute_line() -> Result<(), Box<dyn Error>> {
    let fixture = TempDirectory::new()?;
    let old = b"first\r\nsecond\r\n";
    let new = b"first\r\nsecond\r\nadded\r\n";
    let (project, published, action) = single_update(fixture.path(), old, new)?;
    let preview =
        futures::executor::block_on(WorkspacePatchAdapter::with_change_window_preview().preview(
            &project,
            &published,
            &action,
            &Active::default(),
        ))?;
    assert_eq!(
        preview.entries()[0]
            .before()
            .ok_or("missing before excerpt")?
            .bytes(),
        b""
    );
    assert_eq!(
        preview.entries()[0]
            .after()
            .ok_or("missing after excerpt")?
            .bytes(),
        b"added\r\n"
    );
    let inspection = inspect(&action, &preview)?;
    let [hunk] = inspection.files()[0].hunks() else {
        return Err("expected one appended hunk".into());
    };
    assert_eq!((hunk.before_start(), hunk.after_start()), (3, 3));
    assert_eq!((hunk.before_count(), hunk.after_count()), (0, 1));
    assert!(matches!(
        hunk.rows(),
        [AgentDiffRow::Added { after_line: 3, .. }]
    ));
    Ok(())
}

#[test]
fn first_line_bom_and_unterminated_tail_are_retained_when_changed() -> Result<(), Box<dyn Error>> {
    let fixture = TempDirectory::new()?;
    for (old, new, first_line, expected_old, expected_new) in [
        (
            b"\xef\xbb\xbffirst\r\nunchanged\r\n".as_slice(),
            b"\xef\xbb\xbfother\r\nunchanged\r\n".as_slice(),
            1,
            b"\xef\xbb\xbffirst\r\n".as_slice(),
            b"\xef\xbb\xbfother\r\n".as_slice(),
        ),
        (
            b"first\rsecond\r\nthird\nlast".as_slice(),
            b"first\rsecond\r\nthird\nlast\r\nadded\r\n".as_slice(),
            4,
            b"last".as_slice(),
            b"last\r\nadded\r\n".as_slice(),
        ),
    ] {
        let (project, published, action) = single_update(fixture.path(), old, new)?;
        let preview = futures::executor::block_on(
            WorkspacePatchAdapter::with_change_window_preview().preview(
                &project,
                &published,
                &action,
                &Active::default(),
            ),
        )?;
        let before = preview.entries()[0]
            .before()
            .ok_or("missing before excerpt")?;
        let after = preview.entries()[0]
            .after()
            .ok_or("missing after excerpt")?;
        assert_eq!(
            (before.first_line(), after.first_line()),
            (first_line, first_line)
        );
        assert_eq!(before.bytes(), expected_old);
        assert_eq!(after.bytes(), expected_new);
        let inspection = inspect(&action, &preview)?;
        assert_eq!(inspection.files()[0].hunks()[0].before_start(), first_line);
    }
    Ok(())
}

#[test]
fn overlong_line_or_second_distant_change_cannot_hide_behind_a_small_hunk()
-> Result<(), Box<dyn Error>> {
    let fixture = TempDirectory::new()?;
    let long_line = "x".repeat(16 * 1_024);
    let distant = "unchanged exterior\r\n".repeat(900);
    for (old, new) in [
        (format!("a{long_line}\n"), format!("b{long_line}\n")),
        (
            format!("before\r\n{distant}old\r\n"),
            format!("after\r\n{distant}new\r\n"),
        ),
    ] {
        let (project, published, action) =
            single_update(fixture.path(), old.as_bytes(), new.as_bytes())?;
        assert_eq!(
            futures::executor::block_on(
                WorkspacePatchAdapter::with_change_window_preview().preview(
                    &project,
                    &published,
                    &action,
                    &Active::default()
                )
            ),
            Err(PatchPreviewFailure::ChangeWindowTooLarge)
        );
        assert_eq!(fs::read(fixture.path().join("source.txt"))?, old.as_bytes());
    }
    Ok(())
}

#[test]
fn mandatory_update_excerpts_share_the_existing_total_preview_limit() -> Result<(), Box<dyn Error>>
{
    let fixture = TempDirectory::new()?;
    let project = project(fixture.path())?;
    let snapshot = SnapshotId::from_bytes([6; 32]);
    let mut revisions = Vec::new();
    let mut operations = Vec::new();
    let old = "a".repeat(12 * 1_024);
    let new = "b".repeat(12 * 1_024);
    for file in ["one.txt", "two.txt", "three.txt"] {
        let revision = write_revision(fixture.path(), file.as_bytes(), old.as_bytes())?;
        revisions.push(revision.clone());
        operations.push(PatchOperation::Update(PatchUpdate::new(
            revision,
            PatchFileContent::try_from_bytes(new.as_bytes().to_vec())?,
        )?));
    }
    let published = published_index(snapshot, revisions)?;
    let action = patch_action(project.worktree().id(), snapshot, operations)?;
    assert_eq!(
        futures::executor::block_on(WorkspacePatchAdapter::with_change_window_preview().preview(
            &project,
            &published,
            &action,
            &Active::default()
        )),
        Err(PatchPreviewFailure::ChangeWindowTooLarge)
    );
    for file in ["one.txt", "two.txt", "three.txt"] {
        assert_eq!(fs::read(fixture.path().join(file))?, old.as_bytes());
    }
    Ok(())
}

#[test]
fn concealed_secret_in_checked_source_denies_focused_preview() -> Result<(), Box<dyn Error>> {
    let fixture = TempDirectory::new()?;
    let prefix = "safe comment line\n".repeat(1_000);
    let old = format!("{prefix}password=fixture-secret-value\n");
    let new = format!("{prefix}blank=removed\n");
    let (project, published, action) =
        single_update(fixture.path(), old.as_bytes(), new.as_bytes())?;
    assert_eq!(
        futures::executor::block_on(WorkspacePatchAdapter::with_change_window_preview().preview(
            &project,
            &published,
            &action,
            &Active::default()
        )),
        Err(PatchPreviewFailure::Denied)
    );
    Ok(())
}

#[test]
fn focused_preview_does_not_authorize_a_later_live_edit() -> Result<(), Box<dyn Error>> {
    let fixture = TempDirectory::new()?;
    let root = fixture.path();
    let prefix = "unchanged line\n".repeat(1_500);
    let old = format!("{prefix}value = 1\n");
    let new = format!("{prefix}value = 2\n");
    let user = format!("{prefix}value = 9\n");
    let (project, published, action) = single_update(root, old.as_bytes(), new.as_bytes())?;
    let adapter = WorkspacePatchAdapter::with_change_window_preview();
    let control = Active::default();
    let preview =
        futures::executor::block_on(adapter.preview(&project, &published, &action, &control))?;
    assert_eq!(preview.action_digest(), action.digest());
    let authorization = authorize(action)?;

    fs::write(root.join("source.txt"), &user)?;
    assert_eq!(
        futures::executor::block_on(adapter.apply(&project, &published, authorization, &control)),
        Err(PatchApplyFailure::Conflict)
    );
    assert_eq!(fs::read(root.join("source.txt"))?, user.as_bytes());
    Ok(())
}
