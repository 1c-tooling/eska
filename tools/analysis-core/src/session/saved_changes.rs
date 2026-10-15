//! Saved-file events invalidate only current declared inputs and never compete with editor authority.

use std::collections::BTreeSet;

use eska::analysis_core::{InputOrigin, InputState};

use super::{AnalysisSession, SessionRequestError, SourcePath};
use crate::{ModuleState, SessionRevision};

impl AnalysisSession {
    /// Preflight the whole batch, then clear each distinct saved value without reading any file.
    ///
    /// Open overlays and already pending inputs keep their revision. Returned paths are sorted.
    /// # Errors
    /// Rejects stale revisions, undeclared/binary inputs or clock exhaustion before any update.
    pub fn invalidate_saved_inputs(
        &mut self,
        paths: &[SourcePath],
        expected: SessionRevision,
    ) -> Result<Vec<SourcePath>, SessionRequestError> {
        let keys = paths
            .iter()
            .map(|path| self.selected_source(path))
            .collect::<Vec<_>>();
        self.invalidate_saved_inputs_at(&keys, expected)
            .map(|keys| keys.into_iter().map(|key| key.source).collect())
    }

    /// Validate a complete root-qualified event batch before releasing any current saved contribution.
    ///
    /// # Errors
    /// Rejects stale snapshots, unknown roots/inputs, binary bodies or clock exhaustion without mutation.
    pub fn invalidate_saved_inputs_at(
        &mut self,
        paths: &[crate::composition::SourceKey],
        expected: SessionRevision,
    ) -> Result<Vec<crate::composition::SourceKey>, SessionRequestError> {
        self.validate_revision(expected)?;
        let mut changed = BTreeSet::new();
        for key in paths {
            let path = &key.source;
            let root = self.root_inputs(key.root)?;
            let (origin, pending) = if let Some(input) = root.inputs.owner(path) {
                (input.origin, matches!(input.state, InputState::Pending))
            } else if let Some(input) = self.metadata_resource_at(key) {
                (
                    input.origin,
                    matches!(input.state, crate::MetadataResourceState::Pending),
                )
            } else if let Some(input) = self.query_input_at(key) {
                (
                    input.origin,
                    matches!(input.state, crate::QueryTextState::Pending),
                )
            } else {
                let input = self
                    .module_at(key)
                    .ok_or_else(|| SessionRequestError::UnknownModule(path.clone()))?;
                if input.state == ModuleState::Binary {
                    return Err(SessionRequestError::BinaryModule(path.clone()));
                }
                (input.origin, matches!(input.state, ModuleState::Pending))
            };
            if origin == InputOrigin::Saved && !pending {
                changed.insert(key.clone());
            }
        }
        // The metadata clock advances only with session publications, so this also bounds that clock.
        self.revision
            .0
            .checked_add(changed.len() as u64)
            .ok_or(SessionRequestError::RevisionExhausted)?;
        for key in &changed {
            let root = self.root_inputs(key.root)?;
            if root.owners.contains_key(&key.source) {
                let next = self.revision.next()?;
                let root = self.root_inputs_mut(key.root)?;
                root.inputs
                    .invalidate_saved(&key.source, root.inputs.revision())?;
                self.publish_at(key, next)?;
            } else if root.metadata_resources.contains_key(&key.source) {
                self.release_resource_saved_at(key, self.revision())?;
            } else if root.query_inputs.contains_key(&key.source) {
                self.release_query_saved_at(key, self.revision())?;
            } else {
                self.release_module_saved_at(key, self.revision())?;
            }
        }
        Ok(changed.into_iter().collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ModelState, QueryCancellation, QueryError,
        session::tests::{fixture, owner},
    };
    use eska::analysis_core::InputRequest;
    use std::fs;

    /// Keep the selected inputs explicit; no full fixture walk is needed for invalidation.
    fn loaded() -> (
        crate::session::tests::support::TestDirectory,
        AnalysisSession,
    ) {
        let (directory, root) = fixture();
        let mut session = AnalysisSession::open(&root).unwrap();
        let metadata = owner("Catalogs/Sample/object.yaml");
        let module = owner("Catalogs/Sample/Ext/ObjectModule.bsl");
        session
            .load_saved(&metadata, session.revision(), &InputRequest::default())
            .unwrap();
        session
            .load_module_saved(&module, session.revision(), &InputRequest::default())
            .unwrap();
        (directory, session)
    }

    /// Changed saved inputs lose canonical values, fingerprints and readable BSL before any disk read.
    #[test]
    fn saved_batch_clears_both_kernel_inputs_and_source_reservations_without_io() {
        let (_directory, mut session) = loaded();
        let metadata = owner("Catalogs/Sample/object.yaml");
        let module = owner("Catalogs/Sample/Ext/ObjectModule.bsl");
        let retained = session.metadata(&metadata, session.revision()).unwrap();
        fs::remove_file(session.root().join(metadata.as_path())).unwrap();
        fs::remove_file(session.root().join(module.as_path())).unwrap();
        let before = session.revision();
        let changed = session
            .invalidate_saved_inputs(&[module.clone(), metadata.clone(), module.clone()], before)
            .unwrap();
        assert_eq!(changed, vec![module.clone(), metadata.clone()]);
        assert_eq!(session.revision().0, before.0 + 2);
        assert_eq!(session.resources().total_source_bytes, 0);
        assert_eq!(session.owner(&metadata).unwrap().state, ModelState::Pending);
        assert!(
            session
                .resolve(&metadata, session.revision())
                .unwrap()
                .is_none()
        );
        assert_eq!(session.module(&module).unwrap().state, ModuleState::Pending);
        assert!(session.module(&module).unwrap().source_sha256.is_none());
        assert!(matches!(retained.input, InputState::Ready(_)));
        assert!(matches!(
            session.module_diagnostics(
                &module,
                session.revision(),
                &crate::DiagnosticsConfig::default(),
                &QueryCancellation::default()
            ),
            Err(QueryError::ModuleUnavailable { .. })
        ));
        let pending = session.revision();
        assert!(
            session
                .invalidate_saved_inputs(&[metadata.clone(), module.clone()], pending)
                .unwrap()
                .is_empty()
        );
        assert_eq!(session.revision(), pending);
        session
            .load_saved(&metadata, pending, &InputRequest::default())
            .unwrap();
        assert!(matches!(
            session.owner(&metadata).unwrap().state,
            ModelState::Unavailable(_)
        ));
        session
            .load_module_saved(&module, session.revision(), &InputRequest::default())
            .unwrap();
        assert!(matches!(
            session.module(&module).unwrap().state,
            ModuleState::Unavailable(_)
        ));
    }

    /// Invalid batches, stale clocks and exhausted clocks preserve every previously ready input.
    #[test]
    fn whole_saved_batch_is_validated_before_any_invalidation() {
        let (_directory, mut session) = loaded();
        let metadata = owner("Catalogs/Sample/object.yaml");
        let module = owner("Catalogs/Sample/Ext/ObjectModule.bsl");
        let current = session.revision();
        let resources = session.resources();
        let yaml = session.metadata(&metadata, current).unwrap();
        let bsl = session.module(&module).unwrap().clone();
        for bad in [
            owner("Unknown/object.yaml"),
            owner("Catalogs/Sample/Ext/ManagerModule.bin"),
        ] {
            assert!(
                session
                    .invalidate_saved_inputs(&[metadata.clone(), module.clone(), bad], current)
                    .is_err()
            );
            assert_eq!(session.revision(), current);
            assert_eq!(session.metadata(&metadata, current).unwrap(), yaml);
            assert_eq!(session.module(&module), Some(&bsl));
            assert_eq!(session.resources(), resources);
        }
        assert!(matches!(
            session.invalidate_saved_inputs(
                std::slice::from_ref(&metadata),
                SessionRevision(current.0 - 1)
            ),
            Err(SessionRequestError::StaleRevision { .. })
        ));
        session.revision = SessionRevision(u64::MAX - 1);
        assert_eq!(
            session
                .invalidate_saved_inputs(&[metadata.clone(), module.clone()], session.revision()),
            Err(SessionRequestError::RevisionExhausted)
        );
        assert_eq!(session.revision().0, u64::MAX - 1);
        assert_eq!(session.owner(&metadata), Some(&yaml.model));
        assert_eq!(session.module(&module), Some(&bsl));
        assert_eq!(session.resources(), resources);
    }

    /// Open editor values survive external deletions; close reads the actual disk state afterward.
    #[test]
    fn saved_batch_skips_yaml_and_bsl_overlays_without_advancing_the_clock() {
        let (_directory, mut session) = loaded();
        let metadata = owner("Catalogs/Sample/object.yaml");
        let module = owner("Catalogs/Sample/Ext/ObjectModule.bsl");
        let text = fs::read_to_string(session.root().join(metadata.as_path())).unwrap();
        session
            .apply_overlay(
                &metadata,
                session.revision(),
                7,
                &text,
                &InputRequest::default(),
            )
            .unwrap();
        session
            .apply_module_overlay(
                &module,
                session.revision(),
                8,
                "// Я😀 editor\r\n",
                &InputRequest::default(),
            )
            .unwrap();
        fs::remove_file(session.root().join(metadata.as_path())).unwrap();
        fs::remove_file(session.root().join(module.as_path())).unwrap();
        let current = session.revision();
        let yaml = session.metadata(&metadata, current).unwrap();
        let bsl = session.module(&module).unwrap().clone();
        let resources = session.resources();
        assert!(
            session
                .invalidate_saved_inputs(&[metadata.clone(), module.clone()], current)
                .unwrap()
                .is_empty()
        );
        assert_eq!(session.revision(), current);
        assert_eq!(session.metadata(&metadata, current).unwrap(), yaml);
        assert_eq!(session.module(&module), Some(&bsl));
        assert_eq!(session.resources(), resources);
        session
            .close_overlay(&metadata, session.revision(), &InputRequest::default())
            .unwrap();
        session
            .close_module_overlay(&module, session.revision(), &InputRequest::default())
            .unwrap();
        assert!(matches!(
            session.owner(&metadata).unwrap().state,
            ModelState::Unavailable(_)
        ));
        assert!(matches!(
            session.module(&module).unwrap().state,
            ModuleState::Unavailable(_)
        ));
    }
}
