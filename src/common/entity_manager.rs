use std::collections::HashMap;
use std::sync::Arc;
use parking_lot::RwLock;

/// Opaque handle with staleness detection. The `id` field is the external-facing
/// entity ID (e.g., char_id). The `generation` field detects use-after-remove.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct EntityId {
    pub id: u32,
    pub(crate) generation: u32,
}

/// Centralized entity registry. Owns `Arc<RwLock<T>>` per entity, keyed by
/// external `u32` ID. Generational tracking detects stale handles.
pub struct EntityManager<T> {
    slots: HashMap<u32, Slot<T>>,
}

struct Slot<T> {
    entity: Option<Arc<RwLock<T>>>,
    generation: u32,
}

impl<T> Default for EntityManager<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> EntityManager<T> {
    pub fn new() -> Self {
        Self {
            slots: HashMap::new(),
        }
    }

    /// Insert an entity with a known external ID. Returns an EntityId handle.
    /// If an entity with this ID already exists (or was previously removed),
    /// the generation is bumped so old handles become stale.
    pub fn insert(&mut self, id: u32, entity: T) -> EntityId {
        let generation = self.slots
            .get(&id)
            .map(|s| next_generation(s.generation))
            .unwrap_or(1);

        self.slots.insert(id, Slot {
            entity: Some(Arc::new(RwLock::new(entity))),
            generation,
        });

        EntityId { id, generation }
    }

    /// Get by EntityId (generation-checked). Returns None if stale or removed.
    pub fn get(&self, eid: EntityId) -> Option<Arc<RwLock<T>>> {
        self.slots.get(&eid.id).and_then(|slot| {
            if slot.generation == eid.generation {
                slot.entity.as_ref().map(Arc::clone)
            } else {
                None
            }
        })
    }

    /// Get by raw u32 ID (no generation check). Migration compatibility path.
    /// Returns None if removed or never inserted.
    pub fn get_by_id(&self, id: u32) -> Option<Arc<RwLock<T>>> {
        self.slots.get(&id).and_then(|slot| slot.entity.as_ref().map(Arc::clone))
    }

    /// Remove by external ID. Bumps generation so stale handles return None.
    /// Returns the Arc if it existed.
    pub fn remove(&mut self, id: u32) -> Option<Arc<RwLock<T>>> {
        if let Some(slot) = self.slots.get_mut(&id) {
            slot.generation = next_generation(slot.generation);
            slot.entity.take()
        } else {
            None
        }
    }

    /// Check if an EntityId handle is still valid.
    pub fn is_alive(&self, eid: EntityId) -> bool {
        self.slots.get(&eid.id)
            .map(|slot| slot.generation == eid.generation && slot.entity.is_some())
            .unwrap_or(false)
    }

    /// Iterate over all live entities (skips removed slots).
    pub fn iter(&self) -> impl Iterator<Item = (u32, &Arc<RwLock<T>>)> {
        self.slots.iter().filter_map(|(&id, slot)| {
            slot.entity.as_ref().map(|arc| (id, arc))
        })
    }

    /// Number of live entities.
    pub fn len(&self) -> usize {
        self.slots.values().filter(|s| s.entity.is_some()).count()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Insert a pre-wrapped Arc<RwLock<T>>. Used for large structs that
    /// must be heap-allocated before wrapping (e.g., via `box_into_arc_rwlock`).
    /// Returns an EntityId handle. Bumps generation if the slot was previously used.
    pub fn insert_arc(&mut self, id: u32, arc: Arc<RwLock<T>>) -> EntityId {
        let generation = self.slots
            .get(&id)
            .map(|s| next_generation(s.generation))
            .unwrap_or(1);

        self.slots.insert(id, Slot {
            entity: Some(arc),
            generation,
        });

        EntityId { id, generation }
    }

    /// Remove all entities. All existing EntityId handles become stale.
    pub fn clear(&mut self) {
        self.slots.clear();
    }

    pub fn entity_id_for(&self, id: u32) -> Option<EntityId> {
        self.slots.get(&id).and_then(|slot| {
            slot.entity.as_ref().map(|_| EntityId { id, generation: slot.generation })
        })
    }

}

/// Advance generation, skipping zero (reserved as "never valid").
fn next_generation(current: u32) -> u32 {
    let next = current.wrapping_add(1);
    if next == 0 { 1 } else { next }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_and_get() {
        let mut em = EntityManager::<String>::new();
        let id = em.insert(1, "alice".to_string());
        let arc = em.get(id).unwrap();
        assert_eq!(*arc.read(), "alice");
    }

    #[test]
    fn get_by_id_works() {
        let mut em = EntityManager::<String>::new();
        em.insert(42, "bob".to_string());
        let arc = em.get_by_id(42).unwrap();
        assert_eq!(*arc.read(), "bob");
    }

    #[test]
    fn get_by_id_missing_returns_none() {
        let em = EntityManager::<String>::new();
        assert!(em.get_by_id(999).is_none());
    }

    #[test]
    fn remove_invalidates_handle() {
        let mut em = EntityManager::<String>::new();
        let id = em.insert(1, "alice".to_string());
        assert!(em.is_alive(id));
        let removed = em.remove(1);
        assert!(removed.is_some());
        assert!(!em.is_alive(id));
        assert!(em.get(id).is_none());
    }

    #[test]
    fn remove_returns_none_for_unknown() {
        let mut em = EntityManager::<String>::new();
        assert!(em.remove(999).is_none());
    }

    #[test]
    fn stale_handle_after_reinsert() {
        let mut em = EntityManager::<String>::new();
        let old_id = em.insert(1, "alice".to_string());
        em.remove(1);
        let new_id = em.insert(1, "charlie".to_string());
        assert!(em.get(old_id).is_none());
        let arc = em.get(new_id).unwrap();
        assert_eq!(*arc.read(), "charlie");
        let arc2 = em.get_by_id(1).unwrap();
        assert_eq!(*arc2.read(), "charlie");
    }

    #[test]
    fn generation_never_zero() {
        let mut em = EntityManager::<String>::new();
        for _ in 0..10 {
            em.insert(1, "x".to_string());
            em.remove(1);
        }
        let id = em.insert(1, "final".to_string());
        assert!(em.is_alive(id));
    }

    #[test]
    fn iter_returns_live_entities() {
        let mut em = EntityManager::<String>::new();
        em.insert(1, "a".to_string());
        em.insert(2, "b".to_string());
        em.insert(3, "c".to_string());
        em.remove(2);
        let ids: Vec<u32> = em.iter().map(|(id, _)| id).collect();
        assert_eq!(ids.len(), 2);
        assert!(ids.contains(&1));
        assert!(ids.contains(&3));
    }

    #[test]
    fn arc_survives_remove() {
        let mut em = EntityManager::<String>::new();
        em.insert(1, "alice".to_string());
        let arc = em.get_by_id(1).unwrap();
        em.remove(1);
        assert_eq!(*arc.read(), "alice");
        assert!(em.get_by_id(1).is_none());
    }

    #[test]
    fn insert_overwrite_stales_old_handle() {
        let mut em = EntityManager::<String>::new();
        let old_id = em.insert(1, "alice".to_string());
        let new_id = em.insert(1, "bob".to_string());
        assert!(em.get(old_id).is_none());
        assert!(!em.is_alive(old_id));
        let arc = em.get(new_id).unwrap();
        assert_eq!(*arc.read(), "bob");
    }

    #[test]
    fn insert_arc_works() {
        let mut em = EntityManager::<String>::new();
        let arc = Arc::new(RwLock::new("pre-wrapped".to_string()));
        let id = em.insert_arc(1, arc.clone());
        assert!(em.is_alive(id));
        let got = em.get(id).unwrap();
        assert_eq!(*got.read(), "pre-wrapped");
        // The returned Arc is the SAME allocation (not a copy)
        assert!(Arc::ptr_eq(&arc, &got));
    }

    #[test]
    fn insert_arc_bumps_generation() {
        let mut em = EntityManager::<String>::new();
        let old_id = em.insert(1, "first".to_string());
        em.remove(1);
        let arc = Arc::new(RwLock::new("second".to_string()));
        let new_id = em.insert_arc(1, arc);
        assert!(!em.is_alive(old_id));
        assert!(em.is_alive(new_id));
        assert_ne!(old_id, new_id);
    }

    #[test]
    fn clear_removes_all() {
        let mut em = EntityManager::<String>::new();
        let id1 = em.insert(1, "a".to_string());
        let id2 = em.insert(2, "b".to_string());
        em.clear();
        assert!(!em.is_alive(id1));
        assert!(!em.is_alive(id2));
        assert!(em.is_empty());
    }

    #[test]
    fn len_and_is_empty() {
        let mut em = EntityManager::<u32>::new();
        assert!(em.is_empty());
        assert_eq!(em.len(), 0);
        em.insert(1, 10);
        assert!(!em.is_empty());
        assert_eq!(em.len(), 1);
        em.remove(1);
        assert!(em.is_empty());
    }
}
