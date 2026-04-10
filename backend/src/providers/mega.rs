use anyhow::Result;
use crate::models::FileInfo;
use super::{Provider, ProgressUpdate};

pub struct MegaProvider;

impl MegaProvider {
    pub fn matches(url: &str) -> bool {
        url.contains("mega.nz") || url.contains("mega.co.nz")
    }
}

impl Provider for MegaProvider {
    fn name(&self) -> &str { "Mega" }

    fn get_file_info<'a>(&'a self, _url: &'a str)
        -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<FileInfo>> + Send + 'a>>
    {
        Box::pin(async move { todo!("Mega get_file_info — será implementado na Task 9") })
    }

    fn download<'a>(&'a self, _url: &'a str, _dest_path: &'a str, _tx: tokio::sync::mpsc::Sender<ProgressUpdate>)
        -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<u64>> + Send + 'a>>
    {
        Box::pin(async move { todo!("Mega download — será implementado na Task 9") })
    }
}
