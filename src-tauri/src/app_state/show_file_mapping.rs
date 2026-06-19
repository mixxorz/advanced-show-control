use std::path::PathBuf;

use super::shell::ShellState;

impl ShellState {
    pub async fn current_show_file_path(&self) -> Option<PathBuf> {
        let inner = self.inner.lock().await;
        inner.show_file_path.clone()
    }

    #[cfg(test)]
    pub async fn export_show_file(&self, saved_at: String) -> crate::show::show_file::ShowFile {
        crate::show::show_file::export_show_file(self.show.get_snapshot().await, saved_at)
    }
}
