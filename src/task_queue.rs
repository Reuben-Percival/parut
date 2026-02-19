use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use std::thread;

#[derive(Debug, Clone, PartialEq)]
pub enum TaskType {
    Install,
    Remove,
    Update,
    UpdatePackage,
    CleanCache,
    RemoveOrphans,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TaskStatus {
    Queued,
    Running,
    Completed,
    Canceled,
    Failed(String),
}

#[derive(Debug, Clone)]
pub struct Task {
    pub id: usize,
    pub task_type: TaskType,
    pub package_name: String,
    pub status: TaskStatus,
    pub output: Vec<String>,
    pub progress: Option<f64>, // 0.0 to 1.0
    pub phase: Option<String>,
    pub started_at_unix: Option<u64>,
    pub finished_at_unix: Option<u64>,
}

impl Task {
    pub fn new(id: usize, task_type: TaskType, package_name: String) -> Self {
        Self {
            id,
            task_type,
            package_name,
            status: TaskStatus::Queued,
            output: Vec::new(),
            progress: None,
            phase: None,
            started_at_unix: None,
            finished_at_unix: None,
        }
    }
}

pub struct TaskQueue {
    tasks: Arc<Mutex<Vec<Task>>>,
    next_id: Arc<Mutex<usize>>,
    update_callback: Arc<Mutex<Option<Box<dyn Fn() + Send>>>>,
    cancel_requested: Arc<Mutex<HashSet<usize>>>,
}

impl TaskQueue {
    pub fn new() -> Self {
        Self {
            tasks: Arc::new(Mutex::new(Vec::new())),
            next_id: Arc::new(Mutex::new(0)),
            update_callback: Arc::new(Mutex::new(None)),
            cancel_requested: Arc::new(Mutex::new(HashSet::new())),
        }
    }

    #[allow(dead_code)]
    pub fn set_update_callback<F>(&self, callback: F)
    where
        F: Fn() + Send + 'static,
    {
        let mut cb = self.update_callback.lock().unwrap();
        *cb = Some(Box::new(callback));
    }

    pub fn add_task(&self, task_type: TaskType, package_name: String) -> usize {
        let mut next_id = self.next_id.lock().unwrap();
        let id = *next_id;
        *next_id += 1;

        let task = Task::new(id, task_type, package_name);

        let mut tasks = self.tasks.lock().unwrap();
        tasks.push(task);

        // Trigger UI update
        if let Some(callback) = self.update_callback.lock().unwrap().as_ref() {
            callback();
        }

        id
    }

    pub fn get_tasks(&self) -> Vec<Task> {
        self.tasks.lock().unwrap().clone()
    }

    pub fn update_task_status(&self, task_id: usize, status: TaskStatus) {
        let mut tasks = self.tasks.lock().unwrap();
        if let Some(task) = tasks.iter_mut().find(|t| t.id == task_id) {
            if status == TaskStatus::Running {
                task.started_at_unix = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .ok()
                    .map(|d| d.as_secs());
                task.phase = Some("Preparing".to_string());
                task.finished_at_unix = None;
            } else if matches!(
                status,
                TaskStatus::Completed | TaskStatus::Canceled | TaskStatus::Failed(_)
            ) {
                task.finished_at_unix = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .ok()
                    .map(|d| d.as_secs());
            }
            task.status = status;
        }

        // Trigger UI update
        if let Some(callback) = self.update_callback.lock().unwrap().as_ref() {
            callback();
        }
    }

    pub fn append_output(&self, task_id: usize, line: String) {
        let mut tasks = self.tasks.lock().unwrap();
        if let Some(task) = tasks.iter_mut().find(|t| t.id == task_id) {
            // Parse progress from common patterns
            let progress = Self::parse_progress(&line);
            if let Some(p) = progress {
                task.progress = Some(p);
            }
            if let Some(phase) = Self::parse_phase(&line) {
                task.phase = Some(phase);
            }

            task.output.push(line);
            let limit = crate::settings::get().task_output_lines_limit.max(50);
            if task.output.len() > limit {
                let drain = task.output.len() - limit;
                task.output.drain(0..drain);
            }
        }

        // Trigger UI update
        if let Some(callback) = self.update_callback.lock().unwrap().as_ref() {
            callback();
        }
    }

    fn parse_progress(line: &str) -> Option<f64> {
        // Parse download progress: "downloading... 45%"
        if let Some(pct_pos) = line.find('%') {
            let before = &line[..pct_pos];
            if let Some(num_start) = before.rfind(|c: char| !c.is_ascii_digit() && c != '.') {
                if let Ok(pct) = before[num_start + 1..].parse::<f64>() {
                    return Some(pct / 100.0);
                }
            }
        }

        // Parse makepkg progress: "(1/4) checking keys..."
        if line.contains("(") && line.contains("/") && line.contains(")") {
            if let Some(start) = line.find('(') {
                if let Some(end) = line.find(')') {
                    let nums = &line[start + 1..end];
                    if let Some(slash) = nums.find('/') {
                        if let (Ok(current), Ok(total)) = (
                            nums[..slash].parse::<f64>(),
                            nums[slash + 1..].parse::<f64>(),
                        ) {
                            return Some(current / total);
                        }
                    }
                }
            }
        }

        None
    }

    fn parse_phase(line: &str) -> Option<String> {
        let l = line.to_lowercase();
        if l.contains("resolving dependencies") {
            Some("Resolving dependencies".to_string())
        } else if l.contains("checking keys") {
            Some("Checking keys".to_string())
        } else if l.contains("checking package integrity") {
            Some("Verifying package integrity".to_string())
        } else if l.contains("loading package files") {
            Some("Loading package files".to_string())
        } else if l.contains("checking for file conflicts") {
            Some("Checking file conflicts".to_string())
        } else if l.contains("downloading") || l.contains("retrieving") {
            Some("Downloading".to_string())
        } else if l.contains("building") || l.contains("makepkg") {
            Some("Building".to_string())
        } else if l.contains("installing") || l.contains("upgrading") {
            Some("Installing".to_string())
        } else if l.contains("removing") {
            Some("Removing".to_string())
        } else {
            None
        }
    }

    pub fn clear_completed(&self) {
        let mut tasks = self.tasks.lock().unwrap();
        tasks.retain(|t| {
            !matches!(
                t.status,
                TaskStatus::Completed | TaskStatus::Canceled | TaskStatus::Failed(_)
            )
        });

        // Trigger UI update
        if let Some(callback) = self.update_callback.lock().unwrap().as_ref() {
            callback();
        }
    }

    pub fn claim_next_queued_task(&self) -> Option<Task> {
        let mut tasks = self.tasks.lock().unwrap();
        let task = tasks
            .iter_mut()
            .find(|t| t.status == TaskStatus::Queued)
            .map(|task| {
                task.started_at_unix = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .ok()
                    .map(|d| d.as_secs());
                task.phase = Some("Preparing".to_string());
                task.finished_at_unix = None;
                task.status = TaskStatus::Running;
                task.clone()
            });
        drop(tasks);
        if task.is_some() {
            self.notify_update();
        }
        task
    }

    #[allow(dead_code)]
    pub fn has_running_task(&self) -> bool {
        self.running_count() > 0
    }

    pub fn running_count(&self) -> usize {
        let tasks = self.tasks.lock().unwrap();
        tasks
            .iter()
            .filter(|t| t.status == TaskStatus::Running)
            .count()
    }

    pub fn cancel_queued_task(&self, task_id: usize) -> bool {
        let mut tasks = self.tasks.lock().unwrap();
        let before = tasks.len();
        tasks.retain(|t| !(t.id == task_id && t.status == TaskStatus::Queued));
        let changed = tasks.len() != before;

        if changed {
            if let Some(callback) = self.update_callback.lock().unwrap().as_ref() {
                callback();
            }
        }

        changed
    }

    pub fn move_queued_task_up(&self, task_id: usize) -> bool {
        let mut tasks = self.tasks.lock().unwrap();
        let Some(idx) = tasks
            .iter()
            .position(|t| t.id == task_id && t.status == TaskStatus::Queued)
        else {
            return false;
        };
        let Some(prev_idx) = (0..idx)
            .rev()
            .find(|i| tasks[*i].status == TaskStatus::Queued)
        else {
            return false;
        };
        tasks.swap(idx, prev_idx);
        drop(tasks);
        self.notify_update();
        true
    }

    pub fn move_queued_task_down(&self, task_id: usize) -> bool {
        let mut tasks = self.tasks.lock().unwrap();
        let Some(idx) = tasks
            .iter()
            .position(|t| t.id == task_id && t.status == TaskStatus::Queued)
        else {
            return false;
        };
        let Some(next_idx) =
            ((idx + 1)..tasks.len()).find(|i| tasks[*i].status == TaskStatus::Queued)
        else {
            return false;
        };
        tasks.swap(idx, next_idx);
        drop(tasks);
        self.notify_update();
        true
    }

    pub fn run_queued_task_now(&self, task_id: usize) -> bool {
        let mut tasks = self.tasks.lock().unwrap();
        let Some(idx) = tasks
            .iter()
            .position(|t| t.id == task_id && t.status == TaskStatus::Queued)
        else {
            return false;
        };

        let Some(first_queued_idx) = tasks.iter().position(|t| t.status == TaskStatus::Queued)
        else {
            return false;
        };
        if idx == first_queued_idx {
            return false;
        }

        let task = tasks.remove(idx);
        tasks.insert(first_queued_idx, task);
        drop(tasks);
        self.notify_update();
        true
    }

    pub fn request_cancel(&self, task_id: usize) -> bool {
        {
            let tasks = self.tasks.lock().unwrap();
            if !tasks
                .iter()
                .any(|t| t.id == task_id && t.status == TaskStatus::Running)
            {
                return false;
            }
        }

        self.cancel_requested.lock().unwrap().insert(task_id);
        self.append_output(task_id, "Cancellation requested...".to_string());
        true
    }

    pub fn is_cancel_requested(&self, task_id: usize) -> bool {
        self.cancel_requested.lock().unwrap().contains(&task_id)
    }

    pub fn take_cancel_request(&self, task_id: usize) -> bool {
        self.cancel_requested.lock().unwrap().remove(&task_id)
    }

    fn notify_update(&self) {
        if let Some(callback) = self.update_callback.lock().unwrap().as_ref() {
            callback();
        }
    }

    pub fn auto_clear_by_settings(&self) {
        let minutes = crate::settings::get().auto_clear_completed_tasks_minutes;
        if minutes == 0 {
            return;
        }
        let cutoff = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .ok()
            .map(|d| d.as_secs())
            .unwrap_or(0)
            .saturating_sub(minutes.saturating_mul(60));

        let mut tasks = self.tasks.lock().unwrap();
        let before = tasks.len();
        tasks.retain(|t| {
            if !matches!(
                t.status,
                TaskStatus::Completed | TaskStatus::Canceled | TaskStatus::Failed(_)
            ) {
                return true;
            }
            t.finished_at_unix
                .map(|done| done >= cutoff)
                .unwrap_or(true)
        });
        let changed = before != tasks.len();
        drop(tasks);
        if changed {
            self.notify_update();
        }
    }

    pub fn retry_failed_task(&self, task_id: usize) -> Option<usize> {
        let (task_type, package_name) = {
            let tasks = self.tasks.lock().unwrap();
            let failed = tasks.iter().find(|t| t.id == task_id).and_then(|t| {
                if matches!(t.status, TaskStatus::Failed(_)) {
                    Some((t.task_type.clone(), t.package_name.clone()))
                } else {
                    None
                }
            })?;
            failed
        };

        Some(self.add_task(task_type, package_name))
    }
}

pub struct TaskWorker {
    queue: Arc<TaskQueue>,
}

impl TaskWorker {
    pub fn new(queue: Arc<TaskQueue>) -> Self {
        Self { queue }
    }

    pub fn start(&self) {
        let queue = self.queue.clone();

        thread::spawn(move || {
            loop {
                queue.auto_clear_by_settings();

                let max_parallel = crate::settings::get().max_parallel_tasks.max(1);
                if queue.running_count() >= max_parallel {
                    thread::sleep(std::time::Duration::from_millis(500));
                    continue;
                }

                // Atomically claim and mark one queued task as running.
                // This prevents duplicate dispatch of the same task when
                // the scheduler loop spins quickly.
                if let Some(task) = queue.claim_next_queued_task() {
                    let queue_for_task = queue.clone();
                    thread::spawn(move || {
                        let result = Self::execute_task(&queue_for_task, &task);
                        match result {
                            Ok(_) => {
                                queue_for_task.update_task_status(task.id, TaskStatus::Completed);
                            }
                            Err(e) => {
                                if queue_for_task.take_cancel_request(task.id) {
                                    queue_for_task
                                        .update_task_status(task.id, TaskStatus::Canceled);
                                } else {
                                    queue_for_task
                                        .update_task_status(task.id, TaskStatus::Failed(e));
                                }
                            }
                        }
                    });
                } else {
                    // No tasks, sleep a bit
                    thread::sleep(std::time::Duration::from_secs(1));
                }
            }
        });
    }

    fn execute_task(queue: &Arc<TaskQueue>, task: &Task) -> Result<(), String> {
        use crate::paru::ParuBackend;
        use crate::settings;
        use crate::utils;

        let task_id = task.id;
        let queue_clone = queue.clone();
        let queue_for_cancel = queue.clone();

        let output_callback = move |line: String| {
            queue_clone.append_output(task_id, line);
        };
        let cancel_requested: std::sync::Arc<dyn Fn() -> bool + Send + Sync> =
            std::sync::Arc::new(move || queue_for_cancel.is_cancel_requested(task_id));

        match task.task_type {
            TaskType::Install => ParuBackend::install_package(
                &task.package_name,
                output_callback,
                cancel_requested.clone(),
            ),
            TaskType::Remove => ParuBackend::remove_package(
                &task.package_name,
                output_callback,
                cancel_requested.clone(),
            ),
            TaskType::Update => {
                ParuBackend::update_system(output_callback, cancel_requested.clone())
            }
            TaskType::UpdatePackage => ParuBackend::update_package(
                &task.package_name,
                output_callback,
                cancel_requested.clone(),
            ),
            TaskType::CleanCache => {
                ParuBackend::clean_cache(output_callback, cancel_requested.clone())
            }
            TaskType::RemoveOrphans => {
                ParuBackend::remove_orphans(output_callback, cancel_requested.clone())
            }
        }
        .inspect(|_| {
            if settings::get().notify_on_task_complete {
                utils::send_notification(
                    "Parut Task Completed",
                    &format!("{:?} {}", task.task_type, task.package_name),
                );
            }
        })
        .inspect_err(|err| {
            if settings::get().notify_on_task_failed {
                utils::send_notification(
                    "Parut Task Failed",
                    &format!("{:?} {}: {}", task.task_type, task.package_name, err),
                );
            }
        })
    }
}
