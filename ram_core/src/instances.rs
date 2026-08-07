//! Which account is each running Roblox client?
//!
//! Windows tells us that `RobloxPlayerBeta.exe` with PID 1234 exists. It will
//! not tell us who is logged into it: the client authenticates with a one-shot
//! ticket embedded in a URI handled by `cmd /C start`, so RM never even holds
//! the child handle.
//!
//! What RM does hold is a token it minted itself. Every launch stamps the URI
//! with `+launchtime:<millis>+`, and that value survives all the way onto the
//! spawned client's **command line**. Reading the command line back off a
//! running client therefore turns attribution from an inference into a lookup:
//!
//! * A launch registers a [`PendingLaunch`] carrying the `launchtime` it used.
//! * [`InstanceRegistry::sweep`] is handed the live clients, each with whatever
//!   [`LaunchToken`] could be read from its command line.
//! * A token that matches a pending launch is an [`Attribution::Exact`] match.
//!   Nothing else can produce one, because nothing else knows that number.
//!
//! # What is guaranteed, and what is not
//!
//! An `Exact` mapping means this specific process was started by this specific
//! RM launch. That is a fact, not a guess: the number came out of RM and came
//! back on the process's own command line, and duplicates are refused at
//! mint time (see `process::next_launchtime`). Appearance order, bulk-launch
//! interleaving, and clients the user starts by hand no longer affect it. A
//! client RM did not launch carries somebody else's `launchtime` (or none), so
//! it reads as unattributed instead of stealing a pending launch.
//!
//! What is *not* guaranteed:
//!
//! * **Reading the command line can fail.** It needs `OpenProcess` +
//!   `ReadProcessMemory` against a Hyperion-protected process. That works today
//!   on this machine, but an elevated client will refuse and Hyperion could
//!   tighten at any time. When it fails, the old appearance-order pairing runs
//!   as a labelled fallback and the mapping is marked [`Attribution::Inferred`]
//!   with all the failure modes that implies: a hand-started client can absorb
//!   a pending launch, and a bulk launch that starts out of order swaps two
//!   mappings. Callers that act on a PID must decide for themselves whether
//!   `Inferred` is good enough. Killing on one is not.
//! * **A mapping is only as fresh as the last sweep.** The registry learns a
//!   process is gone when a sweep does not see it, so between sweeps it can
//!   name a PID that has already exited. Recycling of that PID is handled:
//!   every client is keyed on `(pid, start_time)`, so Windows handing the same
//!   number to an unrelated process reads as a different process rather than
//!   silently inheriting the mapping. Anything that acts destructively on a PID
//!   must still re-verify at the moment it acts, because the sweep interval is
//!   a window no bookkeeping here can close.
//!
//! This module is substrate and is deliberately free of Win32: it takes live
//! clients as data. `process::LaunchTokenCache` is what actually reads command
//! lines and builds them.

use std::collections::{HashSet, VecDeque};

use chrono::{DateTime, Duration as ChronoDuration, Utc};

/// How long a launch stays eligible for the appearance-order *fallback*.
///
/// Generous on purpose. A cold Roblox start on a slow disk, with the
/// bootstrapper checking for an update first, routinely takes over half a
/// minute; giving up at ten seconds would leave the common case unattributed.
/// The cost of waiting is that a genuinely failed launch can absorb an
/// unrelated client during the window, which is why this is the *short* of the
/// two lifetimes below.
pub const PENDING_TTL_SECS: i64 = 45;

/// How long a launch stays eligible for an **exact** `launchtime` match.
///
/// Much longer than [`PENDING_TTL_SECS`] because it is safe to be. Ordering
/// stops meaning anything after a minute or so, but a token match cannot claim
/// the wrong client no matter how late it arrives: no other process will ever
/// carry this number. A first-ever install downloading the whole client can
/// take many minutes, and attributing it correctly at the end is strictly
/// better than giving up at 45 seconds.
pub const EXACT_TTL_SECS: i64 = 300;

/// What one client's command line had to say about who launched it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LaunchToken {
    /// The command line was read and carried this `launchtime`.
    Found(i64),
    /// The command line was read and carried no `launchtime` at all. A
    /// `--launch-to-tray` background client looks like this, and so does
    /// anything started without a `roblox-player:` URI. Crucially this is *not*
    /// the same as a failed read: RM knows this process did not come from a
    /// launch URI, so it must never absorb a pending launch.
    Absent,
    /// The command line could not be read. Nothing is known either way, so the
    /// appearance-order fallback applies.
    Unreadable,
}

/// One running Roblox client as a sweep sees it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LiveClient {
    pub pid: u32,
    /// Process creation time. Paired with the PID it identifies a process
    /// across sweeps, so a recycled PID cannot inherit an old mapping.
    pub start_time: u64,
    pub token: LaunchToken,
}

impl LiveClient {
    /// The identity a sweep tracks a process by.
    fn key(&self) -> (u32, u64) {
        (self.pid, self.start_time)
    }
}

/// How much weight a mapping carries.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Attribution {
    /// The client's own command line carried a `launchtime` RM minted for this
    /// launch. Not a guess.
    Exact,
    /// The command line could not be read, so the client was paired with the
    /// oldest pending launch by appearance order. A guess, and labelled as one.
    Inferred,
}

impl Attribution {
    /// True only for a mapping backed by the client's own command line.
    pub fn is_exact(self) -> bool {
        matches!(self, Attribution::Exact)
    }
}

/// A launch that has happened but whose client has not been spotted yet.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PendingLaunch {
    pub user_id: u64,
    pub place_id: u64,
    /// The token stamped into this launch's URI. Unique across the process.
    pub launchtime: i64,
    pub launched_at: DateTime<Utc>,
    /// Set once [`PENDING_TTL_SECS`] has passed and the launch has been
    /// reported as abandoned. It stays in the queue until [`EXACT_TTL_SECS`]
    /// anyway, because an exact match is still safe long after ordering has
    /// stopped being informative. It simply stops being offered to the
    /// appearance-order fallback.
    pub fifo_expired: bool,
}

/// A running Roblox client and the account RM believes is signed into it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrackedInstance {
    pub pid: u32,
    /// Process creation time, so a caller can confirm it is still looking at
    /// the same process this mapping was made against.
    pub start_time: u64,
    pub user_id: u64,
    pub place_id: u64,
    /// The `launchtime` this client is expected to carry. For an
    /// [`Attribution::Exact`] mapping it has been read off the process. For an
    /// `Inferred` one it is only what the paired launch used, and re-reading
    /// the process is what would turn it into a fact.
    pub launchtime: i64,
    /// When the launch was issued, not when the process was first seen. This is
    /// the number a user would recognise as "how long has this been up".
    pub launched_at: DateTime<Utc>,
    pub attribution: Attribution,
}

/// What one [`InstanceRegistry::sweep`] changed.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SweepOutcome {
    /// Clients that appeared and were matched to a pending launch.
    pub attributed: Vec<TrackedInstance>,
    /// Mappings that were `Inferred` and have since been confirmed by reading
    /// the client's command line. Same instances, now carrying
    /// [`Attribution::Exact`].
    pub upgraded: Vec<TrackedInstance>,
    /// Tracked clients whose process is gone.
    pub exited: Vec<TrackedInstance>,
    /// Launches that timed out without a client ever appearing.
    pub abandoned: Vec<PendingLaunch>,
    /// Clients that appeared with nothing to match them to: either they carry
    /// a `launchtime` RM did not mint (started outside RM) or none at all (a
    /// tray client). Not a failure, just not ours.
    pub unattributed: Vec<u32>,
}

impl SweepOutcome {
    /// True when nothing changed, so callers can skip broadcasting.
    pub fn is_empty(&self) -> bool {
        self.attributed.is_empty()
            && self.upgraded.is_empty()
            && self.exited.is_empty()
            && self.abandoned.is_empty()
            && self.unattributed.is_empty()
    }
}

/// Pull the `launchtime` out of a Roblox client's command line.
///
/// The launch URI embeds it as `+launchtime:1754438400000+`, so the value runs
/// from after the colon to the first non-digit. Returns `None` for a command
/// line that has no such field at all, which is the normal answer for
/// `--launch-to-tray` and for anything not started from a launch URI.
pub fn parse_launchtime(cmdline: &str) -> Option<i64> {
    const KEY: &str = "launchtime:";
    let start = cmdline.find(KEY)? + KEY.len();
    let digits: String = cmdline[start..]
        .chars()
        .take_while(char::is_ascii_digit)
        .collect();
    // A field present but empty, or a number too large to be a timestamp, is
    // not something to guess at.
    digits.parse::<i64>().ok()
}

/// Classify one client's command line, given whether it could be read at all.
///
/// Kept next to [`parse_launchtime`] so the three-way distinction that the
/// sweep depends on is defined in one place: "no launchtime" and "could not
/// look" are different answers and must not collapse into each other.
pub fn classify_cmdline(cmdline: Option<&str>) -> LaunchToken {
    match cmdline {
        None => LaunchToken::Unreadable,
        Some(line) => match parse_launchtime(line) {
            Some(lt) => LaunchToken::Found(lt),
            None => LaunchToken::Absent,
        },
    }
}

/// The live PID-to-account map, owned by the backend.
#[derive(Debug, Default)]
pub struct InstanceRegistry {
    tracked: Vec<TrackedInstance>,
    pending: VecDeque<PendingLaunch>,
    /// Clients the registry has already seen, keyed by `(pid, start_time)`.
    /// Anything outside this set at the next sweep is new.
    known: HashSet<(u32, u64)>,
}

impl InstanceRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record that a launch stamped with `launchtime` is about to happen for
    /// `user_id`.
    ///
    /// `live_now` must be the client set captured **immediately before** the
    /// launch. It only matters to the appearance-order fallback: folding it
    /// into `known` is what stops a client that was already running from being
    /// mistaken for the one this launch produces. The exact path does not need
    /// it, but the fallback is still there and still does.
    ///
    /// Must be called before the launch rather than after, so that a sweep
    /// landing in the gap cannot see the new client before RM knows what to
    /// match it to.
    pub fn note_launch(
        &mut self,
        user_id: u64,
        place_id: u64,
        launchtime: i64,
        live_now: &[LiveClient],
        now: DateTime<Utc>,
    ) {
        self.known.extend(live_now.iter().map(LiveClient::key));
        self.pending.push_back(PendingLaunch {
            user_id,
            place_id,
            launchtime,
            launched_at: now,
            fifo_expired: false,
        });
    }

    /// Reconcile against the current set of live clients.
    ///
    /// Call this on a timer. It is the only thing that ever adds to or removes
    /// from the map, so the map cannot drift from reality by more than one
    /// sweep interval.
    pub fn sweep(&mut self, live: &[LiveClient], now: DateTime<Utc>) -> SweepOutcome {
        let mut outcome = SweepOutcome::default();

        // Age the pending queue. A launch that failed outright (bad cookie,
        // Roblox refused the ticket) must stop claiming clients by order, but
        // it can safely stay exact-matchable for much longer.
        let fifo_ttl = ChronoDuration::seconds(PENDING_TTL_SECS);
        let exact_ttl = ChronoDuration::seconds(EXACT_TTL_SECS);
        for launch in self.pending.iter_mut() {
            if !launch.fifo_expired && now - launch.launched_at > fifo_ttl {
                launch.fifo_expired = true;
                outcome.abandoned.push(launch.clone());
            }
        }
        self.pending
            .retain(|launch| now - launch.launched_at <= exact_ttl);

        // Brand-new clients, in ascending PID order purely so the result is
        // deterministic. For the exact path the order is irrelevant. For the
        // fallback it carries no information either: Windows PIDs are not
        // allocated monotonically, so when two unreadable clients appear in the
        // same sweep the pairing is a coin flip. That is the honest failure
        // mode described in this module's docs.
        let mut appeared: Vec<&LiveClient> = live
            .iter()
            .filter(|client| !self.known.contains(&client.key()))
            .collect();
        appeared.sort_unstable_by_key(|client| client.pid);

        for client in appeared {
            let matched = match client.token {
                // The whole point of the module: a number RM minted came back
                // on a process's own command line.
                LaunchToken::Found(token) => self
                    .pending
                    .iter()
                    .position(|launch| launch.launchtime == token)
                    .and_then(|idx| self.pending.remove(idx))
                    .map(|launch| (launch, Attribution::Exact)),
                // Read successfully, and it is not from an RM launch URI. This
                // is the case that used to steal a pending launch.
                LaunchToken::Absent => None,
                // Nothing is known, so fall back to appearance order over the
                // launches still inside the short window.
                LaunchToken::Unreadable => self
                    .pending
                    .iter()
                    .position(|launch| !launch.fifo_expired)
                    .and_then(|idx| self.pending.remove(idx))
                    .map(|launch| (launch, Attribution::Inferred)),
            };

            match matched {
                Some((launch, attribution)) => {
                    let instance = TrackedInstance {
                        pid: client.pid,
                        start_time: client.start_time,
                        user_id: launch.user_id,
                        place_id: launch.place_id,
                        launchtime: launch.launchtime,
                        launched_at: launch.launched_at,
                        attribution,
                    };
                    self.tracked.push(instance.clone());
                    outcome.attributed.push(instance);
                }
                None => outcome.unattributed.push(client.pid),
            }
        }

        // Promote a guess to a fact. A client's PEB is not always readable the
        // instant it appears, so a mapping can start out inferred and become
        // confirmable a sweep or two later. A mismatch is deliberately *not*
        // acted on here: the mapping stays labelled `Inferred`, which already
        // means "do not trust this", and unwinding a consumed pending launch
        // would be a lot of machinery for a case that only arises when reading
        // command lines is failing anyway.
        for instance in self.tracked.iter_mut() {
            if instance.attribution.is_exact() {
                continue;
            }
            let confirmed = live.iter().any(|client| {
                client.pid == instance.pid
                    && client.start_time == instance.start_time
                    && client.token == LaunchToken::Found(instance.launchtime)
            });
            if confirmed {
                instance.attribution = Attribution::Exact;
                outcome.upgraded.push(instance.clone());
            }
        }

        // Reap. A mapping must never outlive its process: the whole point of
        // this registry is that "act on this account's client" can trust the
        // PID it is handed. Matching on `(pid, start_time)` rather than the PID
        // alone is what makes a recycled PID read as a different process
        // instead of silently inheriting this mapping.
        let live_keys: HashSet<(u32, u64)> = live.iter().map(LiveClient::key).collect();
        let mut still_alive = Vec::with_capacity(self.tracked.len());
        for instance in self.tracked.drain(..) {
            if live_keys.contains(&(instance.pid, instance.start_time)) {
                still_alive.push(instance);
            } else {
                outcome.exited.push(instance);
            }
        }
        self.tracked = still_alive;

        // Replacing rather than extending keeps this from growing without
        // bound over a long session. Dropping a dead entry is safe now that the
        // key includes the start time: a recycled PID is a genuinely new
        // process and reads as one.
        self.known = live_keys;

        outcome
    }

    /// Every tracked client, ordered by launch time.
    pub fn snapshot(&self) -> Vec<TrackedInstance> {
        let mut out = self.tracked.clone();
        out.sort_by_key(|i| (i.launched_at, i.pid));
        out
    }

    /// Clients believed to belong to `user_id`. More than one when the account
    /// was launched several times under multi-instance.
    pub fn instances_for(&self, user_id: u64) -> Vec<TrackedInstance> {
        self.tracked
            .iter()
            .filter(|i| i.user_id == user_id)
            .cloned()
            .collect()
    }

    /// PIDs believed to belong to `user_id`, exact and inferred alike. Callers
    /// that care about the difference want [`Self::instances_for`].
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

    /// Launches still eligible for the appearance-order fallback.
    pub fn pending_count(&self) -> usize {
        self.pending.iter().filter(|l| !l.fifo_expired).count()
    }

    /// Launches still eligible for an exact match, including ones that have
    /// already been reported as abandoned for ordering purposes.
    pub fn matchable_count(&self) -> usize {
        self.pending.len()
    }

    /// Forget everything: tracked clients, pending launches, and the set of
    /// clients already seen.
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

    /// A client whose command line RM could read and which carries `token`.
    fn tagged(pid: u32, token: i64) -> LiveClient {
        LiveClient {
            pid,
            start_time: 1_000,
            token: LaunchToken::Found(token),
        }
    }

    /// A client whose command line RM could read and which has no launchtime:
    /// a `--launch-to-tray` background process.
    fn tray(pid: u32) -> LiveClient {
        LiveClient {
            pid,
            start_time: 1_000,
            token: LaunchToken::Absent,
        }
    }

    /// A client whose command line RM could not read at all.
    fn opaque(pid: u32) -> LiveClient {
        LiveClient {
            pid,
            start_time: 1_000,
            token: LaunchToken::Unreadable,
        }
    }

    // -----------------------------------------------------------------------
    // Parsing
    // -----------------------------------------------------------------------

    /// The real shape, taken from a command line read off a live client.
    const REAL_CMDLINE: &str = concat!(
        r#""C:\Users\K\AppData\Local\Roblox\Versions\version-145f18\RobloxPlayerBeta.exe" "#,
        "roblox-player:1+launchmode:play+gameinfo:SOMETICKET+launchtime:1779673402853",
        "+placelauncherurl:https%3A%2F%2Fassetgame.roblox.com%2Fgame%2FPlaceLauncher.ashx",
        "%3Frequest%3DRequestGame%26browserTrackerId%3D963346767%26placeId%3D91370983012229"
    );

    #[test]
    fn a_real_command_line_yields_its_launchtime() {
        assert_eq!(parse_launchtime(REAL_CMDLINE), Some(1_779_673_402_853));
    }

    #[test]
    fn the_token_stops_at_the_field_separator() {
        // `+` terminates the field, so the following field must not be eaten.
        let line = "roblox-player:1+launchtime:1700000000000+placelauncherurl:https";
        assert_eq!(parse_launchtime(line), Some(1_700_000_000_000));
    }

    #[test]
    fn a_tray_command_line_has_no_launchtime() {
        let line = r#""C:\Roblox\RobloxPlayerBeta.exe" --launch-to-tray"#;
        assert_eq!(parse_launchtime(line), None);
    }

    #[test]
    fn a_command_line_with_no_uri_at_all_has_no_launchtime() {
        assert_eq!(parse_launchtime(r#""C:\Roblox\RobloxPlayerBeta.exe""#), None);
    }

    /// A truncated or malformed field must read as absent rather than as some
    /// arbitrary number that could collide with a real launch.
    #[test]
    fn an_empty_or_unparseable_launchtime_is_not_guessed_at() {
        assert_eq!(parse_launchtime("launchtime:+placelauncherurl:x"), None);
        assert_eq!(parse_launchtime("launchtime:notanumber"), None);
        // Wider than i64.
        assert_eq!(parse_launchtime("launchtime:99999999999999999999999"), None);
    }

    /// The three-way classification the whole sweep hangs off.
    #[test]
    fn classification_separates_absent_from_unreadable() {
        assert_eq!(classify_cmdline(None), LaunchToken::Unreadable);
        assert_eq!(
            classify_cmdline(Some(r#""RobloxPlayerBeta.exe" --launch-to-tray"#)),
            LaunchToken::Absent
        );
        assert_eq!(
            classify_cmdline(Some(REAL_CMDLINE)),
            LaunchToken::Found(1_779_673_402_853)
        );
    }

    // -----------------------------------------------------------------------
    // Exact attribution
    // -----------------------------------------------------------------------

    #[test]
    fn a_launch_claims_the_client_carrying_its_token() {
        let mut reg = InstanceRegistry::new();
        reg.note_launch(7, 606, 5_000, &[], t0());

        let out = reg.sweep(&[tagged(1234, 5_000)], at(4));
        assert_eq!(out.attributed.len(), 1);
        assert_eq!(out.attributed[0].pid, 1234);
        assert_eq!(out.attributed[0].user_id, 7);
        assert_eq!(out.attributed[0].place_id, 606);
        assert_eq!(out.attributed[0].launchtime, 5_000);
        assert_eq!(out.attributed[0].attribution, Attribution::Exact);
        // The timestamp is the launch, not the sighting.
        assert_eq!(out.attributed[0].launched_at, t0());
        assert_eq!(reg.pids_for(7), vec![1234]);
        assert_eq!(reg.user_for(1234), Some(7));
    }

    /// The headline fix. Under the old appearance-order model this client would
    /// have been handed user 7's pending launch.
    #[test]
    fn a_client_started_outside_rm_does_not_steal_a_pending_launch() {
        let mut reg = InstanceRegistry::new();
        reg.note_launch(7, 606, 5_000, &[], t0());

        // Someone clicks Play on the Roblox website. Its URI carries a
        // launchtime too, just not one RM minted.
        let out = reg.sweep(&[tagged(999, 8_888)], at(4));

        assert!(out.attributed.is_empty(), "{out:?}");
        assert_eq!(out.unattributed, vec![999]);
        assert_eq!(reg.user_for(999), None);
        assert_eq!(reg.pending_count(), 1, "the launch must still be waiting");

        // And RM's real client, whenever it turns up, still lands correctly.
        let out = reg.sweep(&[tagged(999, 8_888), tagged(1234, 5_000)], at(9));
        assert_eq!(out.attributed.len(), 1);
        assert_eq!(out.attributed[0].pid, 1234);
        assert_eq!(out.attributed[0].user_id, 7);
    }

    /// A `--launch-to-tray` client has no launchtime whatsoever. It must read
    /// as unattributed, not fall through to the fallback and absorb a launch.
    #[test]
    fn a_tray_client_never_absorbs_a_pending_launch() {
        let mut reg = InstanceRegistry::new();
        reg.note_launch(7, 606, 5_000, &[], t0());

        let out = reg.sweep(&[tray(4321)], at(3));

        assert!(out.attributed.is_empty(), "{out:?}");
        assert_eq!(out.unattributed, vec![4321]);
        assert_eq!(reg.pending_count(), 1);
    }

    /// Out-of-order appearance was the second failure mode of the old model.
    /// With tokens, order is simply not consulted.
    #[test]
    fn a_bulk_launch_landing_out_of_order_still_maps_each_account_correctly() {
        let mut reg = InstanceRegistry::new();
        reg.note_launch(11, 606, 5_001, &[], t0());
        reg.note_launch(22, 606, 5_002, &[], at(1));
        reg.note_launch(33, 606, 5_003, &[], at(2));

        // All three appear in one sweep, and the PID order is the reverse of
        // the launch order.
        let out = reg.sweep(
            &[tagged(700, 5_003), tagged(800, 5_002), tagged(900, 5_001)],
            at(8),
        );

        assert_eq!(out.attributed.len(), 3);
        assert!(out.attributed.iter().all(|i| i.attribution.is_exact()));
        assert_eq!(reg.pids_for(11), vec![900]);
        assert_eq!(reg.pids_for(22), vec![800]);
        assert_eq!(reg.pids_for(33), vec![700]);
        assert_eq!(reg.pending_count(), 0);
    }

    #[test]
    fn one_account_launched_twice_tracks_both_clients_by_their_own_tokens() {
        let mut reg = InstanceRegistry::new();
        reg.note_launch(7, 606, 5_001, &[], t0());
        reg.sweep(&[tagged(10, 5_001)], at(2));
        reg.note_launch(7, 606, 5_002, &[tagged(10, 5_001)], at(3));
        reg.sweep(&[tagged(10, 5_001), tagged(11, 5_002)], at(5));

        let mut got = reg.pids_for(7);
        got.sort_unstable();
        assert_eq!(got, vec![10, 11]);
        assert!(reg
            .instances_for(7)
            .iter()
            .all(|i| i.attribution.is_exact()));
    }

    #[test]
    fn a_client_that_predates_the_launch_is_not_attributed() {
        let mut reg = InstanceRegistry::new();
        let existing = tagged(999, 1_111);
        reg.note_launch(7, 606, 5_000, std::slice::from_ref(&existing), t0());

        let out = reg.sweep(std::slice::from_ref(&existing), at(2));
        assert!(out.attributed.is_empty(), "{out:?}");
        assert_eq!(reg.pending_count(), 1, "the launch should still be waiting");

        let out = reg.sweep(&[existing, tagged(1000, 5_000)], at(5));
        assert_eq!(out.attributed.len(), 1);
        assert_eq!(out.attributed[0].pid, 1000);
    }

    // -----------------------------------------------------------------------
    // The labelled fallback
    // -----------------------------------------------------------------------

    /// When the command line cannot be read at all, the old behaviour is still
    /// there, and it says so.
    #[test]
    fn an_unreadable_client_falls_back_to_appearance_order_and_is_labelled() {
        let mut reg = InstanceRegistry::new();
        reg.note_launch(7, 606, 5_000, &[], t0());

        let out = reg.sweep(&[opaque(1234)], at(4));

        assert_eq!(out.attributed.len(), 1);
        assert_eq!(out.attributed[0].pid, 1234);
        assert_eq!(out.attributed[0].user_id, 7);
        assert_eq!(out.attributed[0].attribution, Attribution::Inferred);
        assert!(!out.attributed[0].attribution.is_exact());
    }

    #[test]
    fn an_unreadable_client_with_nothing_pending_is_unattributed() {
        let mut reg = InstanceRegistry::new();
        let out = reg.sweep(&[opaque(42)], t0());
        assert_eq!(out.unattributed, vec![42]);
        assert!(out.attributed.is_empty());
        assert_eq!(reg.user_for(42), None);
    }

    /// Two unreadable clients in one sweep is the genuinely ambiguous case. It
    /// must still produce two mappings rather than dropping one.
    #[test]
    fn two_unreadable_clients_in_one_sweep_consume_two_pending_launches() {
        let mut reg = InstanceRegistry::new();
        reg.note_launch(11, 606, 5_001, &[], t0());
        reg.note_launch(22, 606, 5_002, &[], at(1));

        let out = reg.sweep(&[opaque(900), opaque(901)], at(6));

        assert_eq!(out.attributed.len(), 2);
        assert!(out.attributed.iter().all(|i| !i.attribution.is_exact()));
        assert_eq!(reg.pending_count(), 0);
        let mut owners: Vec<u64> = out.attributed.iter().map(|i| i.user_id).collect();
        owners.sort_unstable();
        assert_eq!(owners, vec![11, 22]);
    }

    /// A client whose PEB was not readable when it first appeared, and is a
    /// sweep later. The guess becomes a fact rather than staying a guess
    /// forever.
    #[test]
    fn an_inferred_mapping_is_upgraded_once_its_token_can_be_read() {
        let mut reg = InstanceRegistry::new();
        reg.note_launch(7, 606, 5_000, &[], t0());

        let out = reg.sweep(&[opaque(1234)], at(2));
        assert_eq!(out.attributed[0].attribution, Attribution::Inferred);

        let out = reg.sweep(&[tagged(1234, 5_000)], at(4));
        assert_eq!(out.upgraded.len(), 1, "{out:?}");
        assert_eq!(out.upgraded[0].pid, 1234);
        assert_eq!(out.upgraded[0].attribution, Attribution::Exact);
        assert!(reg.instances_for(7)[0].attribution.is_exact());

        // And it does not keep re-reporting the upgrade every sweep.
        let out = reg.sweep(&[tagged(1234, 5_000)], at(6));
        assert!(out.is_empty(), "{out:?}");
    }

    /// A token that does not match leaves the mapping alone rather than
    /// silently "confirming" it.
    #[test]
    fn a_mismatched_token_does_not_upgrade_an_inferred_mapping() {
        let mut reg = InstanceRegistry::new();
        reg.note_launch(7, 606, 5_000, &[], t0());
        reg.sweep(&[opaque(1234)], at(2));

        let out = reg.sweep(&[tagged(1234, 9_999)], at(4));

        assert!(out.upgraded.is_empty(), "{out:?}");
        assert_eq!(reg.instances_for(7)[0].attribution, Attribution::Inferred);
    }

    // -----------------------------------------------------------------------
    // Lifetime
    // -----------------------------------------------------------------------

    #[test]
    fn a_dead_client_is_reaped_and_stops_answering() {
        let mut reg = InstanceRegistry::new();
        reg.note_launch(7, 606, 5_000, &[], t0());
        reg.sweep(&[tagged(1234, 5_000)], at(2));

        let out = reg.sweep(&[], at(30));
        assert_eq!(out.exited.len(), 1);
        assert_eq!(out.exited[0].pid, 1234);
        assert!(reg.pids_for(7).is_empty());
        assert_eq!(reg.user_for(1234), None);
        assert!(reg.snapshot().is_empty());
    }

    #[test]
    fn a_launch_that_never_starts_a_client_expires() {
        let mut reg = InstanceRegistry::new();
        reg.note_launch(7, 606, 5_000, &[], t0());

        let out = reg.sweep(&[], at(PENDING_TTL_SECS - 1));
        assert!(out.abandoned.is_empty(), "expired too early");
        assert_eq!(reg.pending_count(), 1);

        let out = reg.sweep(&[], at(PENDING_TTL_SECS + 1));
        assert_eq!(out.abandoned.len(), 1);
        assert_eq!(out.abandoned[0].user_id, 7);
        assert_eq!(reg.pending_count(), 0);

        // Reported exactly once, not on every subsequent sweep.
        let out = reg.sweep(&[], at(PENDING_TTL_SECS + 3));
        assert!(out.abandoned.is_empty(), "{out:?}");
    }

    /// The point of expiry: a failed launch must not silently claim the
    /// unreadable client the user starts by hand five minutes later.
    #[test]
    fn an_expired_launch_does_not_claim_a_later_unreadable_client() {
        let mut reg = InstanceRegistry::new();
        reg.note_launch(7, 606, 5_000, &[], t0());
        reg.sweep(&[], at(PENDING_TTL_SECS + 1));

        let out = reg.sweep(&[opaque(4242)], at(PENDING_TTL_SECS + 5));
        assert!(out.attributed.is_empty(), "{out:?}");
        assert_eq!(out.unattributed, vec![4242]);
    }

    /// The other half of the two lifetimes: ordering has expired, but the token
    /// has not. A very slow first-run install still lands on its account.
    #[test]
    fn a_very_late_client_is_still_matched_by_its_token() {
        let mut reg = InstanceRegistry::new();
        reg.note_launch(7, 606, 5_000, &[], t0());
        reg.sweep(&[], at(PENDING_TTL_SECS + 1));
        assert_eq!(reg.pending_count(), 0, "no longer eligible by order");
        assert_eq!(reg.matchable_count(), 1, "still eligible by token");

        let out = reg.sweep(&[tagged(1234, 5_000)], at(EXACT_TTL_SECS - 10));
        assert_eq!(out.attributed.len(), 1);
        assert_eq!(out.attributed[0].user_id, 7);
        assert!(out.attributed[0].attribution.is_exact());
    }

    #[test]
    fn a_launch_stops_being_matchable_at_all_eventually() {
        let mut reg = InstanceRegistry::new();
        reg.note_launch(7, 606, 5_000, &[], t0());
        reg.sweep(&[], at(EXACT_TTL_SECS + 1));
        assert_eq!(reg.matchable_count(), 0);

        let out = reg.sweep(&[tagged(1234, 5_000)], at(EXACT_TTL_SECS + 5));
        assert!(out.attributed.is_empty(), "{out:?}");
        assert_eq!(out.unattributed, vec![1234]);
    }

    #[test]
    fn sweeping_a_steady_state_reports_nothing() {
        let mut reg = InstanceRegistry::new();
        reg.note_launch(7, 606, 5_000, &[], t0());
        reg.sweep(&[tagged(1234, 5_000)], at(2));

        let out = reg.sweep(&[tagged(1234, 5_000)], at(4));
        assert!(out.is_empty(), "{out:?}");
    }

    /// This used to be `a_recycled_pid_reads_as_a_brand_new_client`, which
    /// asserted that the registry could not tell a recycled PID apart from a
    /// genuinely new client and would hand it whatever launch was pending.
    ///
    /// Two things now prevent that. Keying on `(pid, start_time)` means the
    /// recycled process is not confused with the original, and the token means
    /// it is only attributed if it actually carries a launch RM issued. Here
    /// the recycled PID belongs to a client the user started by hand, which is
    /// precisely the case the old model got wrong: user 8's launch stays
    /// pending instead of being spent on somebody else's client.
    #[test]
    fn a_recycled_pid_is_not_confused_with_the_client_that_had_it() {
        let mut reg = InstanceRegistry::new();

        // User 7 launches, is given PID 1234, then exits.
        reg.note_launch(7, 606, 5_001, &[], t0());
        reg.sweep(&[tagged(1234, 5_001)], at(2));
        assert_eq!(reg.user_for(1234), Some(7));

        let out = reg.sweep(&[], at(4));
        assert_eq!(out.exited.len(), 1);
        assert_eq!(out.exited[0].user_id, 7);
        assert_eq!(reg.user_for(1234), None);

        // User 8 launches. Meanwhile Windows hands PID 1234 to a client the
        // user started from the website, with a later start time.
        reg.note_launch(8, 606, 5_002, &[], at(5));
        let recycled = LiveClient {
            pid: 1234,
            start_time: 2_000,
            token: LaunchToken::Found(7_777),
        };
        let out = reg.sweep(std::slice::from_ref(&recycled), at(7));

        assert!(out.attributed.is_empty(), "{out:?}");
        assert_eq!(out.unattributed, vec![1234]);
        assert_eq!(reg.user_for(1234), None);
        assert!(reg.pids_for(7).is_empty());
        assert!(reg.pids_for(8).is_empty());
        assert_eq!(reg.pending_count(), 1, "user 8 is still waiting");

        // User 8's real client then arrives and takes its own mapping.
        let out = reg.sweep(&[recycled, tagged(1235, 5_002)], at(9));
        assert_eq!(out.attributed.len(), 1);
        assert_eq!(out.attributed[0].user_id, 8);
        assert_eq!(reg.pids_for(8), vec![1235]);
    }

    /// The same PID reappearing with a different start time is a different
    /// process, so the old mapping is reaped rather than transferred.
    #[test]
    fn a_restarted_process_reusing_a_pid_drops_the_old_mapping() {
        let mut reg = InstanceRegistry::new();
        reg.note_launch(7, 606, 5_001, &[], t0());
        reg.sweep(&[tagged(1234, 5_001)], at(2));

        let replacement = LiveClient {
            pid: 1234,
            start_time: 9_999,
            token: LaunchToken::Absent,
        };
        let out = reg.sweep(&[replacement], at(4));

        assert_eq!(out.exited.len(), 1, "{out:?}");
        assert_eq!(out.exited[0].user_id, 7);
        assert_eq!(out.unattributed, vec![1234]);
        assert_eq!(reg.user_for(1234), None);
    }

    #[test]
    fn clearing_forgets_every_account_id() {
        let mut reg = InstanceRegistry::new();
        reg.note_launch(7, 606, 5_001, &[], t0());
        reg.sweep(&[tagged(1234, 5_001)], at(2));
        reg.note_launch(8, 606, 5_002, &[tagged(1234, 5_001)], at(3));

        reg.clear();
        assert!(reg.snapshot().is_empty());
        assert_eq!(reg.pending_count(), 0);
        assert_eq!(reg.matchable_count(), 0);
        assert_eq!(reg.user_for(1234), None);
        // And a client that was live before the clear is not then attributed to
        // whatever is launched next.
        let out = reg.sweep(&[tagged(1234, 5_001)], at(4));
        assert_eq!(out.unattributed, vec![1234]);
    }
}
