use uuid::Uuid;

/// A presentation-only item in the ordered cue sequence, never persisted as a cue.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct CueMarker {
    sequence: Vec<Option<Uuid>>,
    pending_move: Option<(Uuid, Option<Uuid>)>,
    pending_marker_move: Option<Uuid>,
    pending_insert: Option<usize>,
}

impl CueMarker {
    pub fn new(ids: &[Uuid], index: usize) -> Self {
        let mut sequence: Vec<_> = ids.iter().copied().map(Some).collect();
        sequence.insert(index.min(sequence.len()), None);
        Self {
            sequence,
            pending_move: None,
            pending_marker_move: None,
            pending_insert: None,
        }
    }

    /// Record the dragged entry and the entry it will precede (`None` means append).
    /// This is intent, not an optimistic update: unchanged projections leave the marker alone.
    pub fn record_move(&mut self, from: Uuid, before: Option<Uuid>) {
        self.pending_move = Some((from, before));
    }

    /// @cc [owner:mixxorz,label:product] insert-marker-drop-side
    /// A cue dropped onto the Insert panel MUST be placed immediately before the marker;
    /// when its real-cue order is unchanged, the presentation marker MUST still move.
    pub fn record_move_to_marker(&mut self, from: Uuid) {
        if !self.sequence.contains(&Some(from)) {
            return;
        }
        let mut expected = self.real_ids();
        expected.retain(|id| *id != from);
        let marker_index = self.index();
        let destination = self.sequence[..marker_index]
            .iter()
            .flatten()
            .filter(|id| **id != from)
            .count();
        expected.insert(destination, from);
        if expected == self.real_ids() {
            self.sequence.retain(|item| *item != Some(from));
            let marker = self.index();
            self.sequence.insert(marker, Some(from));
        } else {
            self.pending_marker_move = Some(from);
        }
    }

    /// Record a scene drop at the real-cue gap. At the marker gap the new cue goes before it.
    pub fn record_insert(&mut self, index: usize) {
        self.pending_insert = Some(index);
    }

    pub fn is_pending(&self) -> bool {
        self.pending_move.is_some()
            || self.pending_marker_move.is_some()
            || self.pending_insert.is_some()
    }

    /// Discard an intent when its command fails; no projected edit has been applied.
    pub fn clear_pending(&mut self) {
        self.pending_move = None;
        self.pending_marker_move = None;
        self.pending_insert = None;
    }

    pub fn reconcile(&mut self, ids: &[Uuid]) {
        let previous = self.real_ids();
        if previous == ids {
            return;
        }
        self.sequence
            .retain(|item| item.is_none_or(|id| ids.contains(&id)));

        if let Some(from) = self.pending_marker_move.take() {
            let mut expected = previous.clone();
            expected.retain(|id| *id != from);
            let destination = self.sequence[..self.index()]
                .iter()
                .flatten()
                .filter(|id| **id != from)
                .count();
            if previous.contains(&from) {
                expected.insert(destination, from);
                let survivors: Vec<_> = ids
                    .iter()
                    .copied()
                    .filter(|id| previous.contains(id))
                    .collect();
                if survivors == expected {
                    self.sequence.retain(|item| *item != Some(from));
                    let marker = self.index();
                    self.sequence.insert(marker, Some(from));
                }
            }
        }

        if let Some((from, before)) = self.pending_move.take() {
            let mut expected = previous.clone();
            if let Some(position) = expected.iter().position(|id| *id == from) {
                expected.remove(position);
                if let Some(destination) = before {
                    if let Some(position) = expected.iter().position(|id| *id == destination) {
                        expected.insert(position, from);
                    }
                } else {
                    expected.push(from);
                }
                // Compare surviving identities so a move and an insertion may arrive together.
                let survivors: Vec<_> = ids
                    .iter()
                    .copied()
                    .filter(|id| previous.contains(id))
                    .collect();
                if survivors == expected {
                    self.sequence.retain(|item| *item != Some(from));
                    let position = before
                        .and_then(|id| self.sequence.iter().position(|item| *item == Some(id)))
                        .unwrap_or(self.sequence.len());
                    self.sequence.insert(position, Some(from));
                }
            }
        }

        let added: Vec<_> = ids
            .iter()
            .copied()
            .filter(|id| !previous.contains(id))
            .collect();
        let insertion_matches = self
            .pending_insert
            .take()
            .is_some_and(|index| added.len() == 1 && ids.get(index) == added.first());
        for (index, id) in ids.iter().enumerate() {
            if self.sequence.contains(&Some(*id)) {
                continue;
            }
            let next = ids[index + 1..].iter().find_map(|candidate| {
                self.sequence
                    .iter()
                    .position(|item| *item == Some(*candidate))
            });
            let position = if insertion_matches && Some(id) == added.first() {
                // The projected insertion index denotes a gap among real entries, not a
                // position in the marker-augmented sequence.
                self.sequence
                    .iter()
                    .position(Option::is_none)
                    .filter(|_| index == self.index())
                    .or(next)
                    .unwrap_or(self.sequence.len())
            } else {
                next.unwrap_or(self.sequence.len())
            };
            self.sequence.insert(position, Some(*id));
        }
        // A projection without recorded drag intent must still display its actual cue order.
        // Preserve the marker's numeric gap when the move cannot be identified unambiguously.
        if self.real_ids() != ids {
            let gap = self.index().min(ids.len());
            *self = Self::new(ids, gap);
        }
    }

    pub fn index(&self) -> usize {
        self.sequence.iter().position(Option::is_none).unwrap_or(0)
    }

    fn real_ids(&self) -> Vec<Uuid> {
        self.sequence.iter().flatten().copied().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(values: &[u128]) -> Vec<Uuid> {
        values.iter().map(|id| Uuid::from_u128(*id)).collect()
    }

    #[test]
    fn insertion_before_first_cue_leaves_marker_between_original_neighbors() {
        let mut marker = CueMarker::new(&ids(&[1, 2]), 1);
        marker.record_insert(0);
        marker.reconcile(&ids(&[0, 1, 2]));
        assert_eq!(marker.index(), 2);
    }

    #[test]
    fn inserting_at_marker_places_new_cue_before_panel() {
        let mut marker = CueMarker::new(&ids(&[1, 2]), 1);
        marker.record_insert(1);
        marker.reconcile(&ids(&[1, 3, 2]));
        assert_eq!(marker.index(), 2);
    }

    #[test]
    fn moving_either_neighbor_away_does_not_take_marker_with_it() {
        let mut left = CueMarker::new(&ids(&[1, 2, 3]), 1);
        left.record_move(ids(&[1])[0], None);
        left.reconcile(&ids(&[2, 3, 1]));
        assert_eq!(left.index(), 0);

        let mut right = CueMarker::new(&ids(&[1, 2, 3]), 1);
        right.record_move(ids(&[2])[0], None);
        right.reconcile(&ids(&[1, 3, 2]));
        assert_eq!(right.index(), 1);
    }

    #[test]
    fn drag_intent_disambiguates_swaps() {
        let mut left = CueMarker::new(&ids(&[1, 2]), 1);
        left.record_move(ids(&[1])[0], None);
        left.reconcile(&ids(&[2, 1]));
        assert_eq!(left.index(), 0);
        let mut right = CueMarker::new(&ids(&[1, 2]), 1);
        right.record_move(ids(&[2])[0], Some(ids(&[1])[0]));
        right.reconcile(&ids(&[2, 1]));
        assert_eq!(right.index(), 2);
    }

    #[test]
    fn repeated_snapshot_and_rejected_move_leave_marker_unchanged() {
        let mut marker = CueMarker::new(&ids(&[1, 2]), 1);
        marker.record_move(ids(&[1])[0], None);
        marker.reconcile(&ids(&[1, 2]));
        assert_eq!(marker.index(), 1);
        assert!(marker.is_pending());
        marker.reconcile(&ids(&[2, 1]));
        assert!(!marker.is_pending());
        assert_eq!(marker.index(), 0);
        marker.reconcile(&ids(&[2, 1]));
        assert_eq!(marker.index(), 0);
    }

    #[test]
    fn drop_onto_marker_waits_for_changed_projection() {
        let mut marker = CueMarker::new(&ids(&[1, 2, 3]), 1);
        marker.record_move_to_marker(ids(&[3])[0]);
        assert_eq!(marker.index(), 1);
        marker.reconcile(&ids(&[1, 2, 3]));
        assert_eq!(marker.index(), 1);
        marker.reconcile(&ids(&[1, 3, 2]));
        assert_eq!(marker.index(), 2);
    }

    #[test]
    fn adjacent_drop_onto_marker_moves_it_without_document_change() {
        let mut marker = CueMarker::new(&ids(&[1, 2]), 1);
        marker.record_move_to_marker(ids(&[2])[0]);
        assert_eq!(marker.index(), 2);
        marker.reconcile(&ids(&[1, 2]));
        assert_eq!(marker.index(), 2);
    }

    #[test]
    fn rejected_insert_does_not_affect_later_unrelated_insert() {
        let mut marker = CueMarker::new(&ids(&[1, 2]), 1);
        marker.record_insert(1);
        marker.clear_pending();
        marker.reconcile(&ids(&[1, 3, 2]));
        assert_eq!(marker.index(), 1);
    }

    #[test]
    fn coalesced_insertion_and_move_preserve_local_marker() {
        let mut marker = CueMarker::new(&ids(&[1, 2, 3]), 1);
        marker.record_move(ids(&[1])[0], None);
        marker.record_insert(0);
        marker.reconcile(&ids(&[4, 2, 3, 1]));
        assert_eq!(marker.index(), 1);
    }

    #[test]
    fn deletion_removes_only_real_entry() {
        let mut marker = CueMarker::new(&ids(&[1, 2]), 1);
        marker.reconcile(&ids(&[2]));
        assert_eq!(marker.index(), 0);
    }
}
