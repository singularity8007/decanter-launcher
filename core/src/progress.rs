//! A shared "what's happening right now" record.
//!
//! Installers write to it while they work; the UI reads snapshots of it. It
//! carries two bars (download and install), raw byte counters the speed
//! sampler turns into network/disk speeds, and the pause flag every
//! downloader checks.

use serde::Serialize;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering};
use std::time::Instant;

/// What a job is doing, shown as a word next to the bars.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Default)]
#[serde(rename_all = "snake_case")]
#[repr(u8)]
pub enum Phase {
    #[default]
    Preparing = 0,
    Verifying = 1,
    Downloading = 2,
    Installing = 3,
    Running = 4,
}

impl Phase {
    fn from_u8(v: u8) -> Self {
        match v {
            1 => Phase::Verifying,
            2 => Phase::Downloading,
            3 => Phase::Installing,
            4 => Phase::Running,
            _ => Phase::Preparing,
        }
    }
}

#[derive(Default)]
pub struct Progress {
    done: AtomicU64,
    total: AtomicU64,
    install_done: AtomicU64,
    install_total: AtomicU64,
    /// Every byte received from the network, ever (for the speed sampler).
    net_bytes: AtomicU64,
    /// Every byte written to disk, ever (for the speed sampler).
    disk_bytes: AtomicU64,
    /// Speeds reported directly by fischl, which keeps its own counters.
    reported: Mutex<Option<(Instant, u64, u64)>>,
    phase: AtomicU8,
    label: Mutex<String>,
    /// Warnings that aren't failures, e.g. "offline, using the installed copy".
    notes: Mutex<Vec<String>>,
    cancel: Arc<AtomicBool>,
}

/// A copy of the numbers at one moment.
#[derive(Debug, Clone, Serialize)]
pub struct Snapshot {
    pub label: String,
    pub phase: Phase,
    pub done: u64,
    pub total: u64,
    pub install_done: u64,
    pub install_total: u64,
}

impl Progress {
    /// Clear everything. Called at the start of each job.
    pub fn reset(&self) {
        for counter in [&self.done, &self.total, &self.install_done, &self.install_total] {
            counter.store(0, Ordering::Relaxed);
        }
        *self.reported.lock().unwrap() = None;
        self.phase.store(Phase::Preparing as u8, Ordering::Relaxed);
        self.label.lock().unwrap().clear();
        self.notes.lock().unwrap().clear();
        self.cancel.store(false, Ordering::Relaxed);
    }

    pub fn set_label(&self, text: impl Into<String>) {
        *self.label.lock().unwrap() = text.into();
    }

    pub fn set_phase(&self, phase: Phase) {
        self.phase.store(phase as u8, Ordering::Relaxed);
    }

    /// Size of the download bar. 0 = unknown (the UI shows an indeterminate bar).
    pub fn set_total(&self, bytes: u64) {
        self.total.store(bytes, Ordering::Relaxed);
    }

    /// Move the download bar forward.
    pub fn add(&self, bytes: u64) {
        self.done.fetch_add(bytes, Ordering::Relaxed);
    }

    /// Set both bars at once (fischl reports absolute numbers).
    pub fn set_bars(&self, done: u64, total: u64, install_done: u64, install_total: u64) {
        self.done.store(done, Ordering::Relaxed);
        self.total.store(total, Ordering::Relaxed);
        self.install_done.store(install_done, Ordering::Relaxed);
        self.install_total.store(install_total, Ordering::Relaxed);
    }

    pub fn set_install(&self, done: u64, total: u64) {
        self.install_done.store(done, Ordering::Relaxed);
        self.install_total.store(total, Ordering::Relaxed);
    }

    /// Bytes that came off the network (compressed size).
    pub fn add_net(&self, bytes: u64) {
        self.net_bytes.fetch_add(bytes, Ordering::Relaxed);
    }

    /// Bytes written to disk.
    pub fn add_disk(&self, bytes: u64) {
        self.disk_bytes.fetch_add(bytes, Ordering::Relaxed);
    }

    /// Speeds measured by a downloader that keeps its own counters (bytes/s).
    pub fn report_speeds(&self, net: u64, disk: u64) {
        *self.reported.lock().unwrap() = Some((Instant::now(), net, disk));
    }

    /// (network bytes ever, disk bytes ever, speeds reported in the last 2 s)
    pub fn counters(&self) -> (u64, u64, Option<(u64, u64)>) {
        let reported = self
            .reported
            .lock()
            .unwrap()
            .filter(|(at, _, _)| at.elapsed().as_secs() < 2)
            .map(|(_, n, d)| (n, d));
        (
            self.net_bytes.load(Ordering::Relaxed),
            self.disk_bytes.load(Ordering::Relaxed),
            reported,
        )
    }

    pub fn note(&self, text: impl Into<String>) {
        self.notes.lock().unwrap().push(text.into());
    }

    pub fn take_notes(&self) -> Vec<String> {
        std::mem::take(&mut *self.notes.lock().unwrap())
    }

    /// Ask the running job to stop at the next safe point (pause).
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }

    pub fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }

    /// The flag itself, for fischl's `cancel_token`.
    pub fn cancel_token(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.cancel)
    }

    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            label: self.label.lock().unwrap().clone(),
            phase: Phase::from_u8(self.phase.load(Ordering::Relaxed)),
            done: self.done.load(Ordering::Relaxed),
            total: self.total.load(Ordering::Relaxed),
            install_done: self.install_done.load(Ordering::Relaxed),
            install_total: self.install_total.load(Ordering::Relaxed),
        }
    }
}
