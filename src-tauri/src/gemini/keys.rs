//! Actor-owned Gemini key allocation and rotation policy.
//!
//! The actor is the only owner of mutable pool state. Callers communicate
//! through `mpsc` commands and receive replies through `oneshot`; no shared
//! mutex guards the pool.

use std::collections::{HashMap, VecDeque};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use specta::Type;
use tokio::sync::{mpsc, oneshot, watch, Mutex};
use tokio::time::Instant;
use uuid::Uuid;

use crate::core::error::{AppError, Code};
use crate::core::Sensitive;
use crate::secrets::{CredentialStore, KeyId, KeyMaterial, SecretService};

use super::params::{
    JOB_MAX_WAIT, MAX_ATTEMPTS, MEMO_DEADLINE, QUOTA_COOLDOWN, SERVER_BACKOFF, SERVER_BACKOFF_MAX,
};

pub trait KeyProvider: Send + Sync + 'static {
    fn load_keys(&self) -> Result<Vec<KeyMaterial>, AppError>;
}

impl<B: CredentialStore> KeyProvider for SecretService<B> {
    fn load_keys(&self) -> Result<Vec<KeyMaterial>, AppError> {
        self.materials()
    }
}

pub trait Clock: Send + Sync + 'static {
    fn now(&self) -> Instant;
    fn sleep_until(&self, deadline: Instant) -> Pin<Box<dyn Future<Output = ()> + Send + '_>>;
}

#[derive(Debug, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> Instant {
        Instant::now()
    }

    fn sleep_until(&self, deadline: Instant) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
        Box::pin(tokio::time::sleep_until(deadline))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "lowercase")]
pub enum Priority {
    Live,
    Job,
    Memo,
}

impl Priority {
    fn default_budget(self) -> Duration {
        match self {
            Self::Live | Self::Job => JOB_MAX_WAIT,
            Self::Memo => MEMO_DEADLINE,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestOutcome {
    Success,
    Quota,
    Auth,
    Request,
    Server,
    Timeout,
}

#[derive(PartialEq, Eq)]
pub struct KeyLease {
    pub request_id: String,
    pub key_id: KeyId,
    pub secret: Sensitive<String>,
    generation: u64,
    attempt: u8,
    priority: Priority,
}

impl KeyLease {
    /// Attempt number for this lease, starting at 1. Exposed read-only so
    /// callers (story 2.4 progress observer) can report it without touching
    /// the actor's private retry bookkeeping.
    pub fn attempt(&self) -> u8 {
        self.attempt
    }
}

impl std::fmt::Debug for KeyLease {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("KeyLease")
            .field("request_id", &self.request_id)
            .field("key_id", &self.key_id)
            .field("secret", &self.secret)
            .field("generation", &self.generation)
            .field("attempt", &self.attempt)
            .field("priority", &self.priority)
            .finish()
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum ReportAction {
    Complete,
    Retry(KeyLease),
}

#[derive(Clone)]
pub struct KeyPoolHandle {
    commands: mpsc::Sender<Command>,
    provider: Arc<dyn KeyProvider>,
    refresh_lock: Arc<Mutex<()>>,
    initialized: watch::Sender<bool>,
}

impl KeyPoolHandle {
    pub fn channel(provider: Arc<dyn KeyProvider>, clock: Arc<dyn Clock>) -> (Self, KeyPoolActor) {
        let (commands, receiver) = mpsc::channel(64);
        let (initialized, _) = watch::channel(false);
        (
            Self {
                commands,
                provider,
                refresh_lock: Arc::new(Mutex::new(())),
                initialized,
            },
            KeyPoolActor::new(receiver, clock),
        )
    }

    pub async fn refresh(&self) -> Result<(), AppError> {
        // Keep each provider snapshot paired with its actor replacement. A
        // slower earlier load must never replace a newer credential snapshot.
        let _refresh_guard = self.refresh_lock.lock().await;
        let result = async {
            let provider = self.provider.clone();
            let loaded = tokio::task::spawn_blocking(move || provider.load_keys())
                .await
                .map_err(|_| actor_error())?;
            match loaded {
                Ok(keys) => self.replace(keys).await,
                Err(error) => {
                    // Fail closed: a store failure must never leave a deleted or
                    // stale secret allocatable from the in-memory pool.
                    self.replace(Vec::new()).await?;
                    Err(error)
                }
            }
        }
        .await;
        // A failed initial read is still a completed initialization: callers
        // must see the fail-closed pool instead of waiting indefinitely.
        self.initialized.send_replace(true);
        result
    }

    async fn wait_initialized(&self) -> Result<(), AppError> {
        let mut ready = self.initialized.subscribe();
        while !*ready.borrow() {
            ready.changed().await.map_err(|_| actor_error())?;
        }
        Ok(())
    }

    async fn replace(&self, keys: Vec<KeyMaterial>) -> Result<(), AppError> {
        let (reply, response) = oneshot::channel();
        self.commands
            .send(Command::Replace { keys, reply })
            .await
            .map_err(|_| actor_error())?;
        response.await.map_err(|_| actor_error())
    }

    pub async fn acquire(&self, priority: Priority) -> Result<KeyLease, AppError> {
        self.acquire_with_budget(priority, priority.default_budget())
            .await
    }

    pub async fn acquire_with_budget(
        &self,
        priority: Priority,
        budget: Duration,
    ) -> Result<KeyLease, AppError> {
        self.wait_initialized().await?;
        let (reply, response) = oneshot::channel();
        self.commands
            .send(Command::Acquire {
                priority,
                budget,
                reply,
            })
            .await
            .map_err(|_| actor_error())?;
        response.await.map_err(|_| actor_error())?
    }

    /// Acquire one exact key for a validation request.  A targeted lease
    /// never falls back to another key, even when the requested key is
    /// disabled; a successful targeted report is what clears that quarantine.
    pub async fn acquire_for_key(
        &self,
        key_id: &KeyId,
        priority: Priority,
        budget: Duration,
    ) -> Result<KeyLease, AppError> {
        self.wait_initialized().await?;
        let (reply, response) = oneshot::channel();
        self.commands
            .send(Command::AcquireForKey {
                key_id: key_id.clone(),
                priority,
                budget,
                reply,
            })
            .await
            .map_err(|_| actor_error())?;
        response.await.map_err(|_| actor_error())?
    }

    pub async fn report(
        &self,
        lease: KeyLease,
        outcome: RequestOutcome,
    ) -> Result<ReportAction, AppError> {
        let (reply, response) = oneshot::channel();
        self.commands
            .send(Command::Report {
                lease,
                outcome,
                reply,
            })
            .await
            .map_err(|_| actor_error())?;
        response.await.map_err(|_| actor_error())?
    }

    /// Report the result of an exact-key validation request.  This path only
    /// changes the state of the leased key and never creates a retry lease.
    pub async fn report_for_key(
        &self,
        lease: KeyLease,
        outcome: RequestOutcome,
    ) -> Result<(), AppError> {
        let (reply, response) = oneshot::channel();
        self.commands
            .send(Command::ReportForKey {
                lease,
                outcome,
                reply,
            })
            .await
            .map_err(|_| actor_error())?;
        response.await.map_err(|_| actor_error())?
    }

    /// 1-based position of `key_id` in the actor's current key list, or
    /// `None` if that key is no longer present. Used only to report a
    /// non-identifying "key ordinal" to a [`super::JobObserver`] (story 2.4)
    /// — never returns the key material or its opaque id.
    pub async fn ordinal_of(&self, key_id: &KeyId) -> Option<u32> {
        let (reply, response) = oneshot::channel();
        if self
            .commands
            .send(Command::Ordinal {
                key_id: key_id.clone(),
                reply,
            })
            .await
            .is_err()
        {
            return None;
        }
        response.await.ok().flatten()
    }

    pub async fn cancel(&self, request_id: impl Into<String>) -> Result<(), AppError> {
        let (reply, response) = oneshot::channel();
        self.commands
            .send(Command::Cancel {
                request_id: request_id.into(),
                reply,
            })
            .await
            .map_err(|_| actor_error())?;
        response.await.map_err(|_| actor_error())
    }
}

fn actor_error() -> AppError {
    AppError::new(Code::Storage, "key pool actor unavailable")
}

enum Command {
    Replace {
        keys: Vec<KeyMaterial>,
        reply: oneshot::Sender<()>,
    },
    Acquire {
        priority: Priority,
        budget: Duration,
        reply: oneshot::Sender<Result<KeyLease, AppError>>,
    },
    AcquireForKey {
        key_id: KeyId,
        priority: Priority,
        budget: Duration,
        reply: oneshot::Sender<Result<KeyLease, AppError>>,
    },
    Report {
        lease: KeyLease,
        outcome: RequestOutcome,
        reply: oneshot::Sender<Result<ReportAction, AppError>>,
    },
    ReportForKey {
        lease: KeyLease,
        outcome: RequestOutcome,
        reply: oneshot::Sender<Result<(), AppError>>,
    },
    Cancel {
        request_id: String,
        reply: oneshot::Sender<()>,
    },
    Ordinal {
        key_id: KeyId,
        reply: oneshot::Sender<Option<u32>>,
    },
}

struct KeyState {
    material: KeyMaterial,
    cooldown_until: Option<Instant>,
    disabled: bool,
}

struct RequestState {
    priority: Priority,
    deadline: Instant,
    attempts: u8,
    target_key: Option<KeyId>,
}

struct Pending {
    request_id: String,
    reply: PendingReply,
}

enum PendingReply {
    Acquire(oneshot::Sender<Result<KeyLease, AppError>>),
    Retry(oneshot::Sender<Result<ReportAction, AppError>>),
}

impl PendingReply {
    fn is_closed(&self) -> bool {
        match self {
            Self::Acquire(reply) => reply.is_closed(),
            Self::Retry(reply) => reply.is_closed(),
        }
    }

    fn send(self, result: Result<KeyLease, AppError>) {
        match self {
            Self::Acquire(reply) => {
                let _ = reply.send(result);
            }
            Self::Retry(reply) => {
                let _ = reply.send(result.map(ReportAction::Retry));
            }
        }
    }
}

pub struct KeyPoolActor {
    receiver: mpsc::Receiver<Command>,
    clock: Arc<dyn Clock>,
    generation: u64,
    keys: Vec<KeyState>,
    cursor: usize,
    requests: HashMap<String, RequestState>,
    pending_live: VecDeque<Pending>,
    pending_job: VecDeque<Pending>,
    pending_memo: VecDeque<Pending>,
    job_blocked_until: Option<Instant>,
}

impl KeyPoolActor {
    fn new(receiver: mpsc::Receiver<Command>, clock: Arc<dyn Clock>) -> Self {
        Self {
            receiver,
            clock,
            generation: 0,
            keys: Vec::new(),
            cursor: 0,
            requests: HashMap::new(),
            pending_live: VecDeque::new(),
            pending_job: VecDeque::new(),
            pending_memo: VecDeque::new(),
            job_blocked_until: None,
        }
    }

    pub async fn run(mut self) {
        loop {
            self.process_pending();
            let Some(wakeup) = self.next_wakeup() else {
                match self.receiver.recv().await {
                    Some(command) => self.handle(command),
                    None => break,
                }
                continue;
            };

            tokio::select! {
                command = self.receiver.recv() => match command {
                    Some(command) => self.handle(command),
                    None => break,
                },
                _ = self.clock.sleep_until(wakeup) => {}
            }
        }
    }

    fn handle(&mut self, command: Command) {
        match command {
            Command::Replace { keys, reply } => {
                self.generation = self.generation.wrapping_add(1);
                let previous = std::mem::take(&mut self.keys);
                self.keys = keys
                    .into_iter()
                    .map(|material| {
                        let retained = previous.iter().find(|state| {
                            state.material.id == material.id
                                && state.material.secret == material.secret
                        });
                        KeyState {
                            material,
                            cooldown_until: retained.and_then(|state| state.cooldown_until),
                            disabled: retained.is_some_and(|state| state.disabled),
                        }
                    })
                    .collect();
                self.cursor = 0;
                let _ = reply.send(());
            }
            Command::Acquire {
                priority,
                budget,
                reply,
            } => {
                let now = self.clock.now();
                let budget = budget.min(priority.default_budget());
                let request_id = Uuid::now_v7().to_string();
                self.requests.insert(
                    request_id.clone(),
                    RequestState {
                        priority,
                        deadline: now + budget,
                        attempts: 0,
                        target_key: None,
                    },
                );
                self.enqueue(Pending {
                    request_id,
                    reply: PendingReply::Acquire(reply),
                });
            }
            Command::AcquireForKey {
                key_id,
                priority,
                budget,
                reply,
            } => {
                let now = self.clock.now();
                let budget = budget.min(priority.default_budget());
                let request_id = Uuid::now_v7().to_string();
                self.requests.insert(
                    request_id.clone(),
                    RequestState {
                        priority,
                        deadline: now + budget,
                        attempts: 0,
                        target_key: Some(key_id),
                    },
                );
                self.enqueue(Pending {
                    request_id,
                    reply: PendingReply::Acquire(reply),
                });
            }
            Command::Report {
                lease,
                outcome,
                reply,
            } => self.handle_report(lease, outcome, reply),
            Command::ReportForKey {
                lease,
                outcome,
                reply,
            } => self.handle_report_for_key(lease, outcome, reply),
            Command::Cancel { request_id, reply } => {
                self.requests.remove(&request_id);
                self.cancel_pending(&request_id);
                let _ = reply.send(());
            }
            Command::Ordinal { key_id, reply } => {
                let ordinal = self
                    .keys
                    .iter()
                    .position(|key| key.material.id == key_id)
                    .map(|index| (index + 1) as u32);
                let _ = reply.send(ordinal);
            }
        }
    }

    fn handle_report_for_key(
        &mut self,
        lease: KeyLease,
        outcome: RequestOutcome,
        reply: oneshot::Sender<Result<(), AppError>>,
    ) {
        let current = lease.generation == self.generation
            && self.requests.get(&lease.request_id).is_some_and(|request| {
                request.target_key.as_ref() == Some(&lease.key_id)
                    && request.attempts == lease.attempt
            })
            && self.keys.iter().any(|key| key.material.id == lease.key_id);
        if !current {
            let _ = reply.send(Err(AppError::new(
                Code::Auth,
                "key validation lease is no longer current",
            )));
            return;
        }

        self.requests.remove(&lease.request_id);
        let key = self
            .keys
            .iter_mut()
            .find(|key| key.material.id == lease.key_id)
            .expect("current targeted lease must have a matching key");

        let result = match outcome {
            RequestOutcome::Success => {
                // A successful check is the only event allowed to restore a
                // key quarantined by a previous auth failure.
                key.disabled = false;
                key.cooldown_until = None;
                Ok(())
            }
            RequestOutcome::Quota => {
                key.cooldown_until = Some(self.clock.now() + QUOTA_COOLDOWN);
                Err(AppError::new(Code::Quota, "Gemini key is rate limited"))
            }
            RequestOutcome::Auth => {
                key.disabled = true;
                Err(AppError::new(Code::Auth, "Gemini key was rejected"))
            }
            RequestOutcome::Request => Err(AppError::new(
                Code::Request,
                "Gemini key validation request was rejected",
            )),
            RequestOutcome::Server => Err(AppError::new(
                Code::Network,
                "Gemini service is unavailable",
            )),
            RequestOutcome::Timeout => Err(AppError::new(
                Code::Timeout,
                "Gemini key validation timed out",
            )),
        };
        let _ = reply.send(result);
    }

    fn handle_report(
        &mut self,
        lease: KeyLease,
        outcome: RequestOutcome,
        reply: oneshot::Sender<Result<ReportAction, AppError>>,
    ) {
        let current = lease.generation == self.generation
            && self
                .requests
                .get(&lease.request_id)
                .is_some_and(|request| request.attempts == lease.attempt)
            && self.keys.iter().any(|key| key.material.id == lease.key_id);
        if !current {
            let _ = reply.send(Err(AppError::new(
                Code::Auth,
                "key lease is no longer current",
            )));
            return;
        }

        match outcome {
            RequestOutcome::Success => {
                self.requests.remove(&lease.request_id);
                let _ = reply.send(Ok(ReportAction::Complete));
            }
            RequestOutcome::Request => {
                self.requests.remove(&lease.request_id);
                let _ = reply.send(Err(AppError::new(
                    Code::Request,
                    "request rejected without key rotation",
                )));
            }
            RequestOutcome::Timeout => {
                self.requests.remove(&lease.request_id);
                let _ = reply.send(Err(AppError::new(
                    Code::Timeout,
                    "request timed out without key rotation",
                )));
            }
            RequestOutcome::Server => {
                // Same `cooldown_until` mechanism as `Quota`, scaled by the
                // attempts already spent on this request (1s × attempts so
                // far, capped at `SERVER_BACKOFF_MAX`) so a flaky key does not
                // get re-dispatched to immediately.
                let backoff = SERVER_BACKOFF
                    .saturating_mul(u32::from(lease.attempt))
                    .min(SERVER_BACKOFF_MAX);
                if let Some(key) = self
                    .keys
                    .iter_mut()
                    .find(|key| key.material.id == lease.key_id)
                {
                    key.cooldown_until = Some(self.clock.now() + backoff);
                }
                self.retry_or_finish(lease.request_id, Code::Network, reply);
            }
            RequestOutcome::Quota => {
                let until = self.clock.now() + QUOTA_COOLDOWN;
                if let Some(key) = self
                    .keys
                    .iter_mut()
                    .find(|key| key.material.id == lease.key_id)
                {
                    key.cooldown_until = Some(until);
                }
                if lease.priority == Priority::Live {
                    self.job_blocked_until = Some(
                        self.job_blocked_until
                            .map_or(until, |current| current.max(until)),
                    );
                }
                self.retry_or_finish(lease.request_id, Code::Quota, reply);
            }
            RequestOutcome::Auth => {
                if let Some(key) = self
                    .keys
                    .iter_mut()
                    .find(|key| key.material.id == lease.key_id)
                {
                    key.disabled = true;
                }
                self.retry_or_finish(lease.request_id, Code::Auth, reply);
            }
        }
    }

    fn retry_or_finish(
        &mut self,
        request_id: String,
        exhausted_code: Code,
        reply: oneshot::Sender<Result<ReportAction, AppError>>,
    ) {
        let exhausted = self
            .requests
            .get(&request_id)
            .map(|request| request.attempts >= MAX_ATTEMPTS)
            .unwrap_or(true);
        if exhausted {
            self.requests.remove(&request_id);
            let _ = reply.send(Err(AppError::new(
                exhausted_code,
                "key request exhausted its attempt budget",
            )));
            return;
        }
        self.enqueue(Pending {
            request_id,
            reply: PendingReply::Retry(reply),
        });
    }

    fn enqueue(&mut self, pending: Pending) {
        let Some(request) = self.requests.get(&pending.request_id) else {
            return;
        };
        match request.priority {
            Priority::Live => self.pending_live.push_back(pending),
            Priority::Job => self.pending_job.push_back(pending),
            Priority::Memo => self.pending_memo.push_back(pending),
        }
    }

    fn process_pending(&mut self) {
        let mut deferred = Vec::new();
        loop {
            let now = self.clock.now();
            let Some(pending) = self.pop_pending(now) else {
                break;
            };
            if pending.reply.is_closed() {
                self.requests.remove(&pending.request_id);
                continue;
            }

            let Some(request) = self.requests.get(&pending.request_id) else {
                continue;
            };
            let deadline = request.deadline;
            let priority = request.priority;
            let target_key = request.target_key.clone();
            if now >= deadline {
                self.requests.remove(&pending.request_id);
                pending.reply.send(Err(AppError::new(
                    if target_key.is_some() {
                        Code::Timeout
                    } else {
                        Code::Quota
                    },
                    if target_key.is_some() {
                        "key validation deadline elapsed"
                    } else {
                        "key request deadline elapsed"
                    },
                )));
                continue;
            }
            let key = target_key
                .as_ref()
                .and_then(|key_id| self.next_key_for(key_id, now))
                .or_else(|| {
                    if target_key.is_none() {
                        self.next_key(now)
                    } else {
                        None
                    }
                });
            match key {
                Some(material) => {
                    let request = self
                        .requests
                        .get_mut(&pending.request_id)
                        .expect("request checked above");
                    request.attempts += 1;
                    let lease = KeyLease {
                        request_id: pending.request_id,
                        key_id: material.id,
                        secret: material.secret,
                        generation: self.generation,
                        attempt: request.attempts,
                        priority,
                    };
                    pending.reply.send(Ok(lease));
                }
                None if target_key.is_some()
                    && !self.keys.iter().any(|key| {
                        target_key
                            .as_ref()
                            .is_some_and(|target| &key.material.id == target)
                    }) =>
                {
                    self.requests.remove(&pending.request_id);
                    pending.reply.send(Err(AppError::new(
                        Code::Auth,
                        "requested Gemini key is no longer available",
                    )));
                }
                None if target_key.is_none() && self.keys.iter().all(|key| key.disabled) => {
                    self.requests.remove(&pending.request_id);
                    pending.reply.send(Err(AppError::new(
                        Code::Auth,
                        "no usable Gemini key remains",
                    )));
                }
                None => {
                    deferred.push(pending);
                }
            }
        }
        for pending in deferred {
            self.enqueue(pending);
        }
    }

    fn next_key(&mut self, now: Instant) -> Option<KeyMaterial> {
        if self.keys.is_empty() {
            return None;
        }
        for offset in 0..self.keys.len() {
            let index = (self.cursor + offset) % self.keys.len();
            let key = &self.keys[index];
            let cooling = key.cooldown_until.is_some_and(|until| now < until);
            if !key.disabled && !cooling {
                self.cursor = (index + 1) % self.keys.len();
                return Some(key.material.clone());
            }
        }
        None
    }

    fn next_key_for(&self, key_id: &KeyId, now: Instant) -> Option<KeyMaterial> {
        self.keys.iter().find_map(|key| {
            if &key.material.id == key_id && !key.cooldown_until.is_some_and(|until| now < until) {
                Some(key.material.clone())
            } else {
                None
            }
        })
    }

    fn pop_pending(&mut self, now: Instant) -> Option<Pending> {
        self.pending_live.pop_front().or_else(|| {
            let job_blocked = self.job_blocked_until.is_some_and(|until| now < until);
            if job_blocked {
                self.pending_memo.pop_front()
            } else {
                self.pending_job
                    .pop_front()
                    .or_else(|| self.pending_memo.pop_front())
            }
        })
    }

    fn cancel_pending(&mut self, request_id: &str) {
        for queue in [
            &mut self.pending_live,
            &mut self.pending_job,
            &mut self.pending_memo,
        ] {
            if let Some(index) = queue
                .iter()
                .position(|pending| pending.request_id == request_id)
            {
                if let Some(pending) = queue.remove(index) {
                    pending.reply.send(Err(AppError::new(
                        Code::Blocked,
                        "key request was cancelled",
                    )));
                }
                return;
            }
        }
    }

    fn next_wakeup(&self) -> Option<Instant> {
        if self.pending_live.is_empty()
            && self.pending_job.is_empty()
            && self.pending_memo.is_empty()
        {
            return None;
        }
        let deadlines = self
            .requests
            .values()
            .map(|request| request.deadline)
            .chain(
                self.keys
                    .iter()
                    .filter_map(|key| key.cooldown_until)
                    .chain(self.job_blocked_until),
            );
        deadlines.min()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::secrets::FakeCredentialStore;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Mutex;
    use tokio::sync::Notify;

    struct FakeProvider {
        keys: Mutex<Vec<KeyMaterial>>,
    }

    impl FakeProvider {
        fn new(keys: Vec<KeyMaterial>) -> Self {
            Self {
                keys: Mutex::new(keys),
            }
        }

        fn replace(&self, keys: Vec<KeyMaterial>) {
            *self.keys.lock().unwrap() = keys;
        }
    }

    impl KeyProvider for FakeProvider {
        fn load_keys(&self) -> Result<Vec<KeyMaterial>, AppError> {
            Ok(self.keys.lock().unwrap().clone())
        }
    }

    struct FakeClock {
        now: Mutex<Instant>,
        changed: Notify,
    }

    impl FakeClock {
        fn new() -> Self {
            Self {
                now: Mutex::new(Instant::now()),
                changed: Notify::new(),
            }
        }

        fn advance(&self, duration: Duration) {
            *self.now.lock().unwrap() += duration;
            self.changed.notify_waiters();
        }
    }

    impl Clock for FakeClock {
        fn now(&self) -> Instant {
            *self.now.lock().unwrap()
        }

        fn sleep_until(&self, deadline: Instant) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
            Box::pin(async move {
                while self.now() < deadline {
                    self.changed.notified().await;
                }
            })
        }
    }

    fn material(id: &str, suffix: &str) -> KeyMaterial {
        KeyMaterial::new(KeyId::from_opaque(id), format!("AIzaSyKEY{suffix}1234"))
    }

    async fn pool(keys: Vec<KeyMaterial>) -> (KeyPoolHandle, Arc<FakeProvider>, Arc<FakeClock>) {
        let provider = Arc::new(FakeProvider::new(keys));
        let clock = Arc::new(FakeClock::new());
        let (handle, actor) = KeyPoolHandle::channel(provider.clone(), clock.clone());
        tokio::spawn(actor.run());
        handle.refresh().await.unwrap();
        (handle, provider, clock)
    }

    #[tokio::test]
    async fn quota_rotates_then_waits_for_cooldown_without_resetting_budget() {
        let (pool, _provider, clock) = pool(vec![material("a", "A"), material("b", "B")]).await;
        let first = pool
            .acquire_with_budget(Priority::Job, Duration::from_secs(180))
            .await
            .unwrap();
        let second = match pool.report(first, RequestOutcome::Quota).await.unwrap() {
            ReportAction::Retry(lease) => lease,
            ReportAction::Complete => panic!("quota must retry"),
        };
        assert_ne!(second.key_id, KeyId::from_opaque("a"));

        let wait = pool.report(second, RequestOutcome::Quota);
        tokio::pin!(wait);
        tokio::task::yield_now().await;
        assert!(tokio::time::timeout(Duration::from_millis(1), &mut wait)
            .await
            .is_err());
        clock.advance(QUOTA_COOLDOWN);
        assert!(matches!(wait.await.unwrap(), ReportAction::Retry(_)));
    }

    #[tokio::test]
    async fn auth_disables_each_key_and_exhaustion_is_immediate() {
        let (pool, _provider, _clock) = pool(vec![material("a", "A"), material("b", "B")]).await;
        let first = pool.acquire(Priority::Job).await.unwrap();
        let second = match pool.report(first, RequestOutcome::Auth).await.unwrap() {
            ReportAction::Retry(lease) => lease,
            _ => panic!("auth should try another key"),
        };
        let error = pool.report(second, RequestOutcome::Auth).await.unwrap_err();
        assert_eq!(error.code, Code::Auth);
    }

    #[tokio::test]
    async fn request_and_timeout_never_rotate() {
        for outcome in [RequestOutcome::Request, RequestOutcome::Timeout] {
            let (pool, _provider, _clock) =
                pool(vec![material("a", "A"), material("b", "B")]).await;
            let lease = pool.acquire(Priority::Job).await.unwrap();
            let error = pool.report(lease, outcome).await.unwrap_err();
            assert!(matches!(error.code, Code::Request | Code::Timeout));
        }
    }

    #[tokio::test]
    async fn server_failures_retry_serially_within_the_attempt_budget() {
        let (pool, _provider, clock) = pool(vec![material("a", "A")]).await;
        let mut lease = pool.acquire(Priority::Job).await.unwrap();
        let mut attempts = 1;
        loop {
            let reporting_pool = pool.clone();
            let report =
                tokio::spawn(
                    async move { reporting_pool.report(lease, RequestOutcome::Server).await },
                );
            tokio::task::yield_now().await;
            // Only the retryable attempts are gated behind the backoff; the
            // exhausted (4th) reply returns immediately regardless, so
            // advancing here is harmless in that case.
            clock.advance(SERVER_BACKOFF_MAX);
            match report.await.unwrap() {
                Ok(ReportAction::Retry(next)) => {
                    attempts += 1;
                    lease = next;
                }
                Err(error) => {
                    assert_eq!(error.code, Code::Network);
                    break;
                }
                Ok(ReportAction::Complete) => panic!("server failure must retry"),
            }
        }
        assert_eq!(attempts, MAX_ATTEMPTS);
    }

    #[tokio::test]
    async fn server_failure_backoff_gates_the_retry_and_scales_with_attempts() {
        let (pool, _provider, clock) = pool(vec![material("a", "A")]).await;
        let first = pool.acquire(Priority::Job).await.unwrap();

        let reporting_pool = pool.clone();
        let first_wait =
            tokio::spawn(async move { reporting_pool.report(first, RequestOutcome::Server).await });
        tokio::task::yield_now().await;
        assert!(
            !first_wait.is_finished(),
            "first retry must wait for its 1s backoff"
        );
        clock.advance(SERVER_BACKOFF);
        let second = match first_wait.await.unwrap().unwrap() {
            ReportAction::Retry(lease) => lease,
            ReportAction::Complete => panic!("server failure must retry"),
        };
        assert_eq!(second.attempt(), 2);

        let reporting_pool = pool.clone();
        let second_wait =
            tokio::spawn(
                async move { reporting_pool.report(second, RequestOutcome::Server).await },
            );
        tokio::task::yield_now().await;
        // 2nd attempt's backoff scales to 2s; 1s must not be enough yet.
        clock.advance(SERVER_BACKOFF);
        tokio::task::yield_now().await;
        assert!(
            !second_wait.is_finished(),
            "second retry's backoff scales with attempts already spent"
        );
        clock.advance(SERVER_BACKOFF);
        let third = match second_wait.await.unwrap().unwrap() {
            ReportAction::Retry(lease) => lease,
            ReportAction::Complete => panic!("server failure must retry"),
        };
        assert_eq!(third.attempt(), 3);
    }

    #[tokio::test]
    async fn refresh_invalidates_an_inflight_lease_and_removed_key_never_returns() {
        let (pool, provider, _clock) = pool(vec![material("a", "A"), material("b", "B")]).await;
        let stale = pool.acquire(Priority::Live).await.unwrap();
        provider.replace(vec![material("b", "B")]);
        pool.refresh().await.unwrap();
        assert_eq!(
            pool.report(stale, RequestOutcome::Success)
                .await
                .unwrap_err()
                .code,
            Code::Auth
        );
        let next = pool.acquire(Priority::Live).await.unwrap();
        assert_eq!(next.key_id, KeyId::from_opaque("b"));
    }

    #[tokio::test]
    async fn concurrent_refreshes_cannot_restore_an_older_snapshot() {
        struct BlockingProvider {
            keys: Mutex<Vec<KeyMaterial>>,
            block_first: AtomicBool,
            started: std::sync::mpsc::Sender<()>,
            release: Mutex<std::sync::mpsc::Receiver<()>>,
        }

        impl KeyProvider for BlockingProvider {
            fn load_keys(&self) -> Result<Vec<KeyMaterial>, AppError> {
                let snapshot = self.keys.lock().unwrap().clone();
                if self.block_first.swap(false, Ordering::SeqCst) {
                    self.started.send(()).unwrap();
                    self.release.lock().unwrap().recv().unwrap();
                }
                Ok(snapshot)
            }
        }

        let (started_tx, started_rx) = std::sync::mpsc::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let provider = Arc::new(BlockingProvider {
            keys: Mutex::new(vec![material("old", "OLD")]),
            block_first: AtomicBool::new(true),
            started: started_tx,
            release: Mutex::new(release_rx),
        });
        let (pool, actor) = KeyPoolHandle::channel(provider.clone(), Arc::new(SystemClock));
        tokio::spawn(actor.run());

        let first_pool = pool.clone();
        let first = tokio::spawn(async move { first_pool.refresh().await });
        tokio::task::spawn_blocking(move || started_rx.recv().unwrap())
            .await
            .unwrap();
        *provider.keys.lock().unwrap() = vec![material("new", "NEW")];
        let second_pool = pool.clone();
        let mut second = tokio::spawn(async move { second_pool.refresh().await });
        assert!(tokio::time::timeout(Duration::from_millis(20), &mut second)
            .await
            .is_err());
        assert!(
            tokio::time::timeout(Duration::from_millis(20), pool.acquire(Priority::Job))
                .await
                .is_err()
        );
        release_tx.send(()).unwrap();
        first.await.unwrap().unwrap();
        second.await.unwrap().unwrap();
        let lease = pool.acquire(Priority::Live).await.unwrap();
        assert_eq!(lease.key_id, KeyId::from_opaque("new"));
    }

    #[tokio::test]
    async fn cooling_target_does_not_block_another_available_target() {
        let (pool, _provider, _clock) = pool(vec![material("a", "A"), material("b", "B")]).await;
        let a = KeyId::from_opaque("a");
        let b = KeyId::from_opaque("b");
        let first = pool
            .acquire_for_key(&a, Priority::Job, Duration::from_secs(5))
            .await
            .unwrap();
        assert!(pool
            .report_for_key(first, RequestOutcome::Quota)
            .await
            .is_err());

        let waiting_pool = pool.clone();
        let waiting = tokio::spawn(async move {
            waiting_pool
                .acquire_for_key(&a, Priority::Job, Duration::from_secs(5))
                .await
        });
        tokio::task::yield_now().await;
        let available = tokio::time::timeout(
            Duration::from_millis(100),
            pool.acquire_for_key(&b, Priority::Job, Duration::from_secs(5)),
        )
        .await
        .expect("available key must not wait behind cooling key")
        .unwrap();
        assert_eq!(available.key_id, b);
        waiting.abort();
    }

    #[tokio::test]
    async fn live_quota_barrier_blocks_job_but_not_memo() {
        let (pool, _provider, clock) = pool(vec![material("a", "A"), material("b", "B")]).await;
        let live = pool.acquire(Priority::Live).await.unwrap();
        let _ = pool.report(live, RequestOutcome::Quota).await.unwrap();

        let job = pool.acquire(Priority::Job);
        tokio::pin!(job);
        tokio::task::yield_now().await;
        assert!(tokio::time::timeout(Duration::from_millis(1), &mut job)
            .await
            .is_err());
        let memo = pool.acquire(Priority::Memo).await.unwrap();
        assert_eq!(memo.priority, Priority::Memo);
        clock.advance(QUOTA_COOLDOWN);
        assert!(job.await.is_ok());
    }

    #[test]
    fn closed_waiter_is_pruned_without_consuming_a_key() {
        let provider = Arc::new(FakeProvider::new(Vec::new()));
        let clock = Arc::new(FakeClock::new());
        let (_handle, mut actor) = KeyPoolHandle::channel(provider, clock.clone());
        actor.requests.insert(
            "closed".to_string(),
            RequestState {
                priority: Priority::Job,
                deadline: clock.now() + Duration::from_secs(30),
                attempts: 0,
                target_key: None,
            },
        );
        let (reply, response) = oneshot::channel();
        drop(response);
        actor.pending_job.push_back(Pending {
            request_id: "closed".to_string(),
            reply: PendingReply::Acquire(reply),
        });
        actor.keys.push(KeyState {
            material: material("a", "A"),
            cooldown_until: None,
            disabled: false,
        });

        actor.process_pending();
        assert!(!actor.requests.contains_key("closed"));
        assert!(actor.pending_job.is_empty());
    }

    #[tokio::test]
    async fn explicit_cancel_unblocks_a_request_waiting_for_quota() {
        let (pool, _provider, _clock) = pool(vec![material("a", "A")]).await;
        let lease = pool.acquire(Priority::Job).await.unwrap();
        let request_id = lease.request_id.clone();
        let reporting_pool = pool.clone();
        let report =
            tokio::spawn(async move { reporting_pool.report(lease, RequestOutcome::Quota).await });
        tokio::task::yield_now().await;

        pool.cancel(request_id).await.unwrap();
        let error = report.await.unwrap().unwrap_err();
        assert_eq!(error.code, Code::Blocked);
    }

    #[tokio::test]
    async fn total_budget_does_not_reset_after_a_retry() {
        let (pool, _provider, clock) = pool(vec![material("a", "A")]).await;
        let first = pool
            .acquire_with_budget(Priority::Job, Duration::from_secs(90))
            .await
            .unwrap();
        let reporting_pool = pool.clone();
        let first_retry =
            tokio::spawn(async move { reporting_pool.report(first, RequestOutcome::Quota).await });
        tokio::task::yield_now().await;
        clock.advance(QUOTA_COOLDOWN);
        let second = match first_retry.await.unwrap().unwrap() {
            ReportAction::Retry(lease) => lease,
            _ => panic!("quota should retry after cooldown"),
        };

        let reporting_pool = pool.clone();
        let second_retry =
            tokio::spawn(async move { reporting_pool.report(second, RequestOutcome::Quota).await });
        tokio::task::yield_now().await;
        clock.advance(Duration::from_secs(30));
        let error = second_retry.await.unwrap().unwrap_err();
        assert_eq!(error.code, Code::Quota);
    }

    #[tokio::test]
    async fn memo_budget_is_clamped_to_ninety_seconds() {
        let (pool, _provider, clock) = pool(vec![material("a", "A")]).await;
        let first = pool
            .acquire_with_budget(Priority::Memo, Duration::from_secs(180))
            .await
            .unwrap();
        let reporting_pool = pool.clone();
        let first_retry =
            tokio::spawn(async move { reporting_pool.report(first, RequestOutcome::Quota).await });
        tokio::task::yield_now().await;
        clock.advance(QUOTA_COOLDOWN);
        let second = match first_retry.await.unwrap().unwrap() {
            ReportAction::Retry(lease) => lease,
            _ => panic!("memo should retry inside its deadline"),
        };

        let reporting_pool = pool.clone();
        let second_retry =
            tokio::spawn(async move { reporting_pool.report(second, RequestOutcome::Quota).await });
        tokio::task::yield_now().await;
        clock.advance(Duration::from_secs(30));
        assert_eq!(second_retry.await.unwrap().unwrap_err().code, Code::Quota);
    }

    #[tokio::test]
    async fn a_request_never_receives_more_than_four_leases() {
        let (pool, _provider, _clock) = pool(vec![
            material("a", "A"),
            material("b", "B"),
            material("c", "C"),
            material("d", "D"),
        ])
        .await;
        let mut lease = pool.acquire(Priority::Job).await.unwrap();
        let mut attempts = 1;
        loop {
            match pool.report(lease, RequestOutcome::Quota).await {
                Ok(ReportAction::Retry(next)) => {
                    attempts += 1;
                    lease = next;
                }
                Err(error) => {
                    assert_eq!(error.code, Code::Quota);
                    break;
                }
                Ok(ReportAction::Complete) => panic!("quota must not complete"),
            }
        }
        assert_eq!(attempts, MAX_ATTEMPTS);
    }

    #[tokio::test]
    async fn stale_attempt_report_cannot_cancel_or_advance_a_retry() {
        let (pool, _provider, _clock) = pool(vec![material("a", "A"), material("b", "B")]).await;
        let first = pool.acquire(Priority::Job).await.unwrap();
        let stale = KeyLease {
            request_id: first.request_id.clone(),
            key_id: first.key_id.clone(),
            secret: first.secret.clone(),
            generation: first.generation,
            attempt: first.attempt,
            priority: first.priority,
        };
        let retry = match pool.report(first, RequestOutcome::Quota).await.unwrap() {
            ReportAction::Retry(lease) => lease,
            ReportAction::Complete => panic!("quota should retry"),
        };
        assert_eq!(
            pool.report(stale, RequestOutcome::Quota)
                .await
                .unwrap_err()
                .code,
            Code::Auth
        );
        assert_eq!(
            pool.report(retry, RequestOutcome::Success).await.unwrap(),
            ReportAction::Complete
        );
    }

    #[tokio::test]
    async fn refresh_preserves_auth_quarantine_until_that_key_changes() {
        let (pool, provider, _clock) = pool(vec![material("a", "A"), material("b", "B")]).await;
        let first = pool.acquire(Priority::Job).await.unwrap();
        assert_eq!(first.key_id, KeyId::from_opaque("a"));
        let second = match pool.report(first, RequestOutcome::Auth).await.unwrap() {
            ReportAction::Retry(lease) => lease,
            _ => panic!("auth should rotate to the next key"),
        };
        pool.report(second, RequestOutcome::Success).await.unwrap();

        pool.refresh().await.unwrap();
        let unchanged = pool.acquire(Priority::Job).await.unwrap();
        assert_eq!(unchanged.key_id, KeyId::from_opaque("b"));
        pool.report(unchanged, RequestOutcome::Success)
            .await
            .unwrap();

        provider.replace(vec![material("c", "CHANGED"), material("b", "B")]);
        pool.refresh().await.unwrap();
        let changed = pool.acquire(Priority::Job).await.unwrap();
        assert_eq!(changed.key_id, KeyId::from_opaque("c"));
    }

    #[tokio::test]
    async fn provider_failure_clears_stale_in_memory_keys() {
        let store = FakeCredentialStore::default();
        let service = Arc::new(SecretService::new(store.clone()));
        service.set("AIzaSySAFE1234").unwrap();
        let provider: Arc<dyn KeyProvider> = service;
        let clock = Arc::new(FakeClock::new());
        let (pool, actor) = KeyPoolHandle::channel(provider, clock);
        tokio::spawn(actor.run());
        pool.refresh().await.unwrap();
        let lease = pool.acquire(Priority::Job).await.unwrap();
        pool.report(lease, RequestOutcome::Success).await.unwrap();

        store.fail_next_read();
        assert_eq!(
            pool.refresh().await.unwrap_err().code,
            Code::Storage,
            "native-store failures must keep their storage category"
        );
        assert_eq!(
            pool.acquire(Priority::Job).await.unwrap_err().code,
            Code::Auth
        );
    }

    #[tokio::test]
    async fn ordinal_of_is_one_based_and_none_for_an_unknown_key() {
        let (pool, _provider, _clock) = pool(vec![material("a", "A"), material("b", "B")]).await;
        assert_eq!(pool.ordinal_of(&KeyId::from_opaque("a")).await, Some(1));
        assert_eq!(pool.ordinal_of(&KeyId::from_opaque("b")).await, Some(2));
        assert_eq!(pool.ordinal_of(&KeyId::from_opaque("z")).await, None);
    }

    #[test]
    fn pending_arbitration_is_live_then_job_then_memo() {
        let provider = Arc::new(FakeProvider::new(Vec::new()));
        let clock = Arc::new(FakeClock::new());
        let (_handle, mut actor) = KeyPoolHandle::channel(provider, clock.clone());
        let deadline = clock.now() + Duration::from_secs(30);

        for (id, priority) in [
            ("memo", Priority::Memo),
            ("job", Priority::Job),
            ("live", Priority::Live),
        ] {
            actor.requests.insert(
                id.to_string(),
                RequestState {
                    priority,
                    deadline,
                    attempts: 0,
                    target_key: None,
                },
            );
            let (reply, _response) = oneshot::channel();
            actor.enqueue(Pending {
                request_id: id.to_string(),
                reply: PendingReply::Acquire(reply),
            });
        }

        assert_eq!(actor.pop_pending(clock.now()).unwrap().request_id, "live");
        assert_eq!(actor.pop_pending(clock.now()).unwrap().request_id, "job");
        assert_eq!(actor.pop_pending(clock.now()).unwrap().request_id, "memo");
    }
}
