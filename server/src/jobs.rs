use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::sync::Mutex;
use uuid::Uuid;

#[derive(Serialize, Clone, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum JobStatus {
    Queued,
    Running,
    Succeeded,
    Failed,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct JobInfo {
    pub job_id: String,
    pub status: JobStatus,
    pub progress: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Default)]
pub struct Jobs {
    inner: Mutex<HashMap<String, JobInfo>>,
    cancel: Mutex<HashSet<String>>,
}

impl Jobs {
    pub fn create(&self) -> String {
        let id = Uuid::new_v4().to_string();
        let job = JobInfo {
            job_id: id.clone(),
            status: JobStatus::Queued,
            progress: 0,
            message: None,
            result: None,
            error: None,
        };
        self.inner.lock().unwrap().insert(id.clone(), job);
        id
    }

    pub fn set_running(&self, id: &str) {
        if let Some(job) = self.inner.lock().unwrap().get_mut(id) {
            job.status = JobStatus::Running;
        }
    }

    pub fn set_progress(&self, id: &str, progress: u8, message: Option<String>) {
        if let Some(job) = self.inner.lock().unwrap().get_mut(id) {
            job.progress = progress.min(100);
            job.message = message;
        }
    }

    pub fn complete(&self, id: &str, result: serde_json::Value) {
        {
            let mut inner = self.inner.lock().unwrap();
            if let Some(job) = inner.get_mut(id) {
                job.status = JobStatus::Succeeded;
                job.progress = 100;
                job.result = Some(result);
            }
        }
        self.cancel.lock().unwrap().remove(id);
    }

    pub fn fail(&self, id: &str, error: String) {
        {
            let mut inner = self.inner.lock().unwrap();
            if let Some(job) = inner.get_mut(id) {
                job.status = JobStatus::Failed;
                job.error = Some(error);
            }
        }
        self.cancel.lock().unwrap().remove(id);
    }

    pub fn mark_cancelled(&self, id: &str) {
        self.cancel.lock().unwrap().insert(id.to_string());
    }

    pub fn is_cancelled(&self, id: &str) -> bool {
        self.cancel.lock().unwrap().contains(id)
    }

    pub fn get(&self, id: &str) -> Option<JobInfo> {
        self.inner.lock().unwrap().get(id).cloned()
    }
}
