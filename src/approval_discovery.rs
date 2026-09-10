//! Bounded scheduling for authenticated private-room marker discovery.
//! No network calls or marker trust decisions live here.

use std::collections::{BTreeMap, BTreeSet};

pub(crate) const DISCOVERY_BATCH_SIZE: usize = 4;
pub(crate) const DISCOVERY_CONCURRENCY: usize = 2;
pub(crate) const DISCOVERY_TIMEOUT_MS: u64 = 10_000;
const DISCOVERY_RESULT_GRACE_MS: u64 = 5_000;
const POSITIVE_REFRESH_MS: u64 = 120_000;
const NEGATIVE_REFRESH_MS: u64 = 60_000;
const FAILURE_REFRESH_MS: u64 = 30_000;
const MAX_DISCOVERY_ROOMS: usize = 4096;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ApprovalDiscoveryRequest {
    pub account: String,
    pub room_id: String,
    pub generation: u64,
    pub nonce: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DiscoveryOutcome {
    Present,
    Absent,
    Failed,
}

#[derive(Debug)]
pub(crate) struct ApprovalDiscoveryResult {
    pub request: ApprovalDiscoveryRequest,
    pub outcome: DiscoveryOutcome,
    /// True when the authenticated full-state response contained an empty-key
    /// v2 event, even if that event was duplicate, oversized, or malformed.
    pub observed_v2: bool,
    /// Full authenticated state event, including actual sender and event ID.
    pub marker: Option<serde_json::Value>,
}

#[derive(Debug, Default)]
struct RoomSchedule {
    next_due_ms: u64,
    in_flight: Option<(u64, u64)>,
    refresh_pending: bool,
}

#[derive(Debug, Default)]
pub(crate) struct ApprovalDiscoverySchedule {
    account: String,
    generation: u64,
    nonce: u64,
    rooms: BTreeMap<String, RoomSchedule>,
    cursor: Option<String>,
    unavailable: bool,
}

impl ApprovalDiscoverySchedule {
    /// `joined_loaded` is supplied from the currently displayed account's room list.
    /// A removed/rejoined room gets a new nonce, so an old request cannot repopulate it.
    pub fn sync_membership(&mut self, account: &str, joined_loaded: &BTreeSet<String>) -> bool {
        if self.account != account {
            let Some(generation) = self.generation.checked_add(1) else {
                self.unavailable = true;
                return false;
            };
            self.account = account.to_owned();
            self.generation = generation;
            self.rooms.clear();
            self.cursor = None;
        }
        self.unavailable = account.is_empty() || joined_loaded.len() > MAX_DISCOVERY_ROOMS;
        if self.unavailable {
            self.rooms.clear();
            return false;
        }
        self.rooms.retain(|room, _| joined_loaded.contains(room));
        for room in joined_loaded {
            self.rooms.entry(room.clone()).or_default();
        }
        true
    }

    pub fn select_due(&mut self, now_ms: u64) -> Vec<ApprovalDiscoveryRequest> {
        if self.unavailable { return Vec::new(); }
        for room in self.rooms.values_mut() {
            if room.in_flight.is_some_and(|(_, deadline)| now_ms >= deadline) {
                room.in_flight = None;
                room.next_due_ms = now_ms.saturating_add(FAILURE_REFRESH_MS);
            }
        }
        // Bound outstanding requests as well as each tick's batch. A slow worker
        // must not let repeated timer ticks build an unbounded semaphore queue.
        let available = DISCOVERY_BATCH_SIZE.saturating_sub(
            self.rooms.values().filter(|room| room.in_flight.is_some()).count(),
        );
        let ordered: Vec<String> = self.rooms.keys()
            .filter(|room| self.cursor.as_ref().is_none_or(|cursor| *room > cursor))
            .chain(self.rooms.keys().filter(|room| self.cursor.as_ref().is_some_and(|cursor| *room <= cursor)))
            .cloned().collect();
        let mut selected = Vec::new();
        for room_id in ordered {
            if selected.len() >= available { break; }
            let Some(room) = self.rooms.get_mut(&room_id) else { continue };
            if room.in_flight.is_some() || now_ms < room.next_due_ms { continue; }
            let Some(nonce) = self.nonce.checked_add(1) else {
                self.unavailable = true;
                break;
            };
            self.nonce = nonce;
            // Two waves of two requests, with a small UI delivery allowance.
            let deadline = now_ms.saturating_add(
                DISCOVERY_TIMEOUT_MS * 2 + DISCOVERY_RESULT_GRACE_MS,
            );
            room.in_flight = Some((nonce, deadline));
            self.cursor = Some(room_id.clone());
            selected.push(ApprovalDiscoveryRequest {
                account: self.account.clone(), room_id, generation: self.generation, nonce,
            });
        }
        selected
    }

    /// Recheck account and membership immediately before this call. Only an
    /// accepted response may update or remove a private marker cache entry.
    pub fn accept_result(
        &mut self,
        request: &ApprovalDiscoveryRequest,
        outcome: DiscoveryOutcome,
        now_ms: u64,
    ) -> bool {
        if self.unavailable || request.account != self.account || request.generation != self.generation {
            return false;
        }
        let Some(room) = self.rooms.get_mut(&request.room_id) else { return false };
        if !room.in_flight.is_some_and(|(nonce, deadline)| nonce == request.nonce && now_ms < deadline) {
            return false;
        }
        room.in_flight = None;
        room.next_due_ms = if room.refresh_pending && outcome != DiscoveryOutcome::Failed {
            now_ms
        } else {
            now_ms.saturating_add(match outcome {
                DiscoveryOutcome::Present => POSITIVE_REFRESH_MS,
                DiscoveryOutcome::Absent => NEGATIVE_REFRESH_MS,
                DiscoveryOutcome::Failed => FAILURE_REFRESH_MS,
            })
        };
        room.refresh_pending = false;
        true
    }

    /// Relevant private events can request a refresh without duplicating a send
    /// that is already in flight. Timer refresh still covers missed events.
    pub fn refresh_room(&mut self, account: &str, room_id: &str) {
        if account == self.account {
            if let Some(room) = self.rooms.get_mut(room_id) {
                room.next_due_ms = 0;
                room.refresh_pending = room.in_flight.is_some();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn rooms(count: usize) -> BTreeSet<String> {
        (0..count).map(|id| format!("!room{id:04}:test")).collect()
    }

    #[test]
    fn approval_discovery_rejects_late_account_and_membership_results() {
        let mut schedule = ApprovalDiscoverySchedule::default();
        let joined = rooms(2);
        assert!(schedule.sync_membership("@owner:test", &joined));
        let first = schedule.select_due(1000);
        schedule.sync_membership("@other:test", &joined);
        assert!(!schedule.accept_result(&first[0], DiscoveryOutcome::Present, 1001));
        let second = schedule.select_due(1002);
        schedule.sync_membership("@other:test", &BTreeSet::new());
        schedule.sync_membership("@other:test", &joined);
        let third = schedule.select_due(1003);
        assert!(!schedule.accept_result(&second[0], DiscoveryOutcome::Present, 1004));
        assert!(schedule.accept_result(&third[0], DiscoveryOutcome::Present, 1004));
        assert!(!schedule.accept_result(&third[0], DiscoveryOutcome::Present, 1005));
    }

    #[test]
    fn approval_discovery_bounds_outstanding_work_and_refreshes_negatives() {
        let mut schedule = ApprovalDiscoverySchedule::default();
        schedule.sync_membership("@owner:test", &rooms(10));
        let first = schedule.select_due(1000);
        assert_eq!(first.len(), DISCOVERY_BATCH_SIZE);
        assert!(schedule.select_due(1001).is_empty());
        assert!(schedule.accept_result(&first[0], DiscoveryOutcome::Absent, 1002));
        let next = schedule.select_due(1003);
        assert_eq!(next.len(), 1);
        assert_ne!(first[0].room_id, next[0].room_id);
        let remaining = schedule.select_due(1000 + DISCOVERY_TIMEOUT_MS * 2 + DISCOVERY_RESULT_GRACE_MS);
        assert!(remaining.len() <= DISCOVERY_BATCH_SIZE);
        assert!(!schedule.accept_result(&first[1], DiscoveryOutcome::Present, 100_000));
        let mut seen = BTreeSet::new();
        for tick in 0..10 {
            let now = 200_000 + tick * 1000;
            for request in schedule.select_due(now) {
                seen.insert(request.room_id.clone());
                assert!(schedule.accept_result(&request, DiscoveryOutcome::Absent, now));
            }
        }
        assert!(seen.contains(&first[0].room_id));
    }

    #[test]
    fn approval_discovery_overflow_and_expired_response_fail_closed() {
        let mut schedule = ApprovalDiscoverySchedule::default();
        assert!(!schedule.sync_membership("@owner:test", &rooms(MAX_DISCOVERY_ROOMS + 1)));
        assert!(schedule.select_due(1000).is_empty());
        assert!(schedule.sync_membership("@owner:test", &rooms(1)));
        let request = schedule.select_due(1000).remove(0);
        assert!(!schedule.accept_result(&request, DiscoveryOutcome::Present, 100_000));
        schedule.select_due(100_000);
        let retried = schedule.select_due(130_000);
        assert_eq!(retried.len(), 1);
        assert_ne!(request.nonce, retried[0].nonce);
    }

    #[test]
    fn approval_discovery_visits_every_eligible_room_before_refreshing_completed_work() {
        for count in [0, 1, 3, 4, 5, 31, 128] {
            let joined = rooms(count);
            let mut schedule = ApprovalDiscoverySchedule::default();
            schedule.sync_membership("@owner:test", &joined);
            let mut visited = BTreeSet::new();
            for _ in 0..count.div_ceil(DISCOVERY_BATCH_SIZE) {
                let batch = schedule.select_due(1000);
                assert!(batch.len() <= DISCOVERY_BATCH_SIZE);
                for request in batch {
                    assert!(visited.insert(request.room_id.clone()));
                    assert!(schedule.accept_result(&request, DiscoveryOutcome::Absent, 1000));
                }
            }
            assert_eq!(visited, joined);
            assert!(schedule.select_due(1001).is_empty());
        }
    }

    #[test]
    fn approval_discovery_retains_refresh_received_during_an_in_flight_fetch() {
        let mut schedule = ApprovalDiscoverySchedule::default();
        schedule.sync_membership("@owner:test", &rooms(1));
        let request = schedule.select_due(1000).remove(0);
        schedule.refresh_room("@owner:test", &request.room_id);
        assert!(schedule.select_due(1001).is_empty());
        assert!(schedule.accept_result(&request, DiscoveryOutcome::Present, 1002));
        let refresh = schedule.select_due(1003);
        assert_eq!(refresh.len(), 1, "a private event received during fetch still needs revalidation");
        assert_ne!(request.nonce, refresh[0].nonce);
    }

    proptest! {
        #[test]
        fn prop_approval_discovery_bounded_fair_batches(count in 0usize..257, now in 1u64..1_000_000) {
            let joined = rooms(count);
            let mut schedule = ApprovalDiscoverySchedule::default();
            prop_assert!(schedule.sync_membership("@owner:test", &joined));
            let mut visited = BTreeSet::new();
            for _ in 0..count.div_ceil(DISCOVERY_BATCH_SIZE) {
                let batch = schedule.select_due(now);
                prop_assert!(batch.len() <= DISCOVERY_BATCH_SIZE);
                for request in batch {
                    prop_assert!(visited.insert(request.room_id.clone()));
                    prop_assert!(schedule.accept_result(&request, DiscoveryOutcome::Absent, now));
                }
            }
            prop_assert_eq!(visited, joined);
            prop_assert!(schedule.select_due(now + 1).is_empty());
        }
    }
}
