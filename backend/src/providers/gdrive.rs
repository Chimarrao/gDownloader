use anyhow::Result;
use crate::models::FileInfo;
use super::{Provider, ProgressUpdate};

pub struct GDriveProvider;

impl GDriveProvider {
    pub fn matches(url: &str) -> bool {
        url.contains("drive.google.com")
    }
}

impl Provider for GDriveProvider {
    fn name(&self) -> &str { "Google Drive" }

    fn get_file_info<'a>(&'a self, _url: &'a str)
        -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<FileInfo>> + Send + 'a>>
    {
        // todo!() é como throw new Error("not implemented") no JS
        // Causa um panic em runtime se chamado — usado como placeholder
        Box::pin(async move { todo!("GDrive get_file_info — será implementado na Task 7") })
    }

    fn download<'a>(&'a self, _url: &'a str, _dest_path: &'a str, _tx: tokio::sync::mpsc::Sender<ProgressUpdate>)
        -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<u64>> + Send + 'a>>
    {
        Box::pin(async move { todo!("GDrive download — será implementado na Task 7") })
    }
}
