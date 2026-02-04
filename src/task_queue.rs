use std::sync::{Arc, Mutex};
use std::sync::mpsc::{channel, Sender, Receiver};
use std::thread;

#[derive(Debug, Clone, PartialEq)]
pub enum TaskType {
    Install,
    Remove,
    Update,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TaskStatus {
    Queued,
    Running,
    Completed,
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
        }
    }
}

#[allow(dead_code)]
pub enum QueueMessage {
    AddTask(TaskType, String),
    UpdateTaskStatus(usize, TaskStatus),
    AppendOutput(usize, String),
    GetTasks,
    ClearCompleted,
}

pub struct TaskQueue {
    tasks: Arc<Mutex<Vec<Task>>>,
    next_id: Arc<Mutex<usize>>,
    #[allow(dead_code)]
    tx: Sender<QueueMessage>,
    #[allow(dead_code)]
    rx: Arc<Mutex<Receiver<QueueMessage>>>,
    update_callback: Arc<Mutex<Option<Box<dyn Fn() + Send>>>>,
}

impl TaskQueue {
    pub fn new() -> Self {
        let (tx, rx) = channel();
        
        Self {
            tasks: Arc::new(Mutex::new(Vec::new())),
            next_id: Arc::new(Mutex::new(0)),
            tx,
            rx: Arc::new(Mutex::new(rx)),
            update_callback: Arc::new(Mutex::new(None)),
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
            
            task.output.push(line);
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
                            nums[slash + 1..].parse::<f64>()
                        ) {
                            return Some(current / total);
                        }
                    }
                }
            }
        }
        
        None
    }

    pub fn clear_completed(&self) {
        let mut tasks = self.tasks.lock().unwrap();
        tasks.retain(|t| !matches!(t.status, TaskStatus::Completed | TaskStatus::Failed(_)));
        
        // Trigger UI update
        if let Some(callback) = self.update_callback.lock().unwrap().as_ref() {
            callback();
        }
    }

    pub fn get_next_queued_task(&self) -> Option<Task> {
        let tasks = self.tasks.lock().unwrap();
        tasks.iter()
            .find(|t| t.status == TaskStatus::Queued)
            .cloned()
    }

    pub fn has_running_task(&self) -> bool {
        let tasks = self.tasks.lock().unwrap();
        tasks.iter().any(|t| t.status == TaskStatus::Running)
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
                // Check if there's already a running task
                if queue.has_running_task() {
                    thread::sleep(std::time::Duration::from_millis(500));
                    continue;
                }

                // Get next queued task
                if let Some(task) = queue.get_next_queued_task() {
                    // Mark as running
                    queue.update_task_status(task.id, TaskStatus::Running);
                    
                    // Execute the task
                    let result = Self::execute_task(&queue, &task);
                    
                    // Update status based on result
                    match result {
                        Ok(_) => {
                            queue.update_task_status(task.id, TaskStatus::Completed);
                        }
                        Err(e) => {
                            queue.update_task_status(task.id, TaskStatus::Failed(e));
                        }
                    }
                } else {
                    // No tasks, sleep a bit
                    thread::sleep(std::time::Duration::from_secs(1));
                }
            }
        });
    }

    fn execute_task(queue: &Arc<TaskQueue>, task: &Task) -> Result<(), String> {
        use crate::paru::ParuBackend;
        
        let task_id = task.id;
        let queue_clone = queue.clone();
        
        let output_callback = move |line: String| {
            queue_clone.append_output(task_id, line);
        };

        match task.task_type {
            TaskType::Install => {
                ParuBackend::install_package(&task.package_name, output_callback)
            }
            TaskType::Remove => {
                ParuBackend::remove_package(&task.package_name, output_callback)
            }
            TaskType::Update => {
                ParuBackend::update_system(output_callback)
            }
        }
    }
}
