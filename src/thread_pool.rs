use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use ffmpeg_next::log::Level;

use tracing::{debug, error};

use crate::task::Task;

pub struct ThreadPool {
    workers: Vec<Worker>,
    sender: Sender<Task>,
}

impl ThreadPool {
    pub(crate) fn new(num_threads: Option<usize>) -> Self {
        let num_threads = num_threads.unwrap_or_else(num_cpus::get);
        let (sender, receiver) = mpsc::channel();
        let receiver = Arc::new(Mutex::new(receiver));

        let workers = (0..num_threads)
            .map(|id| Worker::new(id, Arc::clone(&receiver)))
            .collect();

        ThreadPool { workers, sender }
    }

    pub fn enqueue(&self, task: Task) {
        if let Err(e) = self.sender.send(task) {
            error!("failed to send task to the queue: {:?}", e);
        }
    }
}

struct Worker {
    id: usize,
    thread: Option<thread::JoinHandle<()>>,
}

impl Worker {
    fn new(id: usize, receiver: Arc<Mutex<Receiver<Task>>>) -> Self {
        let thread = thread::spawn(move || {
            ffmpeg_next::init()
                .unwrap_or_else(|err| tracing::error!("couldn't init FFmpeg: {:?}", err));
            ffmpeg_next::util::log::set_level(Level::Quiet);

            loop {
                let task = {
                    let lock = receiver.lock().unwrap();
                    lock.recv()
                };

                match task {
                    Ok(task) => {
                        debug!("worker {} got a task; executing.", id);
                        if let Err(err) = task.execute() {
                            error!("worker {} failed to finish the task: {:?}", id, err);
                        }
                    }
                    Err(e) => {
                        error!("worker {} failed to receive task: {:?}", id, e);
                        thread::sleep(Duration::from_secs(1)); // sleep to avoid busy-looping
                    }
                }
            }
        });

        Worker {
            id,
            thread: Some(thread),
        }
    }
}
