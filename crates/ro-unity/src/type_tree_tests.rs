use super::*;

const A: i32 = ALIGN_BYTES;

/// Encodes `(level, type, name, meta)` rows as a little-endian v22 blob; "int" uses the common table.
pub(crate) fn blob(rows: &[(u8, &str, &str, i32)]) -> Vec<u8> {
    let mut strings = Vec::new();
    let mut intern = |s: &str| -> u32 {
        if s == "int" {
            return 0x8000_0000 | crate::common_strings::offset_of("int");
        }
        let at = strings.len() as u32;
        strings.extend(s.as_bytes());
        strings.push(0);
        at
    };
    let mut nodes = Vec::new();
    for &(level, ty, name, meta) in rows {
        nodes.extend(1i16.to_le_bytes());
        nodes.push(level);
        nodes.push(0);
        nodes.extend(intern(ty).to_le_bytes());
        nodes.extend(intern(name).to_le_bytes());
        nodes.extend((-1i32).to_le_bytes());
        nodes.extend(0i32.to_le_bytes());
        nodes.extend(meta.to_le_bytes());
        nodes.extend(0u64.to_le_bytes());
    }
    let mut out = (rows.len() as i32).to_le_bytes().to_vec();
    out.extend((strings.len() as i32).to_le_bytes());
    out.extend(nodes);
    out.extend(strings);
    out
}

pub(crate) fn string_rows(
    level: u8,
    name: &'static str,
) -> [(u8, &'static str, &'static str, i32); 4] {
    [
        (level, "string", name, 0x8000),
        (level + 1, "Array", "Array", A | 1),
        (level + 2, "int", "size", 1),
        (level + 2, "char", "data", 1),
    ]
}

fn sample_tree() -> TypeNode {
    let mut rows = vec![(0, "Thing", "Base", 0x8000)];
    rows.extend(string_rows(1, "m_Name"));
    rows.extend([
        (1, "bool", "m_Flag", A),
        (1, "int", "m_Width", 0),
        (1, "map", "m_Container", 0x8000),
        (2, "Array", "Array", 0x8000),
        (3, "int", "size", 0),
        (3, "pair", "data", 0x8000),
    ]);
    rows.extend(string_rows(4, "first"));
    rows.extend([
        (4, "PPtr<Object>", "second", 0),
        (5, "int", "m_FileID", 0),
        (5, "SInt64", "m_PathID", 0),
        (1, "vector", "m_Flags", 0x8000),
        (2, "Array", "Array", A),
        (3, "int", "size", 0),
        (3, "bool", "data", A),
        (1, "UInt64", "m_Tail", 0),
    ]);
    let bytes = blob(&rows);
    parse_blob(&mut Cursor::new(&bytes, false), 22).unwrap()
}

#[test]
fn blob_builds_nested_tree_with_common_strings() {
    let t = sample_tree();
    assert_eq!((t.ty.as_str(), t.children.len()), ("Thing", 6));
    assert_eq!(t.children[2].ty, "int");
    let pair = &t.children[3].children[0].children[1];
    assert_eq!(pair.ty, "pair");
    assert_eq!(pair.children[1].children[1].name, "m_PathID");
}

#[test]
fn reads_values_with_alignment() {
    let t = sample_tree();
    let mut d = Vec::new();
    d.extend(3i32.to_le_bytes());
    d.extend(b"abc\0");
    d.extend([1, 0, 0, 0]);
    d.extend(256i32.to_le_bytes());
    d.extend(1i32.to_le_bytes());
    d.extend(5i32.to_le_bytes());
    d.extend(b"hello\0\0\0");
    d.extend(0i32.to_le_bytes());
    d.extend(42i64.to_le_bytes());
    d.extend(3i32.to_le_bytes());
    d.extend([1, 0, 1, 0]);
    d.extend(7u64.to_le_bytes());

    let mut c = Cursor::new(&d, false);
    let v = read(&t, &mut c).unwrap();
    assert_eq!(c.pos(), d.len());
    assert_eq!(v.field("m_Name").unwrap().str().unwrap(), "abc");
    assert_eq!(v.field("m_Flag").unwrap(), &Value::Bool(true));
    assert_eq!(v.field("m_Width").unwrap().int().unwrap(), 256);
    let entries = v.field("m_Container").unwrap().array().unwrap();
    let (k, ptr) = entries[0].pair().unwrap();
    assert_eq!(k.str().unwrap(), "hello");
    assert_eq!(ptr.field("m_PathID").unwrap().int().unwrap(), 42);
    assert_eq!(v.field("m_Flags").unwrap().array().unwrap().len(), 3);
    assert_eq!(v.field("m_Tail").unwrap().int().unwrap(), 7);
}

#[test]
fn rejects_truncated_and_malformed_input() {
    let t = sample_tree();
    assert!(read(&t, &mut Cursor::new(&[3, 0, 0, 0, b'a'], false)).is_err());
    let bad = blob(&[(0, "A", "a", 0), (2, "B", "b", 0)]);
    assert!(parse_blob(&mut Cursor::new(&bad, false), 22).is_err());
    let two_roots = blob(&[(0, "A", "a", 0), (0, "B", "b", 0)]);
    assert!(parse_blob(&mut Cursor::new(&two_roots, false), 22).is_err());
}
