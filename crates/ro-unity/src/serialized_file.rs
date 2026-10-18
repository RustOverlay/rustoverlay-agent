//! SerializedFile (asset file) header, type table and object table.

use std::collections::HashMap;
use std::io;

use crate::cursor::{Cursor, invalid};
use crate::type_tree::{self, TypeNode, Value};

const MIN_VERSION: u32 = 17;
const MONO_BEHAVIOUR: i32 = 114;

struct ObjectInfo {
    start: usize,
    size: usize,
    type_index: usize,
}

pub(crate) struct SerializedFile {
    data: Vec<u8>,
    big_endian: bool,
    types: Vec<(i32, TypeNode)>,
    objects: HashMap<i64, ObjectInfo>,
}

impl SerializedFile {
    pub(crate) fn parse(data: Vec<u8>) -> io::Result<Self> {
        let mut c = Cursor::new(&data, true);
        c.u32()?;
        c.u32()?;
        let version = c.u32()?;
        let mut data_offset = u64::from(c.u32()?);
        if !(MIN_VERSION..=23).contains(&version) {
            return Err(invalid(format!(
                "unsupported SerializedFile version {version}"
            )));
        }
        let big_endian = c.bool()?;
        c.skip(3)?;
        if version >= 22 {
            c.u32()?;
            c.i64()?;
            data_offset = u64::try_from(c.i64()?).map_err(|_| invalid("negative data offset"))?;
            c.i64()?;
        }
        c.set_big_endian(big_endian);
        c.cstr()?;
        c.i32()?;
        if !c.bool()? {
            return Err(invalid("SerializedFile has no embedded type trees"));
        }

        let type_count = c.len_prefix()?;
        let mut types = Vec::with_capacity(type_count.min(4096));
        for _ in 0..type_count {
            types.push(read_type(&mut c, version)?);
        }

        let object_count = c.len_prefix()?;
        let mut objects = HashMap::with_capacity(object_count.min(1 << 20));
        for _ in 0..object_count {
            c.align(4);
            let path_id = c.i64()?;
            let rel = if version >= 22 {
                c.i64()?
            } else {
                i64::from(c.u32()?)
            };
            let start = u64::try_from(rel)
                .ok()
                .and_then(|r| r.checked_add(data_offset));
            let start = start
                .and_then(|s| usize::try_from(s).ok())
                .ok_or_else(|| invalid("object offset"))?;
            let size = c.u32()? as usize;
            let type_index = usize::try_from(c.i32()?).ok().filter(|&t| t < types.len());
            let type_index = type_index.ok_or_else(|| invalid("object type index out of range"))?;
            if start.checked_add(size).is_none_or(|end| end > data.len()) {
                return Err(invalid(format!("object {path_id} lies outside the file")));
            }
            objects.insert(
                path_id,
                ObjectInfo {
                    start,
                    size,
                    type_index,
                },
            );
        }
        Ok(Self {
            data,
            big_endian,
            types,
            objects,
        })
    }

    pub(crate) fn class_id(&self, path_id: i64) -> Option<i32> {
        self.objects
            .get(&path_id)
            .map(|o| self.types[o.type_index].0)
    }

    pub(crate) fn path_ids_of_class(&self, class_id: i32) -> impl Iterator<Item = i64> + '_ {
        self.objects
            .iter()
            .filter(move |(_, o)| self.types[o.type_index].0 == class_id)
            .map(|(&id, _)| id)
    }

    /// Reads an object through its type tree; the tree must consume exactly the object's bytes.
    pub(crate) fn read_object(&self, path_id: i64) -> io::Result<Value<'_>> {
        let o = self
            .objects
            .get(&path_id)
            .ok_or_else(|| invalid(format!("no object {path_id}")))?;
        let bytes = &self.data[o.start..o.start + o.size];
        let mut c = Cursor::new(bytes, self.big_endian);
        let v = type_tree::read(&self.types[o.type_index].1, &mut c)?;
        if c.pos() != bytes.len() {
            return Err(invalid(format!(
                "object {path_id}: read {} of {} bytes",
                c.pos(),
                bytes.len()
            )));
        }
        Ok(v)
    }
}

fn read_type(c: &mut Cursor, version: u32) -> io::Result<(i32, TypeNode)> {
    let class_id = c.i32()?;
    c.bool()?;
    c.i16()?;
    if class_id == MONO_BEHAVIOUR {
        c.skip(16)?;
    }
    c.skip(16)?;
    if version >= 23 {
        c.skip(16)?;
        if c.i32()? == 0 {
            return Err(invalid(format!("class {class_id} has no type tree")));
        }
    }
    let tree = type_tree::parse_blob(c, version)?;
    if version >= 21 {
        let deps = c.len_prefix()?;
        c.skip(
            deps.checked_mul(4)
                .ok_or_else(|| invalid("dependency count"))?,
        )?;
    }
    Ok((class_id, tree))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::type_tree::tests::{blob, string_rows};

    /// A little-endian v22 file with one type (`class_id`, `tree` blob) and objects as (path_id, payload).
    pub(crate) fn file(class_id: i32, tree: &[u8], objects: &[(i64, Vec<u8>)]) -> Vec<u8> {
        let mut meta = b"6000.3.15f1\0".to_vec();
        meta.extend(19i32.to_le_bytes());
        meta.push(1);
        meta.extend(1i32.to_le_bytes());
        meta.extend(class_id.to_le_bytes());
        meta.push(0);
        meta.extend((-1i16).to_le_bytes());
        meta.extend([0u8; 16]);
        meta.extend(tree);
        meta.extend(0i32.to_le_bytes());
        meta.extend((objects.len() as i32).to_le_bytes());
        let header_len = 48;
        let mut payload = Vec::new();
        for (path_id, bytes) in objects {
            while !(header_len + meta.len()).is_multiple_of(4) {
                meta.push(0);
            }
            meta.extend(path_id.to_le_bytes());
            meta.extend((payload.len() as i64).to_le_bytes());
            meta.extend((bytes.len() as u32).to_le_bytes());
            meta.extend(0i32.to_le_bytes());
            payload.extend(bytes);
            payload.resize(payload.len().next_multiple_of(8), 0);
        }
        let data_offset = (header_len + meta.len()).next_multiple_of(16);
        let mut out = vec![0u8; 8];
        out.extend(22u32.to_be_bytes());
        out.extend(0u32.to_be_bytes());
        out.extend([0, 0, 0, 0]);
        out.extend((meta.len() as u32).to_be_bytes());
        out.extend(((data_offset + payload.len()) as i64).to_be_bytes());
        out.extend((data_offset as i64).to_be_bytes());
        out.extend(0i64.to_be_bytes());
        assert_eq!(out.len(), header_len);
        out.extend(&meta);
        out.resize(data_offset, 0);
        out.extend(payload);
        out
    }

    pub(crate) fn thing_tree() -> Vec<u8> {
        let mut rows = vec![(0, "Thing", "Base", 0)];
        rows.extend(string_rows(1, "m_Name"));
        rows.push((1, "int", "m_Width", 0));
        blob(&rows)
    }

    fn thing(name: &str, width: i32) -> Vec<u8> {
        let mut d = (name.len() as i32).to_le_bytes().to_vec();
        d.extend(name.as_bytes());
        d.resize(d.len().next_multiple_of(4), 0);
        d.extend(width.to_le_bytes());
        d
    }

    #[test]
    fn parses_objects_through_type_trees() {
        let sf = SerializedFile::parse(file(
            28,
            &thing_tree(),
            &[(7, thing("ab", 3)), (-9, thing("xyz", 4))],
        ))
        .unwrap();
        assert_eq!(sf.class_id(7), Some(28));
        assert_eq!(sf.path_ids_of_class(28).count(), 2);
        let v = sf.read_object(-9).unwrap();
        assert_eq!(v.field("m_Name").unwrap().str().unwrap(), "xyz");
        assert_eq!(v.field("m_Width").unwrap().int().unwrap(), 4);
    }

    #[test]
    fn rejects_size_mismatch_and_unknown_object() {
        let mut long = thing("ab", 3);
        long.extend([0; 4]);
        let sf = SerializedFile::parse(file(28, &thing_tree(), &[(1, long)])).unwrap();
        assert!(
            sf.read_object(1)
                .unwrap_err()
                .to_string()
                .contains("read 12 of 16")
        );
        assert!(sf.read_object(2).is_err());
    }
}
