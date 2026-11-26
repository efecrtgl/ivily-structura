//! Application state shared by Tauri commands.

use crate::analysis::model::Analysis;
use crate::core::CancelToken;
use crate::db::Db;
use crate::error::{AppError, AppResult};
use parking_lot::Mutex;
use std::collections::VecDeque;
use std::sync::Arc;

const MAX_CACHED_ANALYSES: usize = 6;

pub struct Job {
    pub id: String,
    pub project_id: i64,
    pub cancel: CancelToken,
}

pub struct AppState {
    pub db: Db,
    /// Degraded mode: the on-disk database could not be opened and an in-memory one is used.
    pub degraded: Option<String>,
    analyses: Mutex<VecDeque<(String, i64, Arc<Analysis>)>>,
    pub jobs: Mutex<Vec<Job>>,
    /// Folder passed on the command line; consumed once by the UI.
    pub startup_path: Mutex<Option<String>>,
}

impl AppState {
    pub fn init() -> Self {
        match Db::open(&crate::core::db_path()) {
            Ok(db) => {
                if let Ok(s) = crate::store::settings::get(&db) {
                    let _ = crate::store::history::prune(&db, s.history_retention_days);
                }
                Self::with_db(db, None)
            }
            Err(e) => {
                let db = Db::open_in_memory().expect("in-memory database");
                Self::with_db(db, Some(e.to_string()))
            }
        }
    }

    pub fn with_db(db: Db, degraded: Option<String>) -> Self {
        Self {
            db,
            degraded,
            analyses: Mutex::new(VecDeque::new()),
            jobs: Mutex::new(Vec::new()),
            startup_path: Mutex::new(crate::startup_path(std::env::args())),
        }
    }

    /// Keep an analysis in memory; returns its id.
    pub fn put_analysis(&self, project_id: i64, a: Analysis) -> String {
        let id = a.meta.id.clone();
        let mut q = self.analyses.lock();
        q.retain(|(x, _, _)| *x != id);
        q.push_front((id.clone(), project_id, Arc::new(a)));
        while q.len() > MAX_CACHED_ANALYSES {
            q.pop_back();
        }
        id
    }

    pub fn analysis(&self, id: &str) -> AppResult<(i64, Arc<Analysis>)> {
        let q = self.analyses.lock();
        q.iter().find(|(x, _, _)| x == id).map(|(_, p, a)| (*p, a.clone())).ok_or_else(|| {
            AppError::NotFound("analysis is no longer in memory — reopen the project or snapshot".into())
        })
    }

    pub fn drop_project(&self, project_id: i64) {
        self.analyses.lock().retain(|(_, p, _)| *p != project_id);
    }
}
