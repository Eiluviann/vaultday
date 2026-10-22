//! The workspace identifier, shared by the scheduler, the quick-add grammar
//! and the registry. Resolving a folder to a workspace is `vault-base`'s job;
//! here a workspace is only a name to route by.

id_type! {
    /// Identifies a workspace, such as `personal` or `acme`.
    WorkspaceId
}
