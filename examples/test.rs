use std::path::{Path, PathBuf};

use futures_lite::{future::block_on, AsyncWriteExt};

fn main() {
    env_logger::Builder::from_default_env()
        .format_timestamp_millis()
        .init();

    let fname: PathBuf = std::env::args_os().nth(1).expect("no output filename").into();

    block_on(capture_to_file(&fname)).unwrap();
}

async fn capture_to_file(fname: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let mut out = async_fs::File::create(fname).await?;
    let dev = fx2la::Device::open().await?.ok_or("no device found")?;

    let mut capture = dev.start_capture(20_000).await?;

    loop {
        let data = capture.read().await?;
        log::debug!("Recieved {} bytes", data.len());
        out.write_all(data).await?;
    }
}
