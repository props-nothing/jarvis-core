//! Contact storage: saving is an upsert, search is literal, and bad input is refused.

use std::{
    fs,
    path::{Path, PathBuf},
};

use super::*;
use crate::{DEFAULT_DATABASE_FILENAME, LOCAL_WORKSPACE_ID, SqliteDatabase};
use jarvis_core::UtcTimestamp;

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn new() -> Self {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("target")
            .join(format!("jarvis-contacts-{}", jarvis_core::scratch_tag()));
        must(fs::create_dir_all(&path));
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        jarvis_core::remove_scratch_dir(&self.0);
    }
}

fn must<T, E: std::fmt::Debug>(result: Result<T, E>) -> T {
    match result {
        Ok(value) => value,
        Err(error) => panic!("expected success, got {error:?}"),
    }
}

fn at(seconds: i128) -> UtcTimestamp {
    must(UtcTimestamp::from_unix_nanos(
        1_774_000_000_000_000_000 + seconds * 1_000_000_000,
    ))
}

async fn database() -> (TestDirectory, SqliteDatabase) {
    let directory = TestDirectory::new();
    let database =
        must(SqliteDatabase::open(&directory.path().join(DEFAULT_DATABASE_FILENAME)).await);
    (directory, database)
}

fn lead(company: &str, person: &str, email: &str) -> ContactInput {
    ContactInput {
        company: Some(company.to_owned()),
        person: Some(person.to_owned()),
        email: (!email.is_empty()).then(|| email.to_owned()),
        ..ContactInput::default()
    }
}

async fn save(
    database: &SqliteDatabase,
    input: &ContactInput,
    second: i128,
) -> (StoredContact, bool) {
    must(save_contact(database, LOCAL_WORKSPACE_ID, None, input, true, at(second)).await)
}

#[tokio::test]
async fn saving_the_same_lead_twice_updates_it_by_address_or_by_company_and_person() {
    let (_directory, database) = database().await;
    let (first, created) = save(&database, &lead("Acme BV", "Eva Jansen", "eva@acme.nl"), 0).await;
    assert!(created && first.status == ContactStatus::New);

    let update = ContactInput {
        email: Some("EVA@acme.nl".to_owned()),
        status: Some(ContactStatus::Contacted),
        notes: Some("Sent the intro.".to_owned()),
        ..ContactInput::default()
    };
    let (again, created) = save(&database, &update, 5).await;
    assert!(!created, "the address is matched without regard to case");
    assert_eq!(
        (again.id.as_str(), again.status),
        (first.id.as_str(), ContactStatus::Contacted)
    );
    assert_eq!(
        (again.company.as_str(), again.person.as_str()),
        ("Acme BV", "Eva Jansen")
    );
    assert_eq!(again.notes, "Sent the intro.");
    assert_ne!(again.updated_at, again.created_at);

    let (bare, created) = save(&database, &lead("Beta", "Tom", ""), 6).await;
    assert!(created);
    let (found, created) = save(&database, &lead("BETA", "tom", "tom@beta.nl"), 7).await;
    assert!(
        !created,
        "company and person match a contact that has no address yet"
    );
    assert_eq!((found.id, found.email.as_str()), (bare.id, "tom@beta.nl"));
    let stats = must(contact_stats(&database, LOCAL_WORKSPACE_ID).await);
    assert_eq!(stats.iter().map(|(_, held)| held).sum::<u32>(), 2);
}

#[tokio::test]
async fn a_new_contact_needs_a_company_and_every_field_is_bounded() {
    let (_directory, database) = database().await;
    for (input, field) in [
        (ContactInput::default(), "company"),
        (lead("", "x", ""), "company"),
        (lead("Acme", "a\u{0007}b", ""), "person"),
        (lead(&"x".repeat(201), "", ""), "company"),
        (lead("Acme", "", "not an address"), "email"),
        (lead("Acme", "", "a b@c.nl"), "email"),
    ] {
        let refused = save_contact(&database, LOCAL_WORKSPACE_ID, None, &input, false, at(0)).await;
        assert!(
            matches!(refused, Err(DatabaseError::InvalidContact { field: named }) if named == field),
            "{field}: {refused:?}"
        );
    }
    let notes = ContactInput {
        notes: Some("line one\nline two".to_owned()),
        ..lead("Acme", "", "")
    };
    assert_eq!(
        save(&database, &notes, 1).await.0.notes,
        "line one\nline two",
        "notes may span lines"
    );
}

#[tokio::test]
async fn search_matches_text_literally_and_filters_by_status() {
    let (_directory, database) = database().await;
    save(&database, &lead("Acme BV", "Eva", "eva@acme.nl"), 0).await;
    let done = ContactInput {
        status: Some(ContactStatus::DoNotContact),
        ..lead("100% Pure", "Tom", "tom@pure.nl")
    };
    save(&database, &done, 1).await;

    let find = |query: Option<&'static str>, status| {
        let database = &database;
        async move { must(search_contacts(database, LOCAL_WORKSPACE_ID, query, status, 50).await) }
    };
    assert_eq!(find(Some("acme"), None).await.len(), 1);
    assert_eq!(
        find(Some("EVA@"), None).await.len(),
        1,
        "case does not matter"
    );
    assert_eq!(
        find(Some("100%"), None).await.len(),
        1,
        "a percent sign is a character, not a wildcard"
    );
    assert!(find(Some("%"), Some(ContactStatus::New)).await.is_empty());
    assert_eq!(
        find(None, Some(ContactStatus::DoNotContact)).await[0].company,
        "100% Pure"
    );
    assert_eq!(find(None, None).await.len(), 2);
    assert_eq!(
        find(Some("   "), None).await.len(),
        2,
        "a blank query is no query"
    );
}

#[tokio::test]
async fn a_contact_can_be_deleted_and_stats_count_every_status() {
    let (_directory, database) = database().await;
    let (made, _) = save(&database, &lead("Acme", "", ""), 0).await;
    let stats = must(contact_stats(&database, LOCAL_WORKSPACE_ID).await);
    assert_eq!(
        stats.len(),
        ContactStatus::ALL.len(),
        "empty statuses are listed too"
    );
    assert_eq!(stats[0], (ContactStatus::New, 1));
    assert!(must(
        delete_contact(&database, LOCAL_WORKSPACE_ID, &made.id).await
    ));
    assert!(!must(
        delete_contact(&database, LOCAL_WORKSPACE_ID, &made.id).await
    ));
}

#[tokio::test]
async fn only_the_owner_can_lift_do_not_contact() {
    let (_directory, database) = database().await;
    let barred = ContactInput {
        status: Some(ContactStatus::DoNotContact),
        ..lead("Acme", "Eva", "eva@acme.nl")
    };
    save(&database, &barred, 0).await;
    let revive = ContactInput {
        status: Some(ContactStatus::New),
        ..lead("Acme", "Eva", "eva@acme.nl")
    };
    let refused = save_contact(&database, LOCAL_WORKSPACE_ID, None, &revive, false, at(1)).await;
    assert!(
        matches!(
            refused,
            Err(DatabaseError::InvalidContact { field: "status" })
        ),
        "{refused:?}"
    );
    let notes = ContactInput {
        notes: Some("asked not to be contacted".to_owned()),
        ..lead("Acme", "Eva", "eva@acme.nl")
    };
    let (kept, _) =
        must(save_contact(&database, LOCAL_WORKSPACE_ID, None, &notes, false, at(2)).await);
    assert_eq!(
        kept.status,
        ContactStatus::DoNotContact,
        "other changes do not touch the status"
    );
    let (lifted, _) = save(&database, &revive, 3).await;
    assert_eq!(lifted.status, ContactStatus::New, "the owner can");
}

async fn saved(database: &SqliteDatabase, input: &ContactInput, seconds: i128) -> StoredContact {
    must(save_contact(database, LOCAL_WORKSPACE_ID, None, input, true, at(seconds)).await).0
}

/// **The same person at a company spelled two ways, and the same phone number, are found; different people are not.**
#[tokio::test]
async fn look_alikes_are_found_and_strangers_are_not() {
    let (_directory, database) = database().await;
    let mut full = lead("Acme B.V.", "Jan de Vries", "jan@acme.nl");
    full.phone = Some("+31 6 1234 5678".to_owned());
    full.role = Some("Owner".to_owned());
    saved(&database, &full, 1).await;
    // Same person, company without the legal ending, no address.
    saved(&database, &lead("ACME", "jan de vries", ""), 2).await;
    // Another person at the same company.
    saved(&database, &lead("Acme", "Piet Jansen", "piet@acme.nl"), 3).await;
    // The same phone number under another name.
    let mut shared = lead("Other Co", "Kees", "kees@other.nl");
    shared.phone = Some("0031612345678".to_owned());
    saved(&database, &shared, 4).await;
    let pairs = must(find_duplicate_contacts(&database, LOCAL_WORKSPACE_ID, 20).await);
    let reasons: Vec<_> = pairs.iter().map(|pair| pair.reason).collect();
    assert_eq!(pairs.len(), 1, "{reasons:?}");
    assert_eq!(pairs[0].reason, "the same person at the same company");
    assert_eq!(
        pairs[0].keep.email, "jan@acme.nl",
        "the fuller entry is the one kept"
    );
    assert_eq!(pairs[0].remove.email, "");
}

#[test]
fn company_names_are_compared_without_punctuation_and_legal_endings() {
    assert_eq!(company_key("Acme B.V."), company_key("acme"));
    assert_eq!(company_key("De Boer Holding"), company_key("De Boer"));
    assert_ne!(company_key("Acme"), company_key("Acme Bouw"));
    // A name that is only an ending keeps it.
    assert_eq!(company_key("B.V."), "bv");
    assert_eq!(phone_key("12345"), None);
    assert_eq!(
        phone_key("+31 (0)6-1234 5678"),
        Some("310612345678".to_owned())
    );
}

/// **A merge combines the two in one step: the other's gaps are filled, a different value is kept in the notes, do-not-contact wins, and the removed entry is returned whole.**
#[tokio::test]
async fn a_merge_keeps_everything_and_do_not_contact_wins() {
    let (_directory, database) = database().await;
    let mut keep = lead("Acme", "Jan", "jan@acme.nl");
    keep.notes = Some("Met at the fair.".to_owned());
    let keep = saved(&database, &keep, 1).await;
    let mut other = lead("Acme B.V.", "Jan", "");
    other.phone = Some("0612345678".to_owned());
    other.role = Some("Owner".to_owned());
    other.notes = Some("Asked for a quote.".to_owned());
    other.status = Some(ContactStatus::DoNotContact);
    let remove = saved(&database, &other, 2).await;

    let (merged, gone) =
        must(merge_contacts(&database, LOCAL_WORKSPACE_ID, &keep.id, &remove.id, at(9)).await);
    assert_eq!(merged.id, keep.id);
    assert_eq!(merged.phone, "0612345678");
    assert_eq!(merged.role, "Owner");
    assert_eq!(merged.email, "jan@acme.nl");
    assert_eq!(
        merged.status,
        ContactStatus::DoNotContact,
        "the owner's word is never lost in a merge"
    );
    assert!(
        merged.notes.contains("Met at the fair.") && merged.notes.contains("Asked for a quote.")
    );
    assert_eq!(gone.id, remove.id);
    let left = must(list_contacts(&database, LOCAL_WORKSPACE_ID, None, 50, 0).await);
    assert_eq!(left.len(), 1);
}

/// **The other entry's address is taken when the kept one has none, even though addresses are unique.**
#[tokio::test]
async fn a_merge_can_move_an_address_across() {
    let (_directory, database) = database().await;
    let keep = saved(&database, &lead("Acme", "Jan", ""), 1).await;
    let remove = saved(&database, &lead("Acme B.V.", "Jan", "jan@acme.nl"), 2).await;
    // Saved under another spelling of the company, so it is a second entry.
    assert_ne!(keep.id, remove.id);
    let (merged, _) =
        must(merge_contacts(&database, LOCAL_WORKSPACE_ID, &keep.id, &remove.id, at(5)).await);
    assert_eq!(merged.email, "jan@acme.nl");
}

/// **A merge that cannot be done changes nothing.**
#[tokio::test]
async fn a_merge_with_an_unknown_or_same_entry_is_refused_and_changes_nothing() {
    let (_directory, database) = database().await;
    let one = saved(&database, &lead("Acme", "Jan", "jan@acme.nl"), 1).await;
    for (keep, remove) in [
        (one.id.as_str(), one.id.as_str()),
        (one.id.as_str(), "no-such-id"),
        ("no-such-id", one.id.as_str()),
    ] {
        assert!(matches!(
            merge_contacts(&database, LOCAL_WORKSPACE_ID, keep, remove, at(3)).await,
            Err(DatabaseError::InvalidContact { field: "id" })
        ));
    }
    assert!(matches!(
        merge_contacts(&database, "another-workspace", &one.id, &one.id, at(3)).await,
        Err(DatabaseError::InvalidContact { .. })
    ));
    assert_eq!(
        must(list_contacts(&database, LOCAL_WORKSPACE_ID, None, 50, 0).await).len(),
        1
    );
}

#[test]
fn merged_notes_never_exceed_the_limit() {
    let make = |id: &str, notes: String| StoredContact {
        id: id.to_owned(),
        project_id: None,
        company: "A".to_owned(),
        person: String::new(),
        role: String::new(),
        email: String::new(),
        phone: String::new(),
        status: ContactStatus::New,
        notes,
        source: String::new(),
        created_at: "2026-01-01T00:00:00Z".to_owned(),
        updated_at: "2026-01-01T00:00:00Z".to_owned(),
    };
    let merged = combine(&make("a", "x".repeat(1900)), &make("b", "y".repeat(1900)));
    assert_eq!(merged.notes.chars().count(), MAX_MERGED_NOTES);
}

#[tokio::test]
async fn automated_delete_is_workspace_scoped_and_cannot_erase_suppression() {
    let (_directory, database) = database().await;
    let ordinary = saved(&database, &lead("Ordinary", "", "ordinary@example.test"), 0).await;
    assert!(!must(
        delete_contact_for_model(&database, "another-workspace", &ordinary.id).await
    ));
    assert!(must(
        delete_contact_for_model(&database, LOCAL_WORKSPACE_ID, &ordinary.id).await
    ));
    assert!(!must(
        delete_contact_for_model(&database, LOCAL_WORKSPACE_ID, &ordinary.id).await
    ));
    let suppressed = ContactInput {
        status: Some(ContactStatus::DoNotContact),
        ..lead("Suppressed", "Person", "suppressed@example.test")
    };
    let barred = saved(&database, &suppressed, 1).await;
    assert!(matches!(
        delete_contact_for_model(&database, LOCAL_WORKSPACE_ID, &barred.id).await,
        Err(DatabaseError::InvalidContact { field: "status" })
    ));
    let revive = ContactInput {
        status: Some(ContactStatus::New),
        ..suppressed
    };
    assert!(matches!(
        import_contacts(&database, LOCAL_WORKSPACE_ID, None, &[revive], false, at(2)).await,
        Err(DatabaseError::InvalidContact { field: "status" })
    ));
    assert_eq!(
        must(
            contact_status_for_email(&database, LOCAL_WORKSPACE_ID, "suppressed@example.test")
                .await
        ),
        Some(ContactStatus::DoNotContact)
    );
}

#[tokio::test]
async fn bulk_import_reuses_save_matching_and_preserves_suppression() {
    let (_directory, database) = database().await;
    let barred = saved(
        &database,
        &ContactInput {
            status: Some(ContactStatus::DoNotContact),
            ..lead("Suppressed", "", "barred@example.test")
        },
        0,
    )
    .await;
    let inputs = [
        lead("Acme", "Eva", "eva@example.test"),
        ContactInput {
            notes: Some("Updated".to_owned()),
            ..lead("ACME", "Eva", "EVA@example.test")
        },
        lead("Beta", "Tom", ""),
        lead("BETA", "tom", "tom@example.test"),
        ContactInput {
            notes: Some("Still barred".to_owned()),
            ..lead("Suppressed", "", "barred@example.test")
        },
    ];
    let imported =
        must(import_contacts(&database, LOCAL_WORKSPACE_ID, None, &inputs, false, at(1)).await);
    assert_eq!(
        imported,
        ContactImportResult {
            created: 2,
            updated: 3
        }
    );
    let all = must(list_contacts(&database, LOCAL_WORKSPACE_ID, None, 50, 0).await);
    assert_eq!(all.len(), 3);
    let kept = all
        .iter()
        .find(|contact| contact.id == barred.id)
        .unwrap_or_else(|| panic!("suppressed contact retained"));
    assert_eq!(kept.status, ContactStatus::DoNotContact);
    assert_eq!(kept.notes, "Still barred");
}

#[tokio::test]
async fn an_import_rolls_back_prior_creates_and_updates_on_any_late_failure() {
    let (_directory, database) = database().await;
    let existing = saved(&database, &lead("Existing", "", "existing@example.test"), 0).await;
    saved(
        &database,
        &ContactInput {
            status: Some(ContactStatus::DoNotContact),
            ..lead("Suppressed", "", "barred@example.test")
        },
        1,
    )
    .await;
    let before = must(list_contacts(&database, LOCAL_WORKSPACE_ID, None, 50, 0).await);
    for bad in [
        ContactInput {
            status: Some(ContactStatus::New),
            ..lead("Suppressed", "", "barred@example.test")
        },
        ContactInput::default(),
        lead("Invalid", "", "not-an-address"),
    ] {
        let inputs = [
            ContactInput {
                notes: Some("Must roll back".to_owned()),
                ..lead("Changed", "", &existing.email)
            },
            lead("New", "", "new@example.test"),
            bad,
        ];
        assert!(
            import_contacts(&database, LOCAL_WORKSPACE_ID, None, &inputs, false, at(3))
                .await
                .is_err()
        );
        assert_eq!(
            must(list_contacts(&database, LOCAL_WORKSPACE_ID, None, 50, 0).await),
            before
        );
    }
    assert!(
        import_contacts(&database, LOCAL_WORKSPACE_ID, None, &[], false, at(4))
            .await
            .is_err()
    );
    assert!(
        import_contacts(
            &database,
            LOCAL_WORKSPACE_ID,
            None,
            &vec![lead("Too many", "", ""); 1_001],
            false,
            at(4)
        )
        .await
        .is_err()
    );
}

#[tokio::test]
async fn reaching_workspace_capacity_rolls_back_new_rows_in_an_import() {
    let (_directory, database) = database().await;
    must(sqlx::query(
        "WITH RECURSIVE numbers(n) AS (SELECT 1 UNION ALL SELECT n + 1 FROM numbers WHERE n < ?1) \
         INSERT INTO contacts (id, workspace_id, company, created_at, updated_at) \
         SELECT printf('00000000-0000-7000-8000-%012d', n), ?2, 'Company ' || n, ?3, ?3 FROM numbers",
    )
    .bind(MAX_CONTACTS - 1)
    .bind(LOCAL_WORKSPACE_ID)
    .bind(at(0).to_string())
    .execute(database.pool())
    .await);
    let inputs = [
        lead("Must roll back", "", "first@example.test"),
        lead("Over limit", "", "last@example.test"),
    ];
    assert!(matches!(
        import_contacts(&database, LOCAL_WORKSPACE_ID, None, &inputs, false, at(1)).await,
        Err(DatabaseError::ContactLimit)
    ));
    assert_eq!(
        must(contact_status_for_email(&database, LOCAL_WORKSPACE_ID, "first@example.test").await),
        None
    );
    let total: u32 = must(contact_stats(&database, LOCAL_WORKSPACE_ID).await)
        .iter()
        .map(|(_, count)| count)
        .sum();
    assert_eq!(i64::from(total), MAX_CONTACTS - 1);
}

#[tokio::test]
async fn sql_failure_during_import_rolls_back_the_entire_batch() {
    let (_directory, database) = database().await;
    must(sqlx::query(
        "CREATE TRIGGER refuse_test_contact BEFORE INSERT ON contacts WHEN NEW.company = 'Refuse' BEGIN SELECT RAISE(ABORT, 'test refusal'); END",
    ).execute(database.pool()).await);
    let inputs = [lead("Accepted first", "", ""), lead("Refuse", "", "")];
    assert!(matches!(
        import_contacts(&database, LOCAL_WORKSPACE_ID, None, &inputs, false, at(0)).await,
        Err(DatabaseError::Sqlite { .. })
    ));
    assert!(must(list_contacts(&database, LOCAL_WORKSPACE_ID, None, 50, 0).await).is_empty());
}
