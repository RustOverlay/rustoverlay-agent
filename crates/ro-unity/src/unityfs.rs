//! UnityFS archive header and directory (block info), without touching the data blocks.

use std::io::{self, Read, Seek, SeekFrom};

use crate::cursor::{Cursor, invalid};

const COMPRESSION_MASK: u32 = 0x3f;
const BLOCKS_INFO_AT_END: u32 = 0x80;
const BLOCK_INFO_NEEDS_PADDING: u32 = 0x200;
const ENCRYPTION: u32 = 0x1400;
pub(crate) const NODE_SERIALIZED_FILE: u32 = 4;
const HEADER_PROBE: usize = 4096;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Node {
    /// Absolute offset of the node's bytes in the archive file.
    pub(crate) offset: u64,
    pub(crate) size: u64,
    pub(crate) flags: u32,
    pub(crate) name: String,
}

pub(crate) fn read_directory<R: Read + Seek>(r: &mut R) -> io::Result<Vec<Node>> {
    let file_len = r.seek(SeekFrom::End(0))?;
    r.seek(SeekFrom::Start(0))?;
    let mut head = Vec::with_capacity(HEADER_PROBE);
    r.by_ref()
        .take(HEADER_PROBE as u64)
        .read_to_end(&mut head)?;

    let mut c = Cursor::new(&head, true);
    let signature = c.cstr()?;
    if signature != "UnityFS" {
        return Err(invalid(format!(
            "not a UnityFS archive (signature {signature:?})"
        )));
    }
    let version = c.u32()?;
    c.cstr()?;
    c.cstr()?;
    c.i64()?;
    let compressed = c.u32()? as usize;
    let uncompressed = c.u32()? as usize;
    let flags = c.u32()?;
    if flags & ENCRYPTION != 0 {
        return Err(invalid(format!("encrypted archive (flags {flags:#x})")));
    }
    if version >= 7 {
        c.align(16);
    }
    let header_end = c.pos() as u64;

    let info_at = if flags & BLOCKS_INFO_AT_END != 0 {
        file_len
            .checked_sub(compressed as u64)
            .ok_or_else(|| invalid("block info larger than file"))?
    } else {
        header_end
    };
    let mut raw = vec![0; compressed];
    r.seek(SeekFrom::Start(info_at))?;
    r.read_exact(&mut raw)?;
    let info = decompress(&raw, uncompressed, flags & COMPRESSION_MASK)?;

    let mut data_start = header_end;
    if flags & BLOCKS_INFO_AT_END == 0 {
        data_start += compressed as u64;
    }
    if flags & BLOCK_INFO_NEEDS_PADDING != 0 {
        data_start = data_start.next_multiple_of(16);
    }
    parse_block_info(&info, data_start)
}

fn decompress(raw: &[u8], uncompressed: usize, kind: u32) -> io::Result<Vec<u8>> {
    match kind {
        0 => Ok(raw.to_vec()),
        2 | 3 => lz4_flex::block::decompress(raw, uncompressed)
            .map_err(|e| invalid(format!("LZ4 block info: {e}"))),
        k => Err(invalid(format!("unsupported block-info compression {k}"))),
    }
}

fn parse_block_info(info: &[u8], data_start: u64) -> io::Result<Vec<Node>> {
    let mut c = Cursor::new(info, true);
    c.skip(16)?;
    let blocks = c.len_prefix()?;
    for i in 0..blocks {
        c.u32()?;
        c.u32()?;
        let block_flags = u32::from(c.u16()?);
        // Node offsets are only file offsets when every data block is stored raw.
        if block_flags & COMPRESSION_MASK != 0 {
            return Err(invalid(format!(
                "data block {i} is compressed (flags {block_flags:#x})"
            )));
        }
    }
    let count = c.len_prefix()?;
    let mut nodes = Vec::with_capacity(count);
    for _ in 0..count {
        let offset = u64::try_from(c.i64()?).map_err(|_| invalid("negative node offset"))?;
        let size = u64::try_from(c.i64()?).map_err(|_| invalid("negative node size"))?;
        let flags = c.u32()?;
        let name = c.cstr()?;
        nodes.push(Node {
            offset: data_start + offset,
            size,
            flags,
            name,
        });
    }
    Ok(nodes)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// Builds a UnityFS v8 archive with the given raw node payloads; `lz4` compresses the block info.
    pub(crate) fn archive(nodes: &[(&str, u32, &[u8])], lz4: bool, block_flags: u16) -> Vec<u8> {
        let data: Vec<u8> = nodes.iter().flat_map(|n| n.2.iter().copied()).collect();
        let mut info = vec![0u8; 16];
        info.extend(1i32.to_be_bytes());
        info.extend((data.len() as u32).to_be_bytes());
        info.extend((data.len() as u32).to_be_bytes());
        info.extend(block_flags.to_be_bytes());
        info.extend((nodes.len() as i32).to_be_bytes());
        let mut off = 0i64;
        for (name, flags, bytes) in nodes {
            info.extend(off.to_be_bytes());
            info.extend((bytes.len() as i64).to_be_bytes());
            info.extend(flags.to_be_bytes());
            info.extend(name.as_bytes());
            info.push(0);
            off += bytes.len() as i64;
        }
        let packed = if lz4 {
            lz4_flex::block::compress(&info)
        } else {
            info.clone()
        };
        let flags: u32 = 0x40 | 0x200 | if lz4 { 3 } else { 0 };
        let mut out = b"UnityFS\0".to_vec();
        out.extend(8u32.to_be_bytes());
        out.extend(b"5.x.x\0" as &[u8]);
        out.extend(b"6000.3.15x1\0" as &[u8]);
        out.extend(0i64.to_be_bytes());
        out.extend((packed.len() as u32).to_be_bytes());
        out.extend((info.len() as u32).to_be_bytes());
        out.extend(flags.to_be_bytes());
        out.resize(out.len().next_multiple_of(16), 0);
        out.extend(&packed);
        out.resize(out.len().next_multiple_of(16), 0);
        out.extend(data);
        out
    }

    #[test]
    fn reads_lz4_directory_with_padding() {
        let bytes = archive(
            &[("CAB-a", 4, b"hello"), ("CAB-a.resS", 0, b"world!")],
            true,
            0,
        );
        let nodes = read_directory(&mut std::io::Cursor::new(&bytes)).unwrap();
        assert_eq!(nodes.len(), 2);
        let (a, b) = (&nodes[0], &nodes[1]);
        assert_eq!((a.name.as_str(), a.flags, a.size), ("CAB-a", 4, 5));
        assert_eq!(&bytes[a.offset as usize..][..5], b"hello");
        assert_eq!(&bytes[b.offset as usize..][..6], b"world!");
    }

    #[test]
    fn rejects_compressed_data_blocks_and_bad_signature() {
        let bytes = archive(&[("CAB-a", 4, b"x")], false, 2);
        let err = read_directory(&mut std::io::Cursor::new(&bytes)).unwrap_err();
        assert!(err.to_string().contains("compressed"), "{err}");
        let err =
            read_directory(&mut std::io::Cursor::new(b"UnityWeb\0rest".to_vec())).unwrap_err();
        assert!(err.to_string().contains("signature"), "{err}");
    }
}
