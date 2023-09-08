use crate::ProgressEvent;
use anyhow::{Context, Result};
use log::{debug, info};
use nwg::NoticeSender;
use progress_streams::ProgressReader;
use serde::Deserialize;
use std::io::BufReader;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc::Sender;
use std::time::Duration;
use xz2::read::XzDecoder;

#[derive(Deserialize, Debug)]
pub struct Release {
    assets: Vec<ReleaseAsset>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct ReleaseAsset {
    pub name: String,
    pub browser_download_url: String,
    pub size: u32,
}

impl ReleaseAsset {
    pub fn display_name(&self) -> String {
        self.name
            .strip_suffix(".tar.xz")
            .unwrap_or(&self.name)
            .to_string()
    }

    pub fn is_supported(&self) -> bool {
        self.name.ends_with(".tar.xz")
    }

    pub fn download(
        &self,
        sender: &Sender<ProgressEvent>,
        notice_sender: &NoticeSender,
    ) -> Result<Vec<u8>> {
        sender
            .send(ProgressEvent::Downloading {
                name: self.display_name(),
                done: 0,
                total: self.size,
            })
            .unwrap();
        notice_sender.notice();
        let res = ureq::get(&self.browser_download_url).call()?;

        let atomic_done = AtomicU32::new(0);
        let bufreader = BufReader::with_capacity(1024 * 1024, res.into_reader());
        let mut reader = ProgressReader::new(bufreader, |progress| {
            let done = atomic_done.fetch_add(progress as u32, Ordering::Relaxed) + progress as u32;
            sender
                .send(ProgressEvent::Downloading {
                    name: self.display_name(),
                    done,
                    total: self.size,
                })
                .unwrap();
            notice_sender.notice();
        });

        let mut out_buf = vec![];
        std::io::copy(&mut reader, &mut out_buf)?;

        Ok(out_buf)
    }

    pub fn install(
        &self,
        sender: &Sender<ProgressEvent>,
        notice_sender: &NoticeSender,
    ) -> Result<()> {
        let zip = self.download(sender, notice_sender)?;

        sender
            .send(ProgressEvent::Installing(self.display_name()))
            .unwrap();
        notice_sender.notice();

        let fonts_dir = PathBuf::from(std::env::var("windir")?).join("Fonts");

        let decompressor = XzDecoder::new(zip.as_slice());
        let mut tar = tar::Archive::new(decompressor);

        for file in tar.entries()? {
            let mut file = file?;
            let path = file.path()?;
            let filename = path
                .file_name()
                .with_context(|| "encountered archive entry without filename")?;
            let target = fonts_dir.join(filename);
            if let Some(ext) = path.extension() {
                if ext == "otf" || ext == "ttf" {
                    debug!(
                        "Extracting {} to {}",
                        filename.to_string_lossy(),
                        target.to_string_lossy()
                    );
                    file.unpack(target)?;
                }
            }
        }

        Ok(())
    }
}

pub fn available_fonts() -> Result<Vec<ReleaseAsset>> {
    info!("Requesting latest nerd-fonts release");
    std::thread::sleep(Duration::from_secs(1));
    let body = ureq::get("https://api.github.com/repos/ryanoasis/nerd-fonts/releases/latest")
        .call()?
        .into_string()?;
    info!("Deserializing");
    let release: Release = serde_json::from_str(&body)?;
    info!("Success");
    Ok(release.assets)
}
