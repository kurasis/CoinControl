//! Bounded process-local jobs for reconnectable progress without long-running IPC replies.
use serde::Serialize;
use std::{
    collections::VecDeque,
    sync::{
        Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct SyncJob {
    pub id: String,
    pub status: String,
    pub error_code: Option<String>,
    pub created_at: i64,
    pub finished_at: Option<i64>,
}
#[derive(Default)]
pub struct Jobs {
    next: AtomicU64,
    records: Mutex<VecDeque<SyncJob>>,
}
impl Jobs {
    pub fn create(&self, now: i64) -> Option<SyncJob> {
        let mut records = self.records.lock().expect("jobs");
        if records.len() >= 32 {
            let finished = records
                .iter()
                .position(|j| !matches!(j.status.as_str(), "queued" | "running" | "cancelling"))?;
            records.remove(finished);
        }
        let id = format!(
            "{now}-{}-{}",
            std::process::id(),
            self.next.fetch_add(1, Ordering::Relaxed)
        );
        let job = SyncJob {
            id,
            status: "queued".into(),
            error_code: None,
            created_at: now,
            finished_at: None,
        };
        records.push_back(job.clone());
        Some(job)
    }
    pub fn start(&self, id: &str) -> bool {
        let mut records = self.records.lock().expect("jobs");
        if let Some(job) = records
            .iter_mut()
            .find(|j| j.id == id && j.status == "queued")
        {
            job.status = "running".into();
            true
        } else {
            false
        }
    }
    pub fn cancel(&self, id: &str, cancelled: &AtomicBool, now: i64) -> bool {
        let mut records = self.records.lock().expect("jobs");
        let Some(job) = records.iter_mut().find(|j| j.id == id) else {
            return false;
        };
        if job.status == "queued" {
            job.status = "cancelled".into();
            job.finished_at = Some(now);
        } else if job.status == "running" {
            job.status = "cancelling".into();
            cancelled.store(true, Ordering::Relaxed);
        }
        true
    }
    pub fn get(&self, id: &str) -> Option<SyncJob> {
        self.records
            .lock()
            .expect("jobs")
            .iter()
            .find(|j| j.id == id)
            .cloned()
    }
    pub fn update(&self, id: &str, status: &str, error: Option<&str>, now: i64) {
        if let Some(job) = self
            .records
            .lock()
            .expect("jobs")
            .iter_mut()
            .find(|j| j.id == id)
        {
            job.status = status.into();
            job.error_code = error.map(str::to_owned);
            if !matches!(status, "queued" | "running" | "cancelling") {
                job.finished_at = Some(now);
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn active_jobs_are_retained_and_completed_history_is_bounded() {
        let jobs = Jobs::default();
        let first = jobs.create(1).unwrap();
        for _ in 1..32 {
            jobs.create(1).unwrap();
        }
        assert!(jobs.create(1).is_none());
        jobs.update(&first.id, "cancelled", None, 2);
        let next = jobs.create(2).unwrap();
        assert_ne!(first.id, next.id);
        assert!(jobs.get(&first.id).is_none());
        assert_eq!(jobs.get(&next.id).unwrap().status, "queued");
        jobs.update(&next.id, "failed", Some("invalid_input"), 3);
        assert_eq!(
            jobs.get(&next.id).unwrap().error_code.as_deref(),
            Some("invalid_input")
        );
    }
}

#[cfg(test)]
mod cancellation_tests {
    use super::*;
    #[test]
    fn queued_cancel_cannot_be_overwritten_by_start_and_finished_jobs_do_not_cancel_other_work() {
        let jobs = Jobs::default();
        let cancelled = AtomicBool::new(false);
        let queued = jobs.create(1).unwrap();
        assert!(jobs.cancel(&queued.id, &cancelled, 2));
        assert!(!jobs.start(&queued.id));
        assert!(!cancelled.load(Ordering::Relaxed));
        let running = jobs.create(3).unwrap();
        assert!(jobs.start(&running.id));
        assert!(jobs.cancel(&running.id, &cancelled, 4));
        assert!(cancelled.load(Ordering::Relaxed));
        jobs.update(&running.id, "cancelled", None, 5);
        cancelled.store(false, Ordering::Relaxed);
        assert!(jobs.cancel(&running.id, &cancelled, 6));
        assert!(!cancelled.load(Ordering::Relaxed));
    }
}
