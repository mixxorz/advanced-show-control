use std::collections::HashSet;
use uuid::Uuid;

use super::{CueEntry, CueList, CueListDocument};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CueListsState {
    document: CueListDocument,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClearedCueEntry {
    pub cue_list_id: Uuid,
    pub cue_entry_id: Uuid,
    pub scene_internal_id: Uuid,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CueListReconciliation {
    pub active_cue_list_cleared: bool,
    pub cued_entry_cleared: Option<ClearedCueEntry>,
}

impl CueListsState {
    pub fn document(&self) -> CueListDocument {
        self.document.clone()
    }

    pub fn replace_document(
        &mut self,
        document: CueListDocument,
        valid_scene_ids: impl IntoIterator<Item = Uuid>,
    ) -> CueListReconciliation {
        self.document = document;
        self.reconcile(valid_scene_ids)
    }

    pub fn create_cue_list(&mut self, name: String) -> Result<CueList, String> {
        let name = normalized_name(name)?;
        let list = CueList {
            id: Uuid::new_v4(),
            name,
            entries: Vec::new(),
        };
        self.document.active_cue_list_id = Some(list.id);
        self.document.cued_cue_entry_id = None;
        self.document.cue_lists.push(list.clone());
        Ok(list)
    }

    pub fn rename_cue_list(&mut self, cue_list_id: Uuid, name: String) -> Result<(), String> {
        let name = normalized_name(name)?;
        let list = self
            .cue_list_mut(cue_list_id)
            .ok_or_else(|| "Cue list not found".to_string())?;
        list.name = name;
        Ok(())
    }

    pub fn delete_cue_list(&mut self, cue_list_id: Uuid) -> Result<(), String> {
        let index = self
            .document
            .cue_lists
            .iter()
            .position(|list| list.id == cue_list_id)
            .ok_or_else(|| "Cue list not found".to_string())?;
        self.document.cue_lists.remove(index);
        if self.document.active_cue_list_id == Some(cue_list_id) {
            self.document.active_cue_list_id = None;
            self.document.cued_cue_entry_id = None;
        }
        Ok(())
    }

    pub fn reorder_cue_lists(&mut self, ordered_ids: Vec<Uuid>) -> Result<(), String> {
        if ordered_ids.len() != self.document.cue_lists.len() {
            return Err("Cue list reorder blocked: ordered IDs do not match cue lists".to_string());
        }
        let mut reordered = Vec::with_capacity(self.document.cue_lists.len());
        for id in ordered_ids {
            let index = self
                .document
                .cue_lists
                .iter()
                .position(|list| list.id == id)
                .ok_or_else(|| "Cue list reorder blocked: cue list not found".to_string())?;
            reordered.push(self.document.cue_lists.remove(index));
        }
        self.document.cue_lists = reordered;
        Ok(())
    }

    pub fn set_active_cue_list(&mut self, cue_list_id: Option<Uuid>) -> Result<bool, String> {
        if let Some(id) = cue_list_id
            && !self.document.cue_lists.iter().any(|list| list.id == id)
        {
            return Err("Cue list not found".to_string());
        }
        if self.document.active_cue_list_id == cue_list_id {
            return Ok(false);
        }
        self.document.active_cue_list_id = cue_list_id;
        self.document.cued_cue_entry_id = None;
        Ok(true)
    }

    pub fn add_scene_to_active_cue_list(
        &mut self,
        scene_internal_id: Uuid,
        insert_index: usize,
    ) -> Result<CueEntry, String> {
        let active_id = self
            .document
            .active_cue_list_id
            .ok_or_else(|| "Cue entry add blocked: no active cue list".to_string())?;
        let list = self
            .cue_list_mut(active_id)
            .ok_or_else(|| "Cue entry add blocked: active cue list not found".to_string())?;
        let entry = CueEntry {
            id: Uuid::new_v4(),
            scene_internal_id,
        };
        let index = insert_index.min(list.entries.len());
        list.entries.insert(index, entry.clone());
        Ok(entry)
    }

    pub fn remove_cue_entry(&mut self, cue_entry_id: Uuid) -> Result<(), String> {
        let list = self
            .active_cue_list_mut()
            .ok_or_else(|| "Cue entry remove blocked: no active cue list".to_string())?;
        let index = list
            .entries
            .iter()
            .position(|entry| entry.id == cue_entry_id)
            .ok_or_else(|| "Cue entry not found".to_string())?;
        list.entries.remove(index);
        if self.document.cued_cue_entry_id == Some(cue_entry_id) {
            self.document.cued_cue_entry_id = None;
        }
        Ok(())
    }

    pub fn reorder_cue_entries(&mut self, ordered_entry_ids: Vec<Uuid>) -> Result<(), String> {
        let list = self
            .active_cue_list_mut()
            .ok_or_else(|| "Cue entry reorder blocked: no active cue list".to_string())?;
        if ordered_entry_ids.len() != list.entries.len() {
            return Err("Cue entry reorder blocked: ordered IDs do not match entries".to_string());
        }
        let mut reordered = Vec::with_capacity(list.entries.len());
        for id in ordered_entry_ids {
            let index = list
                .entries
                .iter()
                .position(|entry| entry.id == id)
                .ok_or_else(|| "Cue entry reorder blocked: cue entry not found".to_string())?;
            reordered.push(list.entries.remove(index));
        }
        list.entries = reordered;
        Ok(())
    }

    pub fn cue_entry(&mut self, cue_entry_id: Option<Uuid>) -> Result<(), String> {
        if let Some(id) = cue_entry_id {
            let list = self
                .active_cue_list()
                .ok_or_else(|| "Cue blocked: no active cue list".to_string())?;
            if !list.entries.iter().any(|entry| entry.id == id) {
                return Err("Cue blocked: cue entry is not in the active cue list".to_string());
            }
        }
        self.document.cued_cue_entry_id = cue_entry_id;
        Ok(())
    }

    pub fn cued_entry(&self) -> Result<CueEntry, String> {
        let cued_id = self
            .document
            .cued_cue_entry_id
            .ok_or_else(|| "Cue recall blocked: no cued cue entry".to_string())?;
        let list = self
            .active_cue_list()
            .ok_or_else(|| "Cue recall blocked: no active cue list".to_string())?;
        list.entries
            .iter()
            .find(|entry| entry.id == cued_id)
            .cloned()
            .ok_or_else(|| {
                "Cue recall blocked: cued entry is not in the active cue list".to_string()
            })
    }

    pub fn advance_after_successful_recall(&mut self) -> Result<CueEntry, String> {
        let cued_id = self
            .document
            .cued_cue_entry_id
            .ok_or_else(|| "Cue recall blocked: no cued cue entry".to_string())?;
        let list = self
            .active_cue_list()
            .ok_or_else(|| "Cue recall blocked: no active cue list".to_string())?;
        let index = list
            .entries
            .iter()
            .position(|entry| entry.id == cued_id)
            .ok_or_else(|| {
                "Cue recall blocked: cued entry is not in the active cue list".to_string()
            })?;
        let recalled = list.entries[index].clone();
        self.document.cued_cue_entry_id = list.entries.get(index + 1).map(|entry| entry.id);
        Ok(recalled)
    }

    fn clear_invalid_cue(&mut self) {
        if let Some(cued_id) = self.document.cued_cue_entry_id
            && self
                .active_cue_list()
                .is_none_or(|list| !list.entries.iter().any(|entry| entry.id == cued_id))
        {
            self.document.cued_cue_entry_id = None;
        }
    }

    pub fn reconcile(
        &mut self,
        valid_scene_ids: impl IntoIterator<Item = Uuid>,
    ) -> CueListReconciliation {
        let valid_scene_ids = valid_scene_ids.into_iter().collect::<HashSet<_>>();
        let mut result = CueListReconciliation::default();
        let active_id = self.document.active_cue_list_id;
        let active =
            active_id.and_then(|id| self.document.cue_lists.iter().find(|list| list.id == id));

        if let Some(_active_id) = active_id
            && active.is_none()
        {
            self.document.active_cue_list_id = None;
            self.document.cued_cue_entry_id = None;
            result.active_cue_list_cleared = true;
            return result;
        }

        if let (Some(list), Some(cued_id)) = (active, self.document.cued_cue_entry_id)
            && let Some(entry) = list.entries.iter().find(|entry| entry.id == cued_id)
            && !valid_scene_ids.contains(&entry.scene_internal_id)
        {
            result.cued_entry_cleared = Some(ClearedCueEntry {
                cue_list_id: list.id,
                cue_entry_id: entry.id,
                scene_internal_id: entry.scene_internal_id,
            });
            self.document.cued_cue_entry_id = None;
        } else {
            self.clear_invalid_cue();
        }

        result
    }

    fn active_cue_list(&self) -> Option<&CueList> {
        self.document
            .active_cue_list_id
            .and_then(|id| self.document.cue_lists.iter().find(|list| list.id == id))
    }

    fn active_cue_list_mut(&mut self) -> Option<&mut CueList> {
        let id = self.document.active_cue_list_id?;
        self.cue_list_mut(id)
    }

    fn cue_list_mut(&mut self, id: Uuid) -> Option<&mut CueList> {
        self.document
            .cue_lists
            .iter_mut()
            .find(|list| list.id == id)
    }
}

fn normalized_name(name: String) -> Result<String, String> {
    let name = name.trim().to_string();
    if name.is_empty() {
        Err("Cue list name cannot be blank".to_string())
    } else {
        Ok(name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(value: u128) -> Uuid {
        Uuid::from_u128(value)
    }

    fn state_with_two_entries() -> CueListsState {
        let mut state = CueListsState::default();
        let first = state.create_cue_list("First".to_string()).unwrap().id;
        let second = state.create_cue_list("Second".to_string()).unwrap().id;
        let entry = state.add_scene_to_active_cue_list(id(10), 0).unwrap();
        state.cue_entry(Some(entry.id)).unwrap();
        state.set_active_cue_list(Some(first)).unwrap();
        state.set_active_cue_list(Some(second)).unwrap();
        state
    }

    #[test]
    fn creating_a_cue_list_makes_it_active_and_clears_cued_entry() {
        let mut state = CueListsState::default();
        state.create_cue_list("Existing".to_string()).unwrap();
        let entry = state.add_scene_to_active_cue_list(id(10), 0).unwrap();
        state.cue_entry(Some(entry.id)).unwrap();

        let created = state.create_cue_list(" Main ".to_string()).unwrap();

        assert_eq!(state.document().cue_lists.len(), 2);
        assert_eq!(state.document().cue_lists[1].name, "Main");
        assert_eq!(state.document().active_cue_list_id, Some(created.id));
        assert_eq!(state.document().cued_cue_entry_id, None);
    }

    #[test]
    fn adding_scene_to_active_cue_list_inserts_at_requested_position() {
        let mut state = CueListsState::default();
        state.create_cue_list("List".to_string()).unwrap();

        let first = state.add_scene_to_active_cue_list(id(10), 0).unwrap();
        let second = state.add_scene_to_active_cue_list(id(11), 0).unwrap();

        let document = state.document();
        let entries = &document.cue_lists[0].entries;
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].id, second.id);
        assert_eq!(entries[0].scene_internal_id, id(11));
        assert_eq!(entries[1].id, first.id);
        assert_eq!(entries[1].scene_internal_id, id(10));
    }

    #[test]
    fn adding_scene_to_active_cue_list_clamps_out_of_range_insert_index_to_end() {
        let mut state = CueListsState::default();
        state.create_cue_list("List".to_string()).unwrap();

        let first = state.add_scene_to_active_cue_list(id(10), 0).unwrap();
        let second = state
            .add_scene_to_active_cue_list(id(11), usize::MAX)
            .unwrap();

        let document = state.document();
        let entries = &document.cue_lists[0].entries;
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].id, first.id);
        assert_eq!(entries[0].scene_internal_id, id(10));
        assert_eq!(entries[1].id, second.id);
        assert_eq!(entries[1].scene_internal_id, id(11));
    }

    #[test]
    fn changing_active_cue_list_clears_cued_entry() {
        let mut state = CueListsState::default();
        let first = state.create_cue_list("First".to_string()).unwrap().id;
        let second = state.create_cue_list("Second".to_string()).unwrap().id;
        let entry = state.add_scene_to_active_cue_list(id(10), 0).unwrap();
        state.cue_entry(Some(entry.id)).unwrap();

        state.set_active_cue_list(Some(first)).unwrap();
        state.set_active_cue_list(Some(second)).unwrap();

        assert_eq!(state.document().active_cue_list_id, Some(second));
        assert_eq!(state.document().cued_cue_entry_id, None);
    }

    #[test]
    fn deleting_active_cue_list_clears_active_and_cued_state() {
        let mut state = CueListsState::default();
        let list = state.create_cue_list("List".to_string()).unwrap().id;
        let entry = state.add_scene_to_active_cue_list(id(10), 0).unwrap();
        state.cue_entry(Some(entry.id)).unwrap();

        state.delete_cue_list(list).unwrap();

        assert!(state.document().cue_lists.is_empty());
        assert_eq!(state.document().active_cue_list_id, None);
        assert_eq!(state.document().cued_cue_entry_id, None);
    }

    #[test]
    fn reordering_cue_lists_preserves_active_list_by_id_and_changes_order() {
        let mut state = CueListsState::default();
        let first = state.create_cue_list("First".to_string()).unwrap().id;
        let second = state.create_cue_list("Second".to_string()).unwrap().id;
        let third = state.create_cue_list("Third".to_string()).unwrap().id;

        state.set_active_cue_list(Some(second)).unwrap();
        state.reorder_cue_lists(vec![third, first, second]).unwrap();

        let document = state.document();
        assert_eq!(document.active_cue_list_id, Some(second));
        assert_eq!(
            document
                .cue_lists
                .iter()
                .map(|list| list.id)
                .collect::<Vec<_>>(),
            vec![third, first, second]
        );
    }

    #[test]
    fn cueing_entry_from_inactive_list_is_rejected() {
        let mut state = CueListsState::default();
        state.create_cue_list("First".to_string()).unwrap();
        let first_entry = state.add_scene_to_active_cue_list(id(10), 0).unwrap();
        let second = state.create_cue_list("Second".to_string()).unwrap().id;
        state.set_active_cue_list(Some(second)).unwrap();

        let err = state.cue_entry(Some(first_entry.id)).unwrap_err();

        assert_eq!(state.document().active_cue_list_id, Some(second));
        assert_eq!(err, "Cue blocked: cue entry is not in the active cue list");
        assert_eq!(state.document().cued_cue_entry_id, None);
    }

    #[test]
    fn reordering_cue_entries_preserves_cued_entry_by_id_and_changes_order() {
        let mut state = CueListsState::default();
        state.create_cue_list("List".to_string()).unwrap();
        let first = state.add_scene_to_active_cue_list(id(10), 0).unwrap();
        let second = state.add_scene_to_active_cue_list(id(11), 1).unwrap();
        let third = state.add_scene_to_active_cue_list(id(12), 2).unwrap();

        state.cue_entry(Some(second.id)).unwrap();
        state
            .reorder_cue_entries(vec![third.id, first.id, second.id])
            .unwrap();

        let document = state.document();
        let entries = &document.cue_lists[0].entries;
        assert_eq!(document.cued_cue_entry_id, Some(second.id));
        assert_eq!(
            entries.iter().map(|entry| entry.id).collect::<Vec<_>>(),
            vec![third.id, first.id, second.id]
        );
    }

    #[test]
    fn successful_recall_advances_to_next_entry_or_clears_at_end() {
        let mut state = CueListsState::default();
        state.create_cue_list("List".to_string()).unwrap();
        let first = state.add_scene_to_active_cue_list(id(10), 0).unwrap();
        let second = state.add_scene_to_active_cue_list(id(11), 1).unwrap();
        state.cue_entry(Some(first.id)).unwrap();

        let recalled = state.advance_after_successful_recall().unwrap();

        assert_eq!(recalled.id, first.id);
        assert_eq!(state.document().cued_cue_entry_id, Some(second.id));
        let recalled = state.advance_after_successful_recall().unwrap();
        assert_eq!(recalled.id, second.id);
        assert_eq!(state.document().cued_cue_entry_id, None);
    }

    #[test]
    fn replacing_document_clears_invalid_active_and_cued_ids() {
        let mut state = CueListsState::default();
        let document = CueListDocument {
            cue_lists: vec![],
            active_cue_list_id: Some(id(1)),
            cued_cue_entry_id: Some(id(2)),
        };

        let result = state.replace_document(document, [id(10)]);

        assert!(result.active_cue_list_cleared);
        assert!(result.cued_entry_cleared.is_none());
        assert_eq!(state.document().active_cue_list_id, None);
        assert_eq!(state.document().cued_cue_entry_id, None);
    }

    #[test]
    fn reconciliation_preserves_missing_entry_but_clears_current_cue() {
        let list_id = id(1);
        let entry_id = id(2);
        let missing_scene_id = id(3);
        let mut state = CueListsState::default();
        let document = CueListDocument {
            cue_lists: vec![CueList {
                id: list_id,
                name: "Main".to_string(),
                entries: vec![CueEntry {
                    id: entry_id,
                    scene_internal_id: missing_scene_id,
                }],
            }],
            active_cue_list_id: Some(list_id),
            cued_cue_entry_id: Some(entry_id),
        };

        let result = state.replace_document(document, [id(99)]);

        assert_eq!(
            result.cued_entry_cleared.unwrap().scene_internal_id,
            missing_scene_id
        );
        assert_eq!(state.document().cue_lists[0].entries.len(), 1);
        assert_eq!(state.document().cued_cue_entry_id, None);
    }

    #[test]
    fn setting_already_active_list_preserves_cue_and_reports_unchanged() {
        let mut state = state_with_two_entries();
        let active = state.document().active_cue_list_id.unwrap();
        let cued = state.document().cued_cue_entry_id;

        let changed = state.set_active_cue_list(Some(active)).unwrap();

        assert!(!changed);
        assert_eq!(state.document().cued_cue_entry_id, cued);
    }
}
