//! Storage adapters and migration infrastructure for JARVIS.

mod config;
mod credential;
mod database;
mod embedding_repository;
mod identity_repository;
mod inspect;
mod knowledge_repository;
pub mod mcp_secrets;
mod memory_repository;
mod message_repository;
mod paths;
mod pgvector;
mod run_event_repository;
mod run_repository;
mod run_transcript_repository;
mod session_repository;
mod summary_repository;

mod approval_repository;
mod contact_repository;
mod delegated_task_repository;
mod digest_repository;
mod phone_call_repository;
mod project_repository;
mod schedule_repository;
pub mod settings;
mod skill_repository;
mod tool_call_repository;
mod trigger_repository;
mod workspace_repository;

#[cfg(any(test, feature = "test-support"))]
mod test_support;

pub use approval_repository::{
    MAX_APPROVAL_ARGUMENTS_BYTES, attach_approval_arguments, clear_approval_arguments,
    clear_arguments_of_started_calls, create_approval, find_approval, read_approval_arguments,
    read_pending_approvals, read_run_approvals, read_workspace_pending_approvals,
    read_workspace_unreleased_approvals, record_decision,
};
pub use config::{
    CURRENT_CONFIG_VERSION, Config, ConfigError, ConfigMigration, ConfigStore,
    DEFAULT_PHONE_CALLS_PER_DAY, DEFAULT_PHONE_CALLS_PER_HOUR, DaemonConfig,
    LIVE_PROVIDER_MODEL_NAME, LoadedConfig, LoggingConfig, ProfileConfig,
    unrecognized_environment_keys,
};
pub use contact_repository::{
    ContactImportResult, ContactInput, ContactStatus, DuplicatePair, MAX_CONTACTS,
    MAX_SEARCH_RESULTS, StoredContact, combine, contact_stats, contact_status_for_email,
    delete_contact, delete_contact_for_model, duplicate_pairs, find_duplicate_contacts,
    import_contacts, list_contacts, merge_contacts, save_contact, search_contacts,
};
pub use credential::{CREDENTIAL_FILE_NAME, CredentialStore, CredentialStoreError};
pub use database::{
    CURRENT_SCHEMA_VERSION, DEFAULT_DATABASE_FILENAME, DaemonInstanceStart, DaemonStopReason,
    DatabaseError, SqliteDatabase,
};
pub use delegated_task_repository::{
    DelegatedTaskFinish, MAX_TASK_ACTIVITY_CHARS, MAX_TASK_ACTIVITY_RETAINED,
    MAX_TASK_ACTIVITY_SHOWN, NewDelegatedTask, StoredDelegatedTask, StoredDelegatedTaskActivity,
    append_delegated_task_activity, create_delegated_task, create_delegated_task_with_request,
    find_delegated_task, finish_delegated_task, interrupt_delegated_tasks, list_delegated_tasks,
    mark_delegated_task_running,
};
pub use digest_repository::{
    FailedRun, ModelUsage, ProjectActivity, RunTotals, StoredDigest, digest_since,
};
pub use embedding_repository::{
    DEFAULT_INDEXED_DIMENSIONS, EmbeddingError, MAX_SIMILARITY_RESULTS, NewMemoryEmbedding,
    SimilarMemory, SimilarityQuery, index_ddl, record_memory_embedding, search_similar,
};
pub use identity_repository::{
    LOCAL_USER_ID, LOCAL_WORKSPACE_ID, LocalIdentity, load_local_identity,
};
pub use inspect::{DaemonInstanceRow, DatabaseInspection, DatabaseState, inspect_database};
pub use knowledge_repository::{
    KnowledgeChunkInput, KnowledgeFileInput, KnowledgeFileState, KnowledgeHit, KnowledgeKind,
    KnowledgeStats, MAX_CHUNK_CHARS, clear_knowledge, knowledge_manifest, knowledge_stats,
    remove_knowledge_file, replace_knowledge_file, search_knowledge,
};
pub use memory_repository::{
    EntityKind, EntityStatus, MAX_RECALL_CANDIDATES, MemoryTransition, NewEntity, StoredAlias,
    StoredEntity, StoredMemory, StoredRelation, TASK_LIKE_PREDICATES, apply_memory_transition,
    count_entity_memory_links, count_memory_entity_links, find_entity, find_memory,
    find_memory_including_deleted, is_tombstoned, link_memory_entity, merge_entities, purge_memory,
    read_alias_candidates, read_all_memories, read_entities_by_label, read_entity_aliases,
    read_entity_memories, read_memory_discovery_page, read_retrievable_memories,
    read_subject_relations, read_task_memory_candidates, read_workspace_entities,
    read_workspace_memories, record_alias, record_entity, record_memory, record_relation,
    record_tombstone, reinforce_memory, replace_memory, resolve_alias,
};
pub use message_repository::{
    MAX_MESSAGE_PAGE, StoredMessage, append_message, count_messages, find_message, read_messages,
    read_messages_from, read_recent_messages,
};
pub use paths::{
    AppPaths, PathError, PathKind, PathMode, RuntimePathSource, portable_layout,
    secure_private_file,
};
pub use pgvector::{
    DistanceMetric, MAX_INDEXED_DIMENSIONS, PgVectorError, decode as decode_embedding,
    encode as encode_embedding, is_indexable,
};
pub use phone_call_repository::{
    MAX_PHONE_REASON_CHARS, NewPhoneCall, PhoneCallUpdate, StoredPhoneCall, attach_provider_call,
    count_phone_calls_since, create_phone_call, delete_phone_calls, find_phone_call,
    list_phone_calls, prune_phone_calls, unsettled_phone_calls, update_phone_call,
};
pub use project_repository::{
    LinkKind, MAX_DAILY_RUN_LIMIT, MAX_DAILY_TOKEN_LIMIT, MAX_NOTE_CHARS, NewProject, NoteKind,
    ProjectChanges, ProjectStatus, StoredProject, StoredProjectNote, add_project_note,
    count_project_runs_since, create_project, delete_project, earlier_important_notes,
    find_project, link_project, list_projects, project_for, project_tokens_since,
    recent_project_notes, set_project_link, update_project, workspace_tokens_since,
};
pub use run_event_repository::{
    NewRunEvent, StoredRunEvent, append_run_event, find_run_event, highest_run_event_sequence,
    latest_run_event_payload, read_run_events,
};
pub use run_repository::{
    INTERRUPTED_ERROR_CODE, InterruptedRun, MAX_OBJECTIVE_CHARS, NewRun, StoredRun,
    TerminalTransition, continuable_interrupted_runs, count_working_runs, create_run, find_run,
    read_recent_runs, read_runs_by_objective_prefix, recover_interrupted_runs,
    request_run_cancellation, settle_parked_run_cancelled, settle_run, transition_run,
    withdraw_cancelled_run_approvals,
};
pub use run_transcript_repository::{
    MAX_RUN_TRANSCRIPT_BYTES, delete_run_transcript, load_run_transcript, save_run_transcript,
};
pub use schedule_repository::{
    StoredSchedule, claim_due_schedules, create_schedule, delete_schedule, find_schedule,
    list_schedules, record_schedule_run, record_schedule_skip, set_schedule_enabled,
};
pub use session_repository::{
    API_SESSION_CHANNEL, NewSession, RunAdmissionLimits, SessionTarget, StartRunInput, StartedRun,
    StoredSession, find_session, start_run,
};
pub use skill_repository::{
    StoredSkillRevision, ToolValidator, archive_skill_revision, delete_skill_revision,
    find_skill_revision, find_skill_revision_state, promote_skill_revision, read_skill_revisions,
    read_usable_skill_revisions, read_workspace_skill_revision_states,
    read_workspace_skill_revisions, record_skill_revision, restore_skill_revision,
    supersede_skill_revision,
};
pub use summary_repository::{
    MAX_SUMMARY_PAGE, StoredSummary, archive_session_summaries, read_session_summaries,
    read_session_summary_spans, read_unsummarized_ranges, record_summary,
};
pub use tool_call_repository::{
    CallBinding, CallOrigin, CallTarget, NewToolCall, StoredToolCall, admit_tool_call,
    advance_tool_call, count_tool_calls, find_tool_call, link_tool_call_approval,
    read_run_tool_calls, read_unrepeatable_calls, record_tool_outcome, settle_orphaned_calls,
};
pub use trigger_repository::{
    MAX_TRIGGER_INTERVAL_SECONDS, MAX_TRIGGER_OBJECTIVE_CHARS, MAX_TRIGGER_QUERY_CHARS,
    MAX_TRIGGERS_PER_WORKSPACE, MIN_TRIGGER_INTERVAL_SECONDS, NewTrigger, StoredTrigger,
    TriggerKind, claim_due_triggers, claim_trigger_event, count_trigger_runs_since, create_trigger,
    delete_trigger, find_trigger, find_webhook_by_secret_hash, list_triggers,
    mark_trigger_baselined, record_trigger_poll, record_trigger_run, set_trigger_enabled,
    valid_trigger_folder,
};
pub use workspace_repository::{DataPolicy, NewWorkspace, WorkspaceMode, record_workspace};
