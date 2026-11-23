//! Delivery tracking for commands and settings.
//!
//! Anything sent with a sequence number stays `Pending` until the receiver confirms it.
//! If no confirmation arrives after the configured attempts it becomes `NotSent`, and
//! the UI shows it that way rather than assuming it arrived.
//!
//! Time is passed in as seconds so this works the same on native and in the browser.

use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Failure {
    /// No confirmation after every attempt.
    Timeout,
    /// The receiver refused it.
    Rejected,
    /// The value read back differs from the value sent.
    Mismatch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TxState {
    Pending { attempt: u8 },
    Confirmed,
    NotSent(Failure),
}

#[derive(Debug, Clone, Copy)]
pub struct Policy {
    pub timeout_s: f64,
    pub max_attempts: u8,
}

#[derive(Debug, Clone)]
struct Entry {
    label: String,
    msg_id: u16,
    frame: Vec<u8>,
    attempt: u8,
    deadline: f64,
    state: TxState,
    finished_at: f64,
}

/// What the owner must do after `poll`.
#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    Resend { seq: u16, frame: Vec<u8> },
    Failed { seq: u16, msg_id: u16 },
}

pub struct TxTracker {
    policy: Policy,
    next_seq: u16,
    entries: HashMap<u16, Entry>,
    /// Sequence numbers of the latest send for each label; several for a batch.
    groups: HashMap<String, Vec<u16>>,
}

impl TxTracker {
    pub fn new(policy: Policy) -> Self {
        Self::with_first_seq(policy, 1)
    }

    /// Start numbering somewhere else, so two clients of one backend rarely collide.
    pub fn with_first_seq(policy: Policy, first: u16) -> Self {
        Self { policy, next_seq: first.max(1), entries: HashMap::new(), groups: HashMap::new() }
    }

    /// Next sequence number; never 0, which means "unsolicited" on the wire.
    pub fn next_seq(&mut self) -> u16 {
        let seq = self.next_seq;
        self.next_seq = if seq == u16::MAX { 1 } else { seq + 1 };
        seq
    }

    /// Forget the previous send under `label`, so the label's state reflects only what
    /// is tracked from here on.
    pub fn begin(&mut self, label: &str) {
        if let Some(old) = self.groups.remove(label) {
            for seq in old {
                self.entries.remove(&seq);
            }
        }
    }

    /// Record a frame that has just been sent and now awaits confirmation.
    pub fn track(&mut self, label: &str, seq: u16, msg_id: u16, frame: Vec<u8>, now: f64) {
        self.entries.insert(
            seq,
            Entry {
                label: label.to_owned(),
                msg_id,
                frame,
                attempt: 1,
                deadline: now + self.policy.timeout_s,
                state: TxState::Pending { attempt: 1 },
                finished_at: 0.0,
            },
        );
        self.groups.entry(label.to_owned()).or_default().push(seq);
    }

    /// Settle a pending send. Returns its label if `seq` was pending.
    pub fn resolve(&mut self, seq: u16, result: Result<(), Failure>, now: f64) -> Option<&str> {
        let entry = self.entries.get_mut(&seq)?;
        if !matches!(entry.state, TxState::Pending { .. }) {
            return None;
        }
        entry.state = match result {
            Ok(()) => TxState::Confirmed,
            Err(failure) => TxState::NotSent(failure),
        };
        entry.finished_at = now;
        entry.frame = Vec::new();
        Some(&entry.label)
    }

    pub fn is_pending(&self, seq: u16) -> bool {
        matches!(self.entries.get(&seq), Some(Entry { state: TxState::Pending { .. }, .. }))
    }

    /// Retry or fail whatever has timed out.
    pub fn poll(&mut self, now: f64) -> Vec<Action> {
        let mut actions = Vec::new();
        for (&seq, entry) in self.entries.iter_mut() {
            if !matches!(entry.state, TxState::Pending { .. }) || now < entry.deadline {
                continue;
            }
            if entry.attempt < self.policy.max_attempts {
                entry.attempt += 1;
                entry.deadline = now + self.policy.timeout_s;
                entry.state = TxState::Pending { attempt: entry.attempt };
                actions.push(Action::Resend { seq, frame: entry.frame.clone() });
            } else {
                entry.state = TxState::NotSent(Failure::Timeout);
                entry.finished_at = now;
                entry.frame = Vec::new();
                actions.push(Action::Failed { seq, msg_id: entry.msg_id });
            }
        }
        actions
    }

    /// State of the latest send under `label`. A batch is confirmed only when every
    /// part is, and not sent as soon as any part fails.
    pub fn state(&self, label: &str) -> Option<TxState> {
        let seqs = self.groups.get(label)?;
        let mut pending = None;
        for entry in seqs.iter().filter_map(|s| self.entries.get(s)) {
            match entry.state {
                TxState::NotSent(_) => return Some(entry.state),
                TxState::Pending { attempt } => {
                    pending = Some(pending.map_or(attempt, |a: u8| a.max(attempt)));
                }
                TxState::Confirmed => {}
            }
        }
        Some(pending.map_or(TxState::Confirmed, |attempt| TxState::Pending { attempt }))
    }

    /// Seconds since the latest send under `label` settled, if it has.
    pub fn settled_for(&self, label: &str, now: f64) -> Option<f64> {
        let seqs = self.groups.get(label)?;
        let mut latest = 0.0f64;
        for entry in seqs.iter().filter_map(|s| self.entries.get(s)) {
            if matches!(entry.state, TxState::Pending { .. }) {
                return None;
            }
            latest = latest.max(entry.finished_at);
        }
        Some(now - latest)
    }

    /// Drop settled entries that no label points at any more.
    pub fn prune(&mut self) {
        let live: std::collections::HashSet<u16> = self.groups.values().flatten().copied().collect();
        self.entries.retain(|seq, e| live.contains(seq) || matches!(e.state, TxState::Pending { .. }));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const POLICY: Policy = Policy { timeout_s: 0.5, max_attempts: 4 };

    /// Deterministic stand-in for a lossy radio link.
    struct Lossy(u32);
    impl Lossy {
        fn delivers(&mut self, loss_pct: u32) -> bool {
            self.0 = self.0.wrapping_mul(1664525).wrapping_add(1013904223);
            (self.0 >> 8) % 100 >= loss_pct
        }
    }

    /// Run one send to completion over a link, acking whatever gets through both ways.
    fn run(tracker: &mut TxTracker, label: &str, link: &mut Lossy, loss_pct: u32) -> TxState {
        let seq = tracker.next_seq();
        tracker.begin(label);
        tracker.track(label, seq, 0x0401, vec![1, 2, 3], 0.0);
        let mut in_flight = link.delivers(loss_pct);
        let mut now = 0.0;
        while matches!(tracker.state(label), Some(TxState::Pending { .. })) {
            if in_flight && link.delivers(loss_pct) {
                tracker.resolve(seq, Ok(()), now);
            }
            now += 0.5;
            in_flight = false;
            for action in tracker.poll(now) {
                if let Action::Resend { frame, .. } = action {
                    assert_eq!(frame, vec![1, 2, 3]);
                    in_flight = link.delivers(loss_pct);
                }
            }
        }
        tracker.state(label).unwrap()
    }

    #[test]
    fn retries_get_through_a_lossy_link() {
        let mut tracker = TxTracker::new(POLICY);
        let mut link = Lossy(7);
        let confirmed =
            (0..200).filter(|i| run(&mut tracker, &format!("p{i}"), &mut link, 30) == TxState::Confirmed).count();
        // One attempt succeeds 49% of the time at 30% loss each way; four attempts ~93%.
        assert!(confirmed > 170, "only {confirmed}/200 confirmed");
        assert!(confirmed < 200, "a 30% lossy link should still lose some");
    }

    #[test]
    fn cut_link_reports_not_sent_after_all_attempts() {
        let mut tracker = TxTracker::new(POLICY);
        let seq = tracker.next_seq();
        tracker.track("mode", seq, 0x0301, vec![9], 0.0);
        let mut resends = 0;
        let mut failed = Vec::new();
        for step in 1..=10 {
            for action in tracker.poll(step as f64 * 0.5) {
                match action {
                    Action::Resend { .. } => resends += 1,
                    Action::Failed { seq, msg_id } => failed.push((seq, msg_id)),
                }
            }
        }
        assert_eq!(resends, 3);
        assert_eq!(failed, vec![(seq, 0x0301)]);
        assert_eq!(tracker.state("mode"), Some(TxState::NotSent(Failure::Timeout)));
        // A late ack does not resurrect it.
        assert!(tracker.resolve(seq, Ok(()), 9.0).is_none());
    }

    #[test]
    fn rejection_mismatch_and_batches() {
        let mut tracker = TxTracker::new(POLICY);
        let a = tracker.next_seq();
        tracker.track("param:1", a, 0, vec![], 0.0);
        assert_eq!(tracker.resolve(a, Err(Failure::Mismatch), 0.1), Some("param:1"));
        assert_eq!(tracker.state("param:1"), Some(TxState::NotSent(Failure::Mismatch)));

        tracker.begin("mission");
        let seqs: Vec<u16> = (0..3).map(|_| tracker.next_seq()).collect();
        for &s in &seqs {
            tracker.track("mission", s, 0, vec![], 0.0);
        }
        tracker.resolve(seqs[0], Ok(()), 0.1);
        assert_eq!(tracker.state("mission"), Some(TxState::Pending { attempt: 1 }));
        assert_eq!(tracker.settled_for("mission", 1.0), None);
        tracker.resolve(seqs[1], Ok(()), 0.2);
        tracker.resolve(seqs[2], Ok(()), 0.3);
        assert_eq!(tracker.state("mission"), Some(TxState::Confirmed));
        assert!((tracker.settled_for("mission", 1.3).unwrap() - 1.0).abs() < 1e-9);

        // A new send under the same label replaces the old outcome.
        tracker.begin("mission");
        let s = tracker.next_seq();
        tracker.track("mission", s, 0, vec![], 2.0);
        tracker.resolve(s, Err(Failure::Rejected), 2.1);
        assert_eq!(tracker.state("mission"), Some(TxState::NotSent(Failure::Rejected)));
        assert_eq!(tracker.state("never"), None);
    }

    #[test]
    fn sequence_numbers_skip_zero() {
        let mut tracker = TxTracker::with_first_seq(POLICY, u16::MAX);
        assert_eq!(tracker.next_seq(), u16::MAX);
        assert_eq!(tracker.next_seq(), 1);
    }
}
