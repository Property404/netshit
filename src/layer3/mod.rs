mod arp;
mod ipv4;
use anyhow::Result;
pub use arp::ArpPacket;
pub use ipv4::Ipv4Packet;
use tokio::io::{AsyncWrite, AsyncWriteExt};

#[derive(Clone, Debug)]
pub enum Layer3Packet {
    Ipv4(Ipv4Packet),
    Arp(ArpPacket),
    Unknown(Vec<u8>),
}

impl Layer3Packet {
    pub async fn onto_writer(&mut self, mut writer: impl AsyncWrite + Unpin) -> Result<()> {
        match self {
            Self::Ipv4(packet) => packet.onto_writer(writer).await?,
            Self::Arp(packet) => packet.onto_writer(writer).await?,
            Self::Unknown(packet) => writer.write_all(packet).await?,
        };

        Ok(())
    }
}
