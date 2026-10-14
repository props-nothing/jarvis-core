//! Contacts: the people and companies JARVIS is working with, and where each stands (`ADR-0155`).
//!
//! A prospecting project that keeps its state in a spreadsheet loses it whenever the file is rewritten, and cannot answer "who have we
//! already contacted?". This is that state, as rows. Saving the same lead twice updates it (matched by address, else by company and
//! person), a status such as `do_not_contact` is a fact a later run can read, and nothing here is ever sent anywhere.

use jarvis_core::{ContactId, UtcTimestamp};
use sqlx::{Row, SqliteConnection};

use crate::database::{DatabaseError, SqliteDatabase};

/// The most contacts one workspace may hold.
pub const MAX_CONTACTS: i64 = 20_000;

/// The most a search returns.
pub const MAX_SEARCH_RESULTS: u32 = 200;

/// Where a contact stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContactStatus {
    /// Known, not yet approached.
    New,
    /// Approached; no answer yet.
    Contacted,
    /// They answered.
    Replied,
    /// A meeting or call is booked or held.
    Meeting,
    /// A customer.
    Won,
    /// Not interested, or not a fit.
    Lost,
    /// Must not be approached again.
    DoNotContact,
}

impl ContactStatus {
    /// Every status, in the order a lead moves through them.
    pub const ALL: [Self; 7] = [
        Self::New,
        Self::Contacted,
        Self::Replied,
        Self::Meeting,
        Self::Won,
        Self::Lost,
        Self::DoNotContact,
    ];

    /// The stored and displayed name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::New => "new",
            Self::Contacted => "contacted",
            Self::Replied => "replied",
            Self::Meeting => "meeting",
            Self::Won => "won",
            Self::Lost => "lost",
            Self::DoNotContact => "do_not_contact",
        }
    }

    /// Reads a stored or typed name.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|status| status.as_str() == text)
    }
}

/// One stored contact.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredContact {
    /// The identifier.
    pub id: String,
    /// The project it was found for, if any.
    pub project_id: Option<String>,
    /// The company (required).
    pub company: String,
    /// The person, or empty.
    pub person: String,
    /// Their role, or empty.
    pub role: String,
    /// Their address, or empty.
    pub email: String,
    /// Their phone number, or empty.
    pub phone: String,
    /// Where they stand.
    pub status: ContactStatus,
    /// Free notes.
    pub notes: String,
    /// Where the lead came from.
    pub source: String,
    /// When it was first saved.
    pub created_at: String,
    /// When it last changed.
    pub updated_at: String,
}

/// What to save. An absent field is left as it is on an existing contact and empty on a new one.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ContactInput {
    /// The company; required for a new contact.
    pub company: Option<String>,
    /// The person.
    pub person: Option<String>,
    /// Their role.
    pub role: Option<String>,
    /// Their address.
    pub email: Option<String>,
    /// Their phone number.
    pub phone: Option<String>,
    /// Where they stand.
    pub status: Option<ContactStatus>,
    /// Free notes (they replace the old ones).
    pub notes: Option<String>,
    /// Where the lead came from.
    pub source: Option<String>,
}

macro_rules! columns {
    () => {
        "id, project_id, company, person, role, email, phone, status, notes, source, created_at, updated_at"
    };
}

fn invalid(field: &'static str) -> DatabaseError {
    DatabaseError::InvalidContact { field }
}

fn sqlite(operation: &'static str) -> impl FnOnce(sqlx::Error) -> DatabaseError {
    move |source| DatabaseError::Sqlite { operation, source }
}

/// Trims a field and checks its length and characters. Notes may span lines; nothing else may.
fn clean(
    value: Option<&String>,
    field: &'static str,
    max: usize,
    multiline: bool,
) -> Result<Option<String>, DatabaseError> {
    let Some(value) = value else { return Ok(None) };
    let value = value.trim();
    let bad_char = |character: char| {
        character.is_control() && !(multiline && (character == '\n' || character == '\t'))
    };
    if value.chars().count() > max || value.chars().any(bad_char) {
        return Err(invalid(field));
    }
    Ok(Some(value.to_owned()))
}

fn check(input: &ContactInput) -> Result<ContactInput, DatabaseError> {
    let email = clean(input.email.as_ref(), "email", 254, false)?;
    if email.as_deref().is_some_and(|email| {
        !email.is_empty() && (!email.contains('@') || email.contains(char::is_whitespace))
    }) {
        return Err(invalid("email"));
    }
    Ok(ContactInput {
        company: clean(input.company.as_ref(), "company", 200, false)?,
        person: clean(input.person.as_ref(), "person", 200, false)?,
        role: clean(input.role.as_ref(), "role", 200, false)?,
        email,
        phone: clean(input.phone.as_ref(), "phone", 40, false)?,
        status: input.status,
        notes: clean(input.notes.as_ref(), "notes", 2000, true)?,
        source: clean(input.source.as_ref(), "source", 300, false)?,
    })
}

fn decode(row: &sqlx::sqlite::SqliteRow) -> Result<StoredContact, DatabaseError> {
    let text = |name: &'static str| -> Result<String, DatabaseError> {
        row.try_get::<String, _>(name)
            .map_err(sqlite("decode a contact"))
    };
    Ok(StoredContact {
        id: text("id")?,
        project_id: row
            .try_get::<Option<String>, _>("project_id")
            .map_err(sqlite("decode a contact"))?,
        company: text("company")?,
        person: text("person")?,
        role: text("role")?,
        email: text("email")?,
        phone: text("phone")?,
        status: ContactStatus::parse(&text("status")?).ok_or(
            DatabaseError::StoredScheduleInvalid {
                field: "contact status",
            },
        )?,
        notes: text("notes")?,
        source: text("source")?,
        created_at: text("created_at")?,
        updated_at: text("updated_at")?,
    })
}

/// Finds the contact an input names: by address, else by company and person when the stored one has no address.
async fn find_existing(
    connection: &mut SqliteConnection,
    workspace_id: &str,
    input: &ContactInput,
) -> Result<Option<StoredContact>, DatabaseError> {
    let by_email = input.email.as_deref().filter(|email| !email.is_empty());
    let row = if let Some(email) = by_email {
        sqlx::query(concat!(
            "SELECT ",
            columns!(),
            " FROM contacts WHERE workspace_id = ?1 AND lower(email) = lower(?2)"
        ))
        .bind(workspace_id)
        .bind(email)
        .fetch_optional(&mut *connection)
        .await
        .map_err(sqlite("find a contact by address"))?
    } else {
        None
    };
    let row = match (row, input.company.as_deref()) {
        (Some(row), _) => Some(row),
        (None, Some(company)) if !company.is_empty() => sqlx::query(concat!(
            "SELECT ",
            columns!(),
            " FROM contacts WHERE workspace_id = ?1 AND lower(company) = lower(?2) \
              AND lower(person) = lower(?3) AND email = ''"
        ))
        .bind(workspace_id)
        .bind(company)
        .bind(input.person.as_deref().unwrap_or(""))
        .fetch_optional(&mut *connection)
        .await
        .map_err(sqlite("find a contact by company"))?,
        (None, _) => None,
    };
    row.as_ref().map(decode).transpose()
}

/// Saves a contact: updates the one the input names, or creates a new one. Returns it and whether it was created.
///
/// # Errors
///
/// `owner` says whether the owner is asking. Anyone else (a model) may not move a contact out of `do_not_contact`: that is the owner's
/// word, and a model that was told to find leads must not be able to undo it.
///
/// # Errors
///
/// Returns [`DatabaseError::InvalidContact`] for a bad field, a new contact with no company, or a status change the caller may not
/// make, [`DatabaseError::ContactLimit`] when the workspace is full, and [`DatabaseError::Sqlite`] when the write fails.
pub async fn save_contact(
    database: &SqliteDatabase,
    workspace_id: &str,
    project_id: Option<&str>,
    input: &ContactInput,
    owner: bool,
    now: UtcTimestamp,
) -> Result<(StoredContact, bool), DatabaseError> {
    let input = check(input)?;
    let mut transaction = database
        .pool()
        .begin_with("BEGIN IMMEDIATE")
        .await
        .map_err(sqlite("begin saving a contact"))?;
    let saved = save_checked_contact(
        &mut transaction,
        workspace_id,
        project_id,
        &input,
        owner,
        now,
    )
    .await?;
    transaction
        .commit()
        .await
        .map_err(sqlite("commit saving a contact"))?;
    Ok(saved)
}

/// Counts successful row operations in an atomic contact import (repeated matches count as updates).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ContactImportResult {
    /// Rows that created a contact.
    pub created: usize,
    /// Rows that updated a matching contact.
    pub updated: usize,
}

/// Saves 1–1,000 validated rows with the same matching and suppression rules as [`save_contact`].
/// Either every row is committed or no contact is changed, including when a later row hits a limit or a do-not-contact guard.
///
/// # Errors
///
/// Returns [`DatabaseError::InvalidContact`] for invalid rows or forbidden status changes, [`DatabaseError::ContactLimit`] when full,
/// and [`DatabaseError::Sqlite`] when the transaction fails.
pub async fn import_contacts(
    database: &SqliteDatabase,
    workspace_id: &str,
    project_id: Option<&str>,
    inputs: &[ContactInput],
    owner: bool,
    now: UtcTimestamp,
) -> Result<ContactImportResult, DatabaseError> {
    if inputs.is_empty() || inputs.len() > 1_000 {
        return Err(invalid("rows"));
    }
    let inputs = inputs.iter().map(check).collect::<Result<Vec<_>, _>>()?;
    let mut transaction = database
        .pool()
        .begin_with("BEGIN IMMEDIATE")
        .await
        .map_err(sqlite("begin importing contacts"))?;
    let mut result = ContactImportResult::default();
    for input in &inputs {
        let (_, created) = save_checked_contact(
            &mut transaction,
            workspace_id,
            project_id,
            input,
            owner,
            now,
        )
        .await?;
        if created {
            result.created += 1;
        } else {
            result.updated += 1;
        }
    }
    transaction
        .commit()
        .await
        .map_err(sqlite("commit importing contacts"))?;
    Ok(result)
}

async fn save_checked_contact(
    connection: &mut SqliteConnection,
    workspace_id: &str,
    project_id: Option<&str>,
    input: &ContactInput,
    owner: bool,
    now: UtcTimestamp,
) -> Result<(StoredContact, bool), DatabaseError> {
    let stamp = now.to_string();
    if let Some(existing) = find_existing(connection, workspace_id, input).await? {
        if !owner
            && existing.status == ContactStatus::DoNotContact
            && input
                .status
                .is_some_and(|status| status != ContactStatus::DoNotContact)
        {
            return Err(invalid("status"));
        }
        let pick = |new: &Option<String>, old: &str| new.clone().unwrap_or_else(|| old.to_owned());
        let status = input.status.unwrap_or(existing.status);
        let row = sqlx::query(concat!(
            "UPDATE contacts SET company = ?2, person = ?3, role = ?4, email = ?5, phone = ?6, status = ?7, notes = ?8, \
             source = ?9, project_id = COALESCE(project_id, ?10), updated_at = ?11 WHERE id = ?1 RETURNING ",
            columns!()
        ))
        .bind(&existing.id)
        .bind(pick(&input.company, &existing.company))
        .bind(pick(&input.person, &existing.person))
        .bind(pick(&input.role, &existing.role))
        .bind(pick(&input.email, &existing.email))
        .bind(pick(&input.phone, &existing.phone))
        .bind(status.as_str())
        .bind(pick(&input.notes, &existing.notes))
        .bind(pick(&input.source, &existing.source))
        .bind(project_id)
        .bind(&stamp)
        .fetch_one(&mut *connection)
        .await
        .map_err(sqlite("update a contact"))?;
        return Ok((decode(&row)?, false));
    }
    let company = input
        .company
        .as_deref()
        .filter(|company| !company.is_empty())
        .ok_or_else(|| invalid("company"))?;
    let held: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM contacts WHERE workspace_id = ?1")
        .bind(workspace_id)
        .fetch_one(&mut *connection)
        .await
        .map_err(sqlite("count contacts"))?;
    if held >= MAX_CONTACTS {
        return Err(DatabaseError::ContactLimit);
    }
    let text = |value: &Option<String>| value.clone().unwrap_or_default();
    let row = sqlx::query(concat!(
        "INSERT INTO contacts (id, workspace_id, project_id, company, person, role, email, phone, status, notes, source, \
         created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?12) RETURNING ",
        columns!()
    ))
    .bind(ContactId::new().to_string())
    .bind(workspace_id)
    .bind(project_id)
    .bind(company)
    .bind(text(&input.person))
    .bind(text(&input.role))
    .bind(text(&input.email))
    .bind(text(&input.phone))
    .bind(input.status.unwrap_or(ContactStatus::New).as_str())
    .bind(text(&input.notes))
    .bind(text(&input.source))
    .bind(&stamp)
    .fetch_one(&mut *connection)
    .await
    .map_err(sqlite("create a contact"))?;
    Ok((decode(&row)?, true))
}

/// Escapes `%`, `_` and the escape character itself, so a search text is matched literally.
fn like_pattern(text: &str) -> String {
    let mut pattern = String::with_capacity(text.len() + 2);
    pattern.push('%');
    for character in text.chars() {
        if matches!(character, '%' | '_' | '\\') {
            pattern.push('\\');
        }
        pattern.push(character);
    }
    pattern.push('%');
    pattern
}

/// Contacts whose company, person, address or notes contain `query` (any, when absent) and that have `status` (any, when absent),
/// by company, at most `limit` (never more than [`MAX_SEARCH_RESULTS`]).
///
/// # Errors
///
/// Returns [`DatabaseError`] when the read fails.
pub async fn search_contacts(
    database: &SqliteDatabase,
    workspace_id: &str,
    query: Option<&str>,
    status: Option<ContactStatus>,
    limit: u32,
) -> Result<Vec<StoredContact>, DatabaseError> {
    let pattern = query
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(|text| like_pattern(&text.to_lowercase()));
    let rows = sqlx::query(concat!(
        "SELECT ",
        columns!(),
        " FROM contacts WHERE workspace_id = ?1 AND (?2 IS NULL OR status = ?2) AND (?3 IS NULL OR \
         lower(company) LIKE ?3 ESCAPE '\\' OR lower(person) LIKE ?3 ESCAPE '\\' OR lower(email) LIKE ?3 ESCAPE '\\' \
         OR lower(notes) LIKE ?3 ESCAPE '\\') \
         ORDER BY company COLLATE NOCASE, person COLLATE NOCASE, id LIMIT ?4"
    ))
    .bind(workspace_id)
    .bind(status.map(ContactStatus::as_str))
    .bind(pattern)
    .bind(i64::from(limit.clamp(1, MAX_SEARCH_RESULTS)))
    .fetch_all(database.pool())
    .await
    .map_err(sqlite("search contacts"))?;
    rows.iter().map(decode).collect()
}

/// The status of the contact with this address, if there is one. Case is ignored, as in saving.
///
/// # Errors
///
/// Returns [`DatabaseError`] when the read fails.
pub async fn contact_status_for_email(
    database: &SqliteDatabase,
    workspace_id: &str,
    email: &str,
) -> Result<Option<ContactStatus>, DatabaseError> {
    let name: Option<String> = sqlx::query_scalar(
        "SELECT status FROM contacts WHERE workspace_id = ?1 AND lower(email) = lower(?2)",
    )
    .bind(workspace_id)
    .bind(email.trim())
    .fetch_optional(database.pool())
    .await
    .map_err(sqlite("read a contact's status"))?;
    Ok(name.as_deref().and_then(ContactStatus::parse))
}

/// One page of contacts with `status` (any, when absent), by company: `limit` of them after skipping `offset`. For the owner's own
/// views and exports, so it is not capped at [`MAX_SEARCH_RESULTS`].
///
/// # Errors
///
/// Returns [`DatabaseError`] when the read fails.
pub async fn list_contacts(
    database: &SqliteDatabase,
    workspace_id: &str,
    status: Option<ContactStatus>,
    limit: u32,
    offset: u32,
) -> Result<Vec<StoredContact>, DatabaseError> {
    let rows = sqlx::query(concat!(
        "SELECT ",
        columns!(),
        " FROM contacts WHERE workspace_id = ?1 AND (?2 IS NULL OR status = ?2) \
         ORDER BY company COLLATE NOCASE, person COLLATE NOCASE, id LIMIT ?3 OFFSET ?4"
    ))
    .bind(workspace_id)
    .bind(status.map(ContactStatus::as_str))
    .bind(i64::from(limit))
    .bind(i64::from(offset))
    .fetch_all(database.pool())
    .await
    .map_err(sqlite("list contacts"))?;
    rows.iter().map(decode).collect()
}

/// How many contacts stand at each status, for every status (zero included).
///
/// # Errors
///
/// Returns [`DatabaseError`] when the read fails.
pub async fn contact_stats(
    database: &SqliteDatabase,
    workspace_id: &str,
) -> Result<Vec<(ContactStatus, u32)>, DatabaseError> {
    let rows = sqlx::query(
        "SELECT status, COUNT(*) AS held FROM contacts WHERE workspace_id = ?1 GROUP BY status",
    )
    .bind(workspace_id)
    .fetch_all(database.pool())
    .await
    .map_err(sqlite("count contacts by status"))?;
    let mut counts: Vec<(ContactStatus, u32)> = ContactStatus::ALL
        .into_iter()
        .map(|status| (status, 0))
        .collect();
    for row in &rows {
        let name: String = row
            .try_get("status")
            .map_err(sqlite("decode a contact count"))?;
        let held: i64 = row
            .try_get("held")
            .map_err(sqlite("decode a contact count"))?;
        if let Some(status) = ContactStatus::parse(&name)
            && let Some(entry) = counts.iter_mut().find(|(known, _)| *known == status)
        {
            entry.1 = u32::try_from(held).unwrap_or(u32::MAX);
        }
    }
    Ok(counts)
}

/// Deletes a contact by identifier, returning whether one was removed.
///
/// # Errors
///
/// Returns [`DatabaseError`] when the write fails.
pub async fn delete_contact(
    database: &SqliteDatabase,
    workspace_id: &str,
    id: &str,
) -> Result<bool, DatabaseError> {
    let done = sqlx::query("DELETE FROM contacts WHERE workspace_id = ?1 AND id = ?2")
        .bind(workspace_id)
        .bind(id)
        .execute(database.pool())
        .await
        .map_err(sqlite("delete a contact"))?;
    Ok(done.rows_affected() > 0)
}

/// Removes a contact for an automated caller, but never erases an owner's suppression.
/// An approval to delete is not permission to lift `do_not_contact`: the owner must change that status explicitly first.
/// Returns whether a contact existed in this workspace.
///
/// # Errors
///
/// Returns [`DatabaseError::InvalidContact`] with field `status` for a suppressed contact, and [`DatabaseError::Sqlite`] on failure.
pub async fn delete_contact_for_model(
    database: &SqliteDatabase,
    workspace_id: &str,
    id: &str,
) -> Result<bool, DatabaseError> {
    let mut transaction = database
        .pool()
        .begin_with("BEGIN IMMEDIATE")
        .await
        .map_err(sqlite("begin deleting a contact"))?;
    let status: Option<String> =
        sqlx::query_scalar("SELECT status FROM contacts WHERE workspace_id = ?1 AND id = ?2")
            .bind(workspace_id)
            .bind(id)
            .fetch_optional(&mut *transaction)
            .await
            .map_err(sqlite("check a contact before deleting"))?;
    if status.as_deref() == Some(ContactStatus::DoNotContact.as_str()) {
        return Err(invalid("status"));
    }
    let done = sqlx::query("DELETE FROM contacts WHERE workspace_id = ?1 AND id = ?2")
        .bind(workspace_id)
        .bind(id)
        .execute(&mut *transaction)
        .await
        .map_err(sqlite("delete an automated contact"))?;
    transaction
        .commit()
        .await
        .map_err(sqlite("commit deleting a contact"))?;
    Ok(done.rows_affected() > 0)
}

/// Two entries that look like one person or company, with the one to keep named first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DuplicatePair {
    /// The entry to keep: the fuller one, or the older when they are equally full.
    pub keep: StoredContact,
    /// The entry that would be folded into it.
    pub remove: StoredContact,
    /// Why they look alike, in plain words.
    pub reason: &'static str,
}

/// The longest notes a merged contact keeps.
const MAX_MERGED_NOTES: usize = 2000;

/// A company name reduced to what identifies it: lower case, no punctuation or spaces, and without a legal ending such as "B.V.".
fn company_key(name: &str) -> String {
    const ENDINGS: [&str; 12] = [
        "bv", "nv", "vof", "bvba", "ltd", "llc", "inc", "gmbh", "sa", "ag", "cv", "holding",
    ];
    let words: Vec<String> = name
        .split_whitespace()
        .map(|word| {
            word.chars()
                .filter(|character| character.is_alphanumeric())
                .collect::<String>()
                .to_lowercase()
        })
        .filter(|word| !word.is_empty())
        .collect();
    let kept = match words.split_last() {
        Some((last, rest)) if !rest.is_empty() && ENDINGS.contains(&last.as_str()) => rest,
        _ => &words[..],
    };
    kept.concat()
}

fn person_key(name: &str) -> String {
    name.chars()
        .filter(|character| character.is_alphanumeric())
        .collect::<String>()
        .to_lowercase()
}

/// A phone number as its digits, when there are enough of them to mean something.
fn phone_key(phone: &str) -> Option<String> {
    let digits: String = phone.chars().filter(char::is_ascii_digit).collect();
    (digits.len() >= 7).then_some(digits)
}

fn fullness(contact: &StoredContact) -> usize {
    [
        &contact.person,
        &contact.role,
        &contact.email,
        &contact.phone,
        &contact.notes,
        &contact.source,
    ]
    .iter()
    .filter(|field| !field.is_empty())
    .count()
}

/// What two entries become when they are one: the kept entry's words win and the other fills what is empty; a different value the
/// other held is written into the notes rather than lost; `do_not_contact` on either side wins; and a kept `new` yields to the other's
/// status.
#[must_use]
pub fn combine(keep: &StoredContact, remove: &StoredContact) -> StoredContact {
    let first = |kept: &str, other: &str| if kept.is_empty() { other } else { kept }.to_owned();
    let mut also: Vec<String> = Vec::new();
    for (label, kept, other) in [
        ("person", &keep.person, &remove.person),
        ("role", &keep.role, &remove.role),
        ("email", &keep.email, &remove.email),
        ("phone", &keep.phone, &remove.phone),
    ] {
        if !kept.is_empty() && !other.is_empty() && !kept.eq_ignore_ascii_case(other) {
            also.push(format!("{label} {other}"));
        }
    }
    let mut notes = keep.notes.clone();
    for addition in [
        (!remove.notes.is_empty() && remove.notes != keep.notes).then(|| remove.notes.clone()),
        (!also.is_empty()).then(|| format!("Also recorded: {}", also.join("; "))),
    ]
    .into_iter()
    .flatten()
    {
        if !notes.is_empty() {
            notes.push('\n');
        }
        notes.push_str(&addition);
    }
    let status = if keep.status == ContactStatus::DoNotContact
        || remove.status == ContactStatus::DoNotContact
    {
        ContactStatus::DoNotContact
    } else if keep.status == ContactStatus::New {
        remove.status
    } else {
        keep.status
    };
    StoredContact {
        id: keep.id.clone(),
        project_id: keep
            .project_id
            .clone()
            .or_else(|| remove.project_id.clone()),
        company: keep.company.clone(),
        person: first(&keep.person, &remove.person),
        role: first(&keep.role, &remove.role),
        email: first(&keep.email, &remove.email),
        phone: first(&keep.phone, &remove.phone),
        status,
        notes: notes.chars().take(MAX_MERGED_NOTES).collect(),
        source: first(&keep.source, &remove.source),
        created_at: keep.created_at.clone().min(remove.created_at.clone()),
        updated_at: keep.updated_at.clone(),
    }
}

/// Finds entries that look like the same company or person: the same person at a company spelled differently or entered with and without an
/// address, or the same phone number. At most `limit` pairs, each with the fuller entry first. Pure, so the owner can see exactly what it
/// would match.
#[must_use]
pub fn duplicate_pairs(contacts: &[StoredContact], limit: usize) -> Vec<DuplicatePair> {
    use std::collections::{BTreeMap, BTreeSet};
    let mut groups: BTreeMap<(&'static str, String), Vec<usize>> = BTreeMap::new();
    for (index, contact) in contacts.iter().enumerate() {
        let (company, person) = (company_key(&contact.company), person_key(&contact.person));
        if !company.is_empty() && !person.is_empty() {
            groups
                .entry((
                    "the same person at the same company",
                    format!("{company}|{person}"),
                ))
                .or_default()
                .push(index);
        }
        if let Some(phone) = phone_key(&contact.phone) {
            groups
                .entry(("the same phone number", phone))
                .or_default()
                .push(index);
        }
    }
    let mut seen: BTreeSet<(usize, usize)> = BTreeSet::new();
    let mut pairs = Vec::new();
    for ((reason, _), members) in &groups {
        for window in members.windows(2) {
            let (a, b) = (window[0].min(window[1]), window[0].max(window[1]));
            if a == b || !seen.insert((a, b)) || pairs.len() >= limit {
                continue;
            }
            let (one, two) = (&contacts[a], &contacts[b]);
            let one_first = (fullness(one), std::cmp::Reverse(&one.created_at))
                >= (fullness(two), std::cmp::Reverse(&two.created_at));
            let (keep, remove) = if one_first { (one, two) } else { (two, one) };
            pairs.push(DuplicatePair {
                keep: keep.clone(),
                remove: remove.clone(),
                reason,
            });
        }
    }
    pairs
}

/// The pairs of entries that look like duplicates, at most `limit`.
///
/// # Errors
///
/// Returns [`DatabaseError`] when the read fails.
pub async fn find_duplicate_contacts(
    database: &SqliteDatabase,
    workspace_id: &str,
    limit: usize,
) -> Result<Vec<DuplicatePair>, DatabaseError> {
    let all = list_contacts(
        database,
        workspace_id,
        None,
        u32::try_from(MAX_CONTACTS).unwrap_or(u32::MAX),
        0,
    )
    .await?;
    Ok(duplicate_pairs(&all, limit))
}

/// Folds `remove` into `keep` as [`combine`] describes, in one step: either both changes happen or neither. Returns the entry as it now
/// is and the entry that was removed, whole, so nothing it held is lost to the caller.
///
/// # Errors
///
/// Returns [`DatabaseError::InvalidContact`] with field `id` when either entry does not exist in this workspace or they are the same one,
/// and [`DatabaseError::Sqlite`] when a write fails.
pub async fn merge_contacts(
    database: &SqliteDatabase,
    workspace_id: &str,
    keep_id: &str,
    remove_id: &str,
    now: UtcTimestamp,
) -> Result<(StoredContact, StoredContact), DatabaseError> {
    if keep_id == remove_id {
        return Err(invalid("id"));
    }
    let mut transaction = database
        .pool()
        .begin()
        .await
        .map_err(sqlite("begin a merge"))?;
    let mut read = Vec::new();
    for id in [keep_id, remove_id] {
        let row = sqlx::query(concat!(
            "SELECT ",
            columns!(),
            " FROM contacts WHERE workspace_id = ?1 AND id = ?2"
        ))
        .bind(workspace_id)
        .bind(id)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(sqlite("read a contact to merge"))?
        .ok_or_else(|| invalid("id"))?;
        read.push(decode(&row)?);
    }
    let (keep, remove) = (&read[0], &read[1]);
    let combined = combine(keep, remove);
    // The other entry goes first: its address may be the one the kept entry takes, and addresses are unique.
    sqlx::query("DELETE FROM contacts WHERE workspace_id = ?1 AND id = ?2")
        .bind(workspace_id)
        .bind(remove_id)
        .execute(&mut *transaction)
        .await
        .map_err(sqlite("remove the merged contact"))?;
    let row = sqlx::query(concat!(
        "UPDATE contacts SET project_id = ?2, person = ?3, role = ?4, email = ?5, phone = ?6, status = ?7, notes = ?8, source = ?9, \
         created_at = ?10, updated_at = ?11 WHERE id = ?1 RETURNING ",
        columns!()
    ))
    .bind(keep_id)
    .bind(combined.project_id.as_deref())
    .bind(&combined.person)
    .bind(&combined.role)
    .bind(&combined.email)
    .bind(&combined.phone)
    .bind(combined.status.as_str())
    .bind(&combined.notes)
    .bind(&combined.source)
    .bind(&combined.created_at)
    .bind(now.to_string())
    .fetch_one(&mut *transaction)
    .await
    .map_err(sqlite("update the merged contact"))?;
    let merged = decode(&row)?;
    transaction
        .commit()
        .await
        .map_err(sqlite("commit a merge"))?;
    Ok((merged, remove.clone()))
}
#[cfg(test)]
#[path = "contact_tests.rs"]
mod tests;
