use super::cache::{CACHE_CAPACITY, Cache, SlotState};

#[test]
fn peek_does_not_promote_a_slot_before_eviction() {
    let mut cache = Cache::default();
    for spine_idx in 0..=CACHE_CAPACITY as u32 {
        cache.insert_laying(spine_idx, 0);
        *cache.state_mut(spine_idx, 0).unwrap() = SlotState::Failed("失敗".to_string());
    }

    let _ = cache.get(0, 0);
    let _ = cache.peek(1, 0);
    cache.evict(CACHE_CAPACITY as u32);

    assert!(cache.peek(0, 0).is_some());
    assert!(cache.peek(1, 0).is_none());
}
