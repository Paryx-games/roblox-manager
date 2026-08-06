//! Which account is each running Roblox client?
//!
//! Windows tells us that `RobloxPlayerBeta.exe` with PID 1234 exists. It will
//! not tell us who is logged into it: the client authenticates with a one-shot
//! ticket embedded in a URI handled by `cmd /C start`, so RM never even holds
//! the child handle, let alone anything identifying. There is no supported way
//! to ask.
//!
//! What is left is inference from timing. RM notes the set of client PIDs at
//! the moment it launches, and the next client to appear is attributed to the
//! account that launch was for. That is genuinely a guess, and this module is
//! written so the shape of the guess is visible rather than buried:
//!
//! * Launches queue as [`PendingLaunch`]es, oldest first.
//! * [`InstanceRegistry::sweep`] diffs the live PID set against the previous
//!   sweep and pairs each brand-new PID with the oldest pending launch.
//! * A pending launch that never sees a client expires after
//!   [`PENDING_TTL`] rather than attaching itself to whatever appears next.
//!
//! # When this is wrong
//!
//! Matching is first-in-first-out on *appearance* order, so it is wrong exactly
//! when appearance order differs from launch order:
//!
//! * The user starts Roblox outside RM while a launch is pending. That client
//!   is attributed to RM's pending account, and RM's real client then takes the
//!   next slot (or none). Both mappings are wrong.
//! * A bulk launch where one account's client is much slower to start than the
//!   next account's. The two mappings swap.
//! * Windows recycles a PID that a tracked-but-exited client used. The recycled
//!   process looks brand new and can absorb a pending launch.
//!
//! None of these can be ruled out without cooperation from Roblox. The
//! *reverse* direction is nearly safe: a PID is only ever mapped to one
//! account, and [`InstanceRegistry::sweep`] drops a mapping as soon as it sees
//! the process is gone. The gap is that it only sees what a sweep observes. If
//! Windows recycles a PID between two sweeps, the registry never watches the
//! old process leave, and the mapping transfers silently to an unrelated one.
//! That is cosmetic while nothing acts on the map beyond a tooltip. Close it
//! before anything kills, focuses, or rejoins by PID.
//!
//! This module is substrate. It answers "which PIDs belong to this account";
//! it does not kill, focus, or arrange anything.

use std::collections::{HashSet, VecDeque};

use chrono::{DateTime, Duration as ChronoDuration, Utc};

/// How long a launch waits for a client to appear before RM stops expecting
/// one.
///
/// Generous on purpose. A cold Roblox start on a slow disk, with the
/// bootstrapper checking for an update first, routinely takes over half a
/// minute; giving up at ten seconds would leave the common case unattributed.
/// The cost of waiting too long is only that a genuinely failed launch can
/// still absorb an unrelated client during the window.
pub const PENDING_TTL_SECS: i64 = 45;

/// A launch that has happened but whose client has not been spotted yet.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PendingLaunch {
    pub user_id: u64,
    pub place_id: u64,
    pub launched_at: DateTime<Utc>,
}

/// A running Roblox client and the account RM believes is signed into it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrackedInstance {
    pub pid: u32,
    pub user_id: u64,
    pub place_id: u64,
    /// When the launch was issued, not when the process was first seen. This is
    /// the number a user would recognise as "how long has this been up".
    pub launched_at: DateTime<Utc>,
}

/// What one [`InstanceRegistry::sweep`] changed.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SweepOutcome {
    /// Clients that appeared and were matched to a pending launch.
    pub attributed: Vec<TrackedInstance>,
    /// Tracked clients whose process is gone.
    pub exited: Vec<TrackedInstance>,
    /// Launches that timed out without a client ever appearing.
    pub abandoned: Vec<PendingLaunch>,
    /// Clients that appeared with nothing pending to match them to. Almost
    /// always Roblox started outside RM.
    pub unattributed: Vec<u32>,
}

impl SweepOutcome {
    /// True when nothing changed, so callers can skip broadcasting.
    pub fn is_empty(&self) -> bool {
        self.attributed.is_empty()
            && self.exited.is_empty()
            && self.abandoned.is_empty()
            && self.unattributed.is_empty()
    }
}

/// The live PID-to-account map, owned by the backend.
#[derive(Debug, Default)]
pub struct InstanceRegistry {
    tracked: Vec<TrackedInstance>,
    pending: VecDeque<PendingLaunch>,
    /// PIDs the registry has already seen. Anything outside this set at the
    /// next sweep is new.
    known: HashSet<u32>,
}

impl InstanceRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record that a launch is about to happen for `user_id`.
    ///
    /// `live_now` must be the client PID set captured **immediately before**
    /// the launch. Folding it into `known` here is what stops the sweep from
    /// mistaking a client that was already running for the one this launch
    /// produces, and it is why this has to be called before the launch rather
    /// than after.
    pub fn note_launch(
        &mut self,
        user_id: u64,
        place_id: u64,
        live_now: &HashSet<u32>,
        now: DateTime<Utc>,
    ) {
        self.known.extend(live_now.iter().copied());
        self.pending.push_back(PendingLaunch {
            user_id,
            place_id,
            launched_at: now,
        });
    }

    /// Reconcile against the current set of live client PIDs.
    ///
    /// Call this on a timer. It is the only thing that ever adds to or removes
    /// from the map, so the map cannot drift from reality by more than one
    /// sweep interval.
    pub fn sweep(&mut self, live: &HashSet<u32>, now: DateTime<Utc>) -> SweepOutcome {
        let mut outcome = SweepOutcome::default();

        // Expire pending launches first. A launch that failed outright (bad
        // cookie, Roblox refused the ticket) must not sit in the queue and
        // claim the next client the user starts by hand.
        let ttl = ChronoDuration::seconds(PENDING_TTL_SECS);
        while let Some(front) = self.pending.front() {
            if now - front.launched_at <= ttl {
                break;
            }
            // `pop_front` cannot fail: `front` just proved the queue is
            // non-empty, and nothing else touches it in between.
            let Some(expired) = self.pending.pop_front() else {
                break;
            };
            outcome.abandoned.push(expired);
        }

        // Brand-new PIDs, in ascending order purely so the result is
        // deterministic. Windows PIDs are not allocated monotonically, so this
        // ordering carries no information about which client started first;
        // when two appear in the same sweep the pairing is a coin flip, and
        // that is the honest failure mode described in this module's docs.
        let mut appeared: Vec<u32> = live.difference(&self.known).copied().collect();
        appeared.sort_unstable();

        for pid in appeared {
            match self.pending.pop_front() {
                Some(launch) => {
                    let instance = TrackedInstance {
                        pid,
                        user_id: launch.user_id,
                        place_id: launch.place_id,
                        launched_at: launch.launched_at,
                    };
                    self.tracked.push(instance.clone());
                    outcome.attributed.push(instance);
                }
                None => outcome.unattributed.push(pid),
            }
        }

        // Reap. A mapping must never outlive its process: the whole point of
        // this registry is that a future "act on this account's client" can
        // trust the PID it is handed.
        let mut still_alive = Vec::with_capacity(self.tracked.len());
        for instance in self.tracked.drain(..) {
            if live.contains(&instance.pid) {
                still_alive.push(instance);
            } else {
                outcome.exited.push(instance);
            }
        }
        self.tracked = still_alive;

        // Replacing rather than extending is deliberate: a PID that died must
        // be forgotten, or the registry grows without bound over a long
        // session. The cost is that a recycled PID reads as new, which is the
        // third failure mode in this module's docs.
        self.known = live.clone();

        outcome
    }

    /// Every tracked client, ordered by launch time.
    pub fn snapshot(&self) -> Vec<TrackedInstance> {
        let mut out = self.tracked.clone();
        out.sort_by_key(|i| (i.launched_at, i.pid));
        out
    }

    /// PIDs believed to belong to `user_id`. More than one when the account was
    /// launched several times under multi-instance.
    pub fn pids_for(&self, user_id: u64) -> Vec<u32> {
        self.tracked
            .iter()
            .filter(|i| i.user_id == user_id)
            .map(|i| i.pid)
            .collect()
    }

    /// The account believed to be signed into `pid`, if RM launched it.
    pub fn user_for(&self, pid: u32) -> Option<u64> {
        self.tracked.iter().find(|i| i.pid == pid).map(|i| i.user_id)
    }

    /// Launches still waiting for a client to show up.
    pub fn pending_count(&self) -> usize {
        self.pending.len()
    }

    /// Forget everything: tracked clients, pending launches, and the set of
    /// PIDs already seen.
    ///
    /// Intended for when the store is locked or replaced, where keeping account
    /// IDs around would outlive the reason RM had them.
    ///
    /// Nothing outside these tests calls it yet. In particular the "delete
    /// everything and start over" reset in the UI does not: the registry lives
    /// on the backend thread and there is no `BackendCommand` that reaches it.
    /// Attributions made under the previous store therefore survive the reset
    /// and show up in the instance tooltip as stale `user <id>` lines until
    /// those clients exit.
    pub fn clear(&mut self) {
        self.tracked.clear();
        self.pending.clear();
        self.known.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t0() -> DateTime<Utc> {
        DateTime::from_timestamp(1_700_000_000, 0).unwrap()
    }

    fn at(secs: i64) -> DateTime<Utc> {
        t0() + ChronoDuration::seconds(secs)
    }

    fn pids(list: &[u32]) -> HashSet<u32> {
        list.iter().copied().collect()
    }

    #[test]
    fn a_launch_claims_the_next_client_to_appear() {
        let mut reg = InstanceRegistry::new();
        reg.note_launch(7, 606, &pids(&[]), t0());

        let out = reg.sweep(&pids(&[1234]), at(4));
        assert_eq!(out.attributed.len(), 1);
        assert_eq!(out.attributed[0].pid, 1234);
        assert_eq!(out.attributed[0].user_id, 7);
        assert_eq!(out.attributed[0].place_id, 606);
        // The timestamp is the launch, not the sighting.
        assert_eq!(out.attributed[0].launched_at, t0());
        assert_eq!(reg.pids_for(7), vec![1234]);
        assert_eq!(reg.user_for(1234), Some(7));
    }

    /// The reason `note_launch` takes the live set: a client that was already
    /// up when the launch went out is not the one the launch produced.
    #[test]
    fn a_client_that_predates_the_launch_is_not_attributed() {
        let mut reg = InstanceRegistry::new();
        reg.note_launch(7, 606, &pids(&[999]), t0());

        let out = reg.sweep(&pids(&[999]), at(2));
        assert!(out.attributed.is_empty(), "{out:?}");
        assert_eq!(reg.pending_count(), 1, "the launch should still be waiting");

        let out = reg.sweep(&pids(&[999, 1000]), at(5));
        assert_eq!(out.attributed.len(), 1);
        assert_eq!(out.attributed[0].pid, 1000);
    }

    #[test]
    fn a_client_started_outside_rm_is_reported_but_not_attributed() {
        let mut reg = InstanceRegistry::new();
        let out = reg.sweep(&pids(&[42]), t0());
        assert_eq!(out.unattributed, vec![42]);
        assert!(out.attributed.is_empty());
        assert_eq!(reg.user_for(42), None);
    }

    #[test]
    fn bulk_launches_are_matched_in_order() {
        let mut reg = InstanceRegistry::new();
        let mut live = pids(&[]);
        for (i, user) in [11u64, 22, 33].into_iter().enumerate() {
            reg.note_launch(user, 606, &live, at(i as i64 * 3));
            // Each client turns up before the next launch goes out, which is
            // the case this ordering is correct for.
            live.insert(500 + i as u32);
            reg.sweep(&live, at(i as i64 * 3 + 1));
        }
        assert_eq!(reg.pids_for(11), vec![500]);
        assert_eq!(reg.pids_for(22), vec![501]);
        assert_eq!(reg.pids_for(33), vec![502]);
        assert_eq!(reg.pending_count(), 0);
    }

    /// Two clients appearing between sweeps is the ambiguous case. It must
    /// still produce two mappings rather than dropping one, and both pending
    /// launches must be consumed.
    #[test]
    fn two_clients_in_one_sweep_consume_two_pending_launches() {
        let mut reg = InstanceRegistry::new();
        reg.note_launch(11, 606, &pids(&[]), t0());
        reg.note_launch(22, 606, &pids(&[]), at(1));

        let out = reg.sweep(&pids(&[900, 901]), at(6));
        assert_eq!(out.attributed.len(), 2);
        assert_eq!(reg.pending_count(), 0);
        // Both PIDs are claimed exactly once, whichever way round.
        let mut owners: Vec<u64> = out.attributed.iter().map(|i| i.user_id).collect();
        owners.sort_unstable();
        assert_eq!(owners, vec![11, 22]);
    }

    #[test]
    fn one_account_launched_twice_tracks_both_clients() {
        let mut reg = InstanceRegistry::new();
        reg.note_launch(7, 606, &pids(&[]), t0());
        reg.sweep(&pids(&[10]), at(2));
        reg.note_launch(7, 606, &pids(&[10]), at(3));
        reg.sweep(&pids(&[10, 11]), at(5));

        let mut got = reg.pids_for(7);
        got.sort_unstable();
        assert_eq!(got, vec![10, 11]);
    }

    #[test]
    fn a_dead_client_is_reaped_and_stops_answering() {
        let mut reg = InstanceRegistry::new();
        reg.note_launch(7, 606, &pids(&[]), t0());
        reg.sweep(&pids(&[1234]), at(2));

        let out = reg.sweep(&pids(&[]), at(30));
        assert_eq!(out.exited.len(), 1);
        assert_eq!(out.exited[0].pid, 1234);
        assert!(reg.pids_for(7).is_empty());
        assert_eq!(reg.user_for(1234), None);
        assert!(reg.snapshot().is_empty());
    }

    #[test]
    fn a_launch_that_never_starts_a_client_expires() {
        let mut reg = InstanceRegistry::new();
        reg.note_launch(7, 606, &pids(&[]), t0());

        let out = reg.sweep(&pids(&[]), at(PENDING_TTL_SECS - 1));
        assert!(out.abandoned.is_empty(), "expired too early");
        assert_eq!(reg.pending_count(), 1);

        let out = reg.sweep(&pids(&[]), at(PENDING_TTL_SECS + 1));
        assert_eq!(out.abandoned.len(), 1);
        assert_eq!(out.abandoned[0].user_id, 7);
        assert_eq!(reg.pending_count(), 0);
    }

    /// The point of expiry: a failed launch must not silently claim the client
    /// the user starts by hand five minutes later.
    #[test]
    fn an_expired_launch_does_not_claim_a_later_client() {
        let mut reg = InstanceRegistry::new();
        reg.note_launch(7, 606, &pids(&[]), t0());
        reg.sweep(&pids(&[]), at(PENDING_TTL_SECS + 1));

        let out = reg.sweep(&pids(&[4242]), at(PENDING_TTL_SECS + 5));
        assert!(out.attributed.is_empty(), "{out:?}");
        assert_eq!(out.unattributed, vec![4242]);
    }

    #[test]
    fn sweeping_a_steady_state_reports_nothing() {
        let mut reg = InstanceRegistry::new();
        reg.note_launch(7, 606, &pids(&[]), t0());
        reg.sweep(&pids(&[1234]), at(2));

        let out = reg.sweep(&pids(&[1234]), at(4));
        assert!(out.is_empty(), "{out:?}");
    }

    /// The third failure mode in this module's docs, pinned down.
    ///
    /// Reaping drops the dead PID from `known` as well as from `tracked`, so
    /// when Windows hands that number to a new client the registry has no way
    /// to tell it apart from a genuinely new one: it reads as brand new and
    /// takes whatever launch is pending. Here that happens to land on the right
    /// account, but only because the pending launch really is the recycled
    /// process. Had the recycled PID belonged to a client the user started by
    /// hand, the same code path would have handed it to user 8 just as
    /// confidently. The registry does not distinguish the two cases and this
    /// test does not pretend it does.
    #[test]
    fn a_recycled_pid_reads_as_a_brand_new_client() {
        let mut reg = InstanceRegistry::new();

        // User 7 launches, is given PID 1234, then exits.
        reg.note_launch(7, 606, &pids(&[]), t0());
        reg.sweep(&pids(&[1234]), at(2));
        assert_eq!(reg.user_for(1234), Some(7));

        let out = reg.sweep(&pids(&[]), at(4));
        assert_eq!(out.exited.len(), 1);
        assert_eq!(out.exited[0].user_id, 7);
        assert_eq!(reg.user_for(1234), None);

        // User 8 launches and Windows reuses 1234 for the new client.
        reg.note_launch(8, 606, &pids(&[]), at(5));
        let out = reg.sweep(&pids(&[1234]), at(7));

        assert_eq!(out.attributed.len(), 1);
        assert_eq!(out.attributed[0].pid, 1234);
        assert_eq!(out.attributed[0].user_id, 8);
        assert!(out.unattributed.is_empty(), "{out:?}");
        // The old owner is gone entirely; nothing lingers to be confused with
        // the new mapping.
        assert_eq!(reg.user_for(1234), Some(8));
        assert!(reg.pids_for(7).is_empty());
        assert_eq!(reg.pids_for(8), vec![1234]);
    }

    #[test]
    fn clearing_forgets_every_account_id() {
        let mut reg = InstanceRegistry::new();
        reg.note_launch(7, 606, &pids(&[]), t0());
        reg.sweep(&pids(&[1234]), at(2));
        reg.note_launch(8, 606, &pids(&[1234]), at(3));

        reg.clear();
        assert!(reg.snapshot().is_empty());
        assert_eq!(reg.pending_count(), 0);
        assert_eq!(reg.user_for(1234), None);
        // And a client that was live before the clear is not then attributed to
        // whatever is launched next.
        let out = reg.sweep(&pids(&[1234]), at(4));
        assert_eq!(out.unattributed, vec![1234]);
    }
}
