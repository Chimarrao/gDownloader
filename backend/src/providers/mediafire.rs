use anyhow::Result;
use crate::models::FileInfo;
use super::{Provider, ProgressUpdate};

pub struct MediaFireProvider;

impl MediaFireProvider {
    pub fn matches(url: &str) -> bool {
        url.contains("mediafire.com")
    }
}

impl Provider for MediaFireProvider {
    fn name(&self) -> &str { "MediaFire" }

    fn get_file_info<'a>(&'a self, _url: &'a str)
        -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<FileInfo>> + Send + 'a>>
    {
        Box::pin(async move { todo!("MediaFire get_file_info — será implementado na Task 8") })
    }

    fn download<'a>(&'a self, _url: &'a str, _dest_path: &'a str, _tx: tokio::sync::mpsc::Sender<ProgressUpdate>)
        -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<u64>> + Send + 'a>>
    {
        Box::pin(async move { todo!("MediaFire download — será implementado na Task 8") })
    }
}
