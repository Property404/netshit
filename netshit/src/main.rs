#![allow(dead_code)]
use anyhow::Result;
mod eth;
use eth::EthFrame;
mod layer3;

#[tokio::main]
async fn main() -> Result<()> {
    let mut config = tun::Configuration::default();
    config
        .address((192, 168, 0, 5))
        .netmask((255, 255, 255, 0))
        .layer(tun::Layer::L2)
        .destination((192, 168, 0, 1))
        .up();

    config.platform_config(|config| {
        // requiring root privilege to acquire complete functions
        config.ensure_root_privileges(true);
    });

    let dev: tun::AsyncDevice = tun::create_as_async(&config)?;
    let mut buf = [0; 4096];

    loop {
        dev.recv(&mut buf).await?;

        match EthFrame::from_reader(buf.as_slice()).await {
            Ok(frame) => println!("{frame:?}"),
            Err(err) => println!("error: {err}"),
        }
    }
}
