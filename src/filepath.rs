use std::path::{Path, PathBuf};

pub const EXT: &str = "atranscoder";

pub fn in_file_path(work_dir: &str, task_id: String) -> PathBuf {
    Path::new(work_dir).join(format!("{}.in.atranscoder", task_id))
}

pub fn out_file_path(work_dir: &str, task_id: String) -> PathBuf {
    Path::new(work_dir).join(format!("{}.out.atranscoder", task_id))
}
