use crate::eth::{Mac6, ethtype};
use anyhow::{Result, anyhow, bail};
use std::net::Ipv4Addr;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

const HW_TYPE_ETHERNET: u16 = 1;
const IPV4_ADDR_SIZE_BYTES: u8 = 4;

// The kind of ARP packet - requeast or reply
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
#[repr(u16)]
enum ArpOperation {
    Request = 1,
    Reply = 2,
}

impl TryFrom<u16> for ArpOperation {
    type Error = anyhow::Error;
    fn try_from(value: u16) -> std::result::Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::Request),
            2 => Ok(Self::Reply),
            _ => Err(anyhow!("Invalid ARP operation")),
        }
    }
}

/// A parsed Ipv4/Ethernet ARP packet
///
/// We're only ever using ethernet, and Ipv6 doesn't use ARP
#[derive(Clone, PartialEq, Debug)]
pub struct ArpPacket {
    operation: ArpOperation,
    sender_hw_address: Mac6,
    sender_protocol_address: Ipv4Addr,
    target_hw_address: Mac6,
    target_protocol_address: Ipv4Addr,
}

impl ArpPacket {
    /// Parse an ARP packet from a reader
    pub async fn from_reader(mut reader: impl AsyncRead + Unpin) -> Result<Self> {
        let hw_type = reader.read_u16().await?;
        let protocol_type = reader.read_u16().await?;
        let hw_length = reader.read_u8().await?;
        let protocol_length = reader.read_u8().await?;

        if hw_type != HW_TYPE_ETHERNET {
            bail!("ARP: hardware type not supported: {hw_type}");
        } else if protocol_type != ethtype::IPV4 {
            bail!("ARP: protocol_type type not supported: {protocol_type}");
        } else if hw_length as usize != std::mem::size_of::<Mac6>() {
            bail!("ARP: hardware length not supported: {hw_length}");
        } else if protocol_length != IPV4_ADDR_SIZE_BYTES {
            bail!("ARP: bad protocol length: {protocol_length}");
        }

        let operation = ArpOperation::try_from(reader.read_u16().await?)?;
        let sender_hw_address = Mac6::from_reader(&mut reader).await?;
        let sender_protocol_address = Ipv4Addr::from_bits(reader.read_u32().await?);
        let target_hw_address = Mac6::from_reader(&mut reader).await?;
        let target_protocol_address = Ipv4Addr::from_bits(reader.read_u32().await?);

        Ok(Self {
            operation,
            sender_hw_address,
            sender_protocol_address,
            target_hw_address,
            target_protocol_address,
        })
    }

    /// Serialize an ARP packet into a writer
    pub async fn onto_writer(&mut self, mut writer: impl AsyncWrite + Unpin) -> Result<()> {
        writer.write_u16(HW_TYPE_ETHERNET).await?;
        writer.write_u16(ethtype::IPV4).await?;
        writer.write_u8(std::mem::size_of::<Mac6>() as u8).await?;
        writer.write_u8(IPV4_ADDR_SIZE_BYTES).await?;

        writer.write_u16(self.operation as u16).await?;
        writer
            .write_all(&self.sender_hw_address.into_inner())
            .await?;
        writer
            .write_u32(self.sender_protocol_address.to_bits())
            .await?;
        writer
            .write_all(&self.target_hw_address.into_inner())
            .await?;
        writer
            .write_u32(self.target_protocol_address.to_bits())
            .await?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn read() {
        let raw = [
            0x00, 0x01, 0x08, 0x00, 0x06, 0x04, 0x00, 0x01, 0x36, 0x1f, 0xb8, 0xa8, 0x1b, 0xc5,
            0xc0, 0xa8, 0x00, 0x05, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xc0, 0xa8, 0x00, 0x04,
        ];
        let arp = ArpPacket::from_reader(raw.as_slice()).await.unwrap();
        assert_eq!(arp.sender_hw_address.to_string(), "36:1F:B8:A8:1B:C5");
        assert_eq!(arp.sender_protocol_address.to_string(), "192.168.0.5");
        assert_eq!(arp.target_hw_address.to_string(), "00:00:00:00:00:00");
        assert_eq!(arp.target_protocol_address.to_string(), "192.168.0.4");
        assert_eq!(arp.operation, ArpOperation::Request);
    }

    #[tokio::test]
    async fn write() {
        let mut arp = ArpPacket {
            operation: ArpOperation::Request,
            sender_hw_address: [0x31, 0x41, 0x59, 0x26, 0x53, 0x58].into(),
            sender_protocol_address: "3.1.4.1".parse().unwrap(),
            target_hw_address: [0x27, 0x18, 0x28, 0x18, 0x28, 0x45].into(),
            target_protocol_address: "2.7.1.8".parse().unwrap(),
        };
        let mut buffer = Vec::new();
        arp.onto_writer(&mut buffer).await.unwrap();

        assert_eq!(
            arp,
            ArpPacket::from_reader(buffer.as_slice()).await.unwrap()
        );
    }
}
