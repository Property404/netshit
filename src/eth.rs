use crate::layer3::{ArpPacket, Ipv4Packet, Layer3Packet};
use anyhow::{Result, bail};
use tokio::io::{AsyncRead, AsyncReadExt};

pub mod ethtype {
    pub const IPV4: u16 = 0x0800;
    pub const ARP: u16 = 0x0806;
    pub const IPV6: u16 = 0x86dd;
}

/// A 48-bit ethernet MAC address
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct Mac6 {
    inner: [u8; 6],
}

impl From<[u8; 6]> for Mac6 {
    fn from(inner: [u8; 6]) -> Self {
        Self { inner }
    }
}

impl std::fmt::Display for Mac6 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut colon = false;
        for val in self.inner.iter() {
            if colon {
                write!(f, ":")?;
            }
            write!(f, "{:02X}", val)?;
            colon = true;
        }
        Ok(())
    }
}

impl Mac6 {
    pub const fn into_inner(self) -> [u8; 6] {
        self.inner
    }

    pub async fn from_reader(mut reader: impl AsyncRead + Unpin) -> std::io::Result<Self> {
        let mut buf = [0; 6];
        reader.read_exact(&mut buf).await?;
        Ok(Self::from(buf))
    }
}

#[derive(Clone, Debug)]
pub struct EthFrame {
    /// Destination MAC
    dst: Mac6,
    /// source MAC
    src: Mac6,
    ethtype: u16,
    payload: Layer3Packet,
}

impl EthFrame {
    pub async fn from_reader(mut reader: impl AsyncRead + Unpin) -> Result<Self> {
        let mut dst = [0; 6];
        reader.read_exact(&mut dst).await?;

        let mut src = [0; 6];
        reader.read_exact(&mut src).await?;

        let ethtype = reader.read_u16().await?;

        let payload = match ethtype {
            // Empty frame
            0 => Layer3Packet::Unknown(Vec::new()),
            // If it's under 1536 it's the length
            1..1536 => {
                let mut payload = vec![0; ethtype as usize];
                reader.read_exact(&mut payload).await?;
                Layer3Packet::Unknown(payload)
            }
            ethtype::IPV4 => Layer3Packet::Ipv4(Ipv4Packet::from_reader(reader).await?),
            ethtype::ARP => Layer3Packet::Arp(ArpPacket::from_reader(reader).await?),
            _ => {
                bail!("Unknown eth type: 0x{ethtype:04x}");
            }
        };

        Ok(Self {
            dst: Mac6::from(dst),
            src: Mac6::from(src),
            ethtype,
            payload,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn parse_basic_frame() {
        let raw_frame = vec![
            1, 0, 94, 0, 0, 251, 54, 31, 184, 168, 27, 197, 8, 0, 69, 0, 0, 163, 172, 55, 64, 0,
            255, 17, 45, 105, 192, 168, 0, 5, 224, 0, 0, 251, 20, 233, 20, 233, 0, 143, 88, 51, 0,
            0, 132, 0, 0, 0, 0, 4, 0, 0, 0, 0, 11, 80, 97, 115, 115, 105, 109, 45, 70, 52, 67, 56,
            6, 95, 99, 97, 99, 104, 101, 4, 95, 116, 99, 112, 5, 108, 111, 99, 97, 108, 0, 0, 16,
            128, 1, 0, 0, 17, 148, 0, 1, 0, 6, 102, 101, 100, 111, 114, 97, 192, 36, 0, 1, 128, 1,
            0, 0, 0, 120, 0, 4, 192, 168, 0, 5, 1, 53, 1, 48, 3, 49, 54, 56, 3, 49, 57, 50, 7, 105,
            110, 45, 97, 100, 100, 114, 4, 97, 114, 112, 97, 0, 0, 12, 128, 1, 0, 0, 0, 120, 0, 2,
            192, 54, 192, 12, 0, 33, 128, 1, 0, 0, 0, 120, 0, 8, 0, 0, 0, 0, 107, 108, 192, 54,
        ];

        let frame = EthFrame::from_reader(raw_frame.as_slice()).await.unwrap();

        assert_eq!(frame.ethtype, ethtype::IPV4);
        let Layer3Packet::Ipv4(packet) = frame.payload else {
            panic!("Wrong packet type!");
        };
        assert_eq!(packet.source.to_string(), "192.168.0.5");
        assert_eq!(packet.destination.to_string(), "224.0.0.251");
    }

    #[test]
    fn format_mac() {
        assert_eq!(
            Mac6::from([3, 1, 4, 1, 5, 9]).to_string(),
            "03:01:04:01:05:09"
        );
    }
}
