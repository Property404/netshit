use anyhow::{Result, bail};
use std::net::Ipv4Addr;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

const MIN_HEADER_LENGTH: u8 = 20; // in bytes
const DONT_FRAGMENT: u16 = 0x2;

/// A parsed Internet Protocol version 4 packet
#[derive(Clone, Debug, PartialEq)]
pub struct Ipv4Packet {
    /// Differentiated Service Code Point
    pub dscp: u8,
    /// Explicit congestion notification
    pub ecn: u8,
    pub identification: u16,
    /// Time-to-live
    pub ttl: u8,
    pub protocol: u8,
    pub source: Ipv4Addr,
    pub destination: Ipv4Addr,
    pub data: Vec<u8>,
}

impl Ipv4Packet {
    /// Parse an IPv4 packet from a reader
    pub async fn from_reader(mut reader: impl AsyncRead + Unpin) -> Result<Self> {
        let mut hasher = internet_checksum::Checksum::new();

        let (version, ihl) = {
            let byte = reader.read_u8().await?;
            hasher.add_bytes(&[byte]);
            (byte >> 4, byte & 0x0F)
        };

        if version != 4 {
            bail!("Trying to parse non-IPv4 packet as IPv4");
        }

        let ihl = match ihl {
            0 => MIN_HEADER_LENGTH,
            // According to https://en.wikipedia.org/wiki/IPv4,
            // IHL either zero or >= 5
            1..5 => {
                bail!("Invalid IHL value: 0x{ihl:02x}");
            }
            // If >=5, ihl is number of 32-bit words in header
            _ => 4 * ihl,
        };

        let (dscp, ecn) = {
            let byte = reader.read_u8().await?;
            hasher.add_bytes(&[byte]);
            (byte >> 2, byte & 0x03)
        };

        let total_length = reader.read_u16().await?;
        if total_length < (ihl as u16) {
            bail!("Bad packet length: 0x{total_length:02x}");
        }
        hasher.add_bytes(&total_length.to_be_bytes());

        let identification = reader.read_u16().await?;
        hasher.add_bytes(&identification.to_be_bytes());
        let flags_and_frag_offset = reader.read_u16().await?;
        hasher.add_bytes(&flags_and_frag_offset.to_be_bytes());
        if flags_and_frag_offset != DONT_FRAGMENT << 13 {
            bail!("Fragmenting not supported:{flags_and_frag_offset:02x}");
        }

        let ttl = reader.read_u8().await?;
        hasher.add_bytes(&[ttl]);
        if total_length < (ihl as u16) {
            bail!("Ipv4: Bad packet length: 0x{total_length:02x}");
        }
        let protocol = reader.read_u8().await?;
        hasher.add_bytes(&[protocol]);
        let header_checksum = reader.read_u16().await?;
        hasher.add_bytes(&header_checksum.to_be_bytes());
        let source = Ipv4Addr::from_bits(reader.read_u32().await?);
        hasher.add_bytes(&source.to_bits().to_be_bytes());
        let destination = Ipv4Addr::from_bits(reader.read_u32().await?);
        hasher.add_bytes(&destination.to_bits().to_be_bytes());

        if ihl > MIN_HEADER_LENGTH {
            let options_size = ihl - MIN_HEADER_LENGTH;
            let mut buffer = vec![0; options_size as usize];
            reader.read_exact(&mut buffer).await?;
            bail!("Ipv4: options not supported");
        }

        if hasher.checksum() != [0, 0] {
            bail!("Invalid checksum");
        }

        let payload_length = (total_length as u64) - (ihl as u64);
        let mut data = Vec::new();

        reader.take(payload_length).read_to_end(&mut data).await?;

        if data.len() != payload_length.try_into()? {
            bail!("IPv4: Unexpected end of payload");
        }

        Ok(Self {
            dscp,
            ecn,
            identification,
            ttl,
            protocol,
            source,
            destination,
            data,
        })
    }

    /// Serialize an IPv4 packet into a writer
    pub async fn onto_writer(&mut self, mut writer: impl AsyncWrite + Unpin) -> Result<()> {
        let mut hasher = internet_checksum::Checksum::new();
        let mut write_bytes = async |bytes| -> Result<()> {
            writer.write_all(bytes).await?;
            hasher.add_bytes(bytes);
            Ok(())
        };

        // Write version(4) and IHL(5)
        write_bytes(&[(4 << 4) | 5]).await?;

        // Write DSCP|ECN
        if self.ecn > 0b11 {
            bail!("IPv4: Invalid ECN");
        }
        let val = [(self.dscp << 2) | self.ecn];
        write_bytes(&val).await?;

        // Write total length (minimum header length is 20)
        let total_length = (20u16 + u16::try_from(self.data.len())?).to_be_bytes();
        write_bytes(&total_length).await?;

        // Write identification
        let identification = self.identification.to_be_bytes();
        write_bytes(&identification).await?;

        // Write flags | fragment offset
        write_bytes(&[(DONT_FRAGMENT as u8) << 5, 0]).await?;

        // Write TTL | protocol
        let ttl_plus_protocol = [self.ttl, self.protocol];
        write_bytes(&ttl_plus_protocol).await?;

        // Write Header checksum
        // Kind of annoying that the checksum includes fields after
        // the checksum field - so we have to stop using `write_bytes` here
        hasher.add_bytes(&self.source.to_bits().to_be_bytes());
        hasher.add_bytes(&self.destination.to_bits().to_be_bytes());
        writer.write_all(&hasher.checksum()).await?;

        // Write IP addresses
        writer.write_u32(self.source.to_bits()).await?;
        writer.write_u32(self.destination.to_bits()).await?;

        // Write data
        writer.write_all(&self.data).await?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Result;

    #[tokio::test]
    async fn read() -> Result<()> {
        let raw = [
            0x45, 0x00, 0x00, 0xb2, 0xb2, 0xfe, 0x40, 0x00, 0xff, 0x11, 0x26, 0x93, 0xc0, 0xa8,
            0x00, 0x05, 0xe0, 0x00, 0x00, 0xfb, 0x14, 0xe9, 0x14, 0xe9, 0x00, 0x9e, 0xd3, 0x0b,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x01, 0x34,
            0x01, 0x65, 0x01, 0x62, 0x01, 0x65, 0x01, 0x37, 0x01, 0x30, 0x01, 0x65, 0x01, 0x66,
            0x01, 0x66, 0x01, 0x66, 0x01, 0x36, 0x01, 0x37, 0x01, 0x38, 0x01, 0x64, 0x01, 0x30,
            0x01, 0x64, 0x01, 0x30, 0x01, 0x30, 0x01, 0x30, 0x01, 0x30, 0x01, 0x30, 0x01, 0x30,
            0x01, 0x30, 0x01, 0x30, 0x01, 0x30, 0x01, 0x30, 0x01, 0x30, 0x01, 0x30, 0x01, 0x30,
            0x01, 0x38, 0x01, 0x65, 0x01, 0x66, 0x03, 0x69, 0x70, 0x36, 0x04, 0x61, 0x72, 0x70,
            0x61, 0x00, 0x00, 0xff, 0x00, 0x01, 0x06, 0x66, 0x65, 0x64, 0x6f, 0x72, 0x61, 0x05,
            0x6c, 0x6f, 0x63, 0x61, 0x6c, 0x00, 0x00, 0xff, 0x00, 0x01, 0xc0, 0x5a, 0x00, 0x1c,
            0x00, 0x01, 0x00, 0x00, 0x00, 0x78, 0x00, 0x10, 0xfe, 0x80, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0xd0, 0xd8, 0x76, 0xff, 0xfe, 0x07, 0xeb, 0xe4, 0xc0, 0x0c, 0x00, 0x0c,
            0x00, 0x01, 0x00, 0x00, 0x00, 0x78, 0x00, 0x02, 0xc0, 0x5a,
        ];

        let mut packet = Ipv4Packet::from_reader(raw.as_slice()).await?;
        assert_eq!(packet.identification, 0xb2fe);
        assert_eq!(packet.ttl, 255);
        assert_eq!(packet.dscp, 0);
        assert_eq!(packet.ecn, 0);
        assert_eq!(packet.protocol, 0x11);
        assert_eq!(packet.source.to_string(), "192.168.0.5");
        assert_eq!(packet.destination.to_string(), "224.0.0.251");

        let mut vec = Vec::new();
        packet.onto_writer(&mut vec).await?;

        assert_eq!(Vec::from(raw), vec);

        Ipv4Packet::from_reader(vec.as_slice()).await?;

        Ok(())
    }

    #[tokio::test]
    async fn write() -> Result<()> {
        let mut packet = Ipv4Packet {
            dscp: 0,
            ttl: 8,
            ecn: 0,
            identification: 0x1234,
            protocol: 0x11,
            source: "1.2.3.4".parse()?,
            destination: "5.6.7.8".parse()?,
            data: vec![3, 1, 4, 1],
        };

        let mut vec = Vec::new();

        packet.onto_writer(&mut vec).await?;

        println!("{vec:?}");

        assert_eq!(Ipv4Packet::from_reader(vec.as_slice()).await?, packet);

        Ok(())
    }
}
