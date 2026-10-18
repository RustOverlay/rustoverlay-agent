//! Embedded type trees (blob format) and a tree-driven value reader.

use std::io;

use crate::common_strings;
use crate::cursor::{Cursor, invalid};

const ALIGN_BYTES: i32 = 0x4000;

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct TypeNode {
    pub(crate) ty: String,
    pub(crate) name: String,
    pub(crate) meta: i32,
    pub(crate) children: Vec<TypeNode>,
}

impl TypeNode {
    fn aligned(&self) -> bool {
        self.meta & ALIGN_BYTES != 0
    }

    fn array_child(&self) -> Option<&TypeNode> {
        self.children.first().filter(|c| c.ty == "Array")
    }
}

/// Parses a type-tree blob (SerializedFile >= 12) at the cursor.
pub(crate) fn parse_blob(c: &mut Cursor, version: u32) -> io::Result<TypeNode> {
    if version >= 23 {
        if c.bytes(4)? != b"mhtt" {
            return Err(invalid("bad type tree blob magic"));
        }
        c.i32()?;
    }
    let count = c.len_prefix()?;
    let strings_len = c.len_prefix()?;
    let node_len = if version >= 19 { 32 } else { 24 };
    let raw_nodes = c.bytes(
        count
            .checked_mul(node_len)
            .ok_or_else(|| invalid("node count"))?,
    )?;
    let strings = c.bytes(strings_len)?;

    let lookup = |v: u32| -> io::Result<String> {
        if v & 0x8000_0000 != 0 {
            let s = common_strings::at(v & 0x7fff_ffff);
            return s
                .map(str::to_owned)
                .ok_or_else(|| invalid(format!("unknown common string {v:#x}")));
        }
        let mut s = Cursor::new(strings, false);
        s.seek(v as usize)?;
        s.cstr()
    };

    let mut flat = Vec::with_capacity(count);
    let mut n = Cursor::new(raw_nodes, c.big_endian());
    for _ in 0..count {
        n.i16()?;
        let level = n.u8()?;
        n.u8()?;
        let ty = lookup(n.u32()?)?;
        let name = lookup(n.u32()?)?;
        n.i32()?;
        n.i32()?;
        let meta = n.i32()?;
        if node_len == 32 {
            n.u64()?;
        }
        flat.push((
            level,
            TypeNode {
                ty,
                name,
                meta,
                children: Vec::new(),
            },
        ));
    }
    build_tree(flat)
}

fn build_tree(flat: Vec<(u8, TypeNode)>) -> io::Result<TypeNode> {
    let mut stack: Vec<(u8, TypeNode)> = Vec::new();
    for (level, node) in flat {
        while stack.last().is_some_and(|(l, _)| *l >= level) {
            fold_top(&mut stack)?;
        }
        if !stack.is_empty() && stack.last().unwrap().0 + 1 != level {
            return Err(invalid("type tree level skips a generation"));
        }
        stack.push((level, node));
    }
    while stack.len() > 1 {
        fold_top(&mut stack)?;
    }
    stack
        .pop()
        .map(|(_, n)| n)
        .ok_or_else(|| invalid("empty type tree"))
}

fn fold_top(stack: &mut Vec<(u8, TypeNode)>) -> io::Result<()> {
    let (_, node) = stack.pop().unwrap();
    let parent = stack
        .last_mut()
        .ok_or_else(|| invalid("type tree has several roots"))?;
    parent.1.children.push(node);
    Ok(())
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Value<'t> {
    Int(i64),
    UInt(u64),
    Float(f64),
    Bool(bool),
    Str(String),
    Bytes(Vec<u8>),
    Array(Vec<Value<'t>>),
    Pair(Box<(Value<'t>, Value<'t>)>),
    Struct(Vec<(&'t str, Value<'t>)>),
}

impl<'t> Value<'t> {
    pub(crate) fn field(&self, name: &str) -> io::Result<&Value<'t>> {
        let Value::Struct(fields) = self else {
            return Err(invalid(format!("no struct for {name}")));
        };
        fields
            .iter()
            .find(|(n, _)| *n == name)
            .map(|(_, v)| v)
            .ok_or_else(|| invalid(format!("missing field {name}")))
    }

    pub(crate) fn int(&self) -> io::Result<i64> {
        match *self {
            Value::Int(v) => Ok(v),
            Value::UInt(v) => i64::try_from(v).map_err(|_| invalid("integer out of range")),
            _ => Err(invalid(format!("expected integer, got {self:?}"))),
        }
    }

    pub(crate) fn str(&self) -> io::Result<&str> {
        match self {
            Value::Str(s) => Ok(s),
            _ => Err(invalid(format!("expected string, got {self:?}"))),
        }
    }

    pub(crate) fn array(&self) -> io::Result<&[Value<'t>]> {
        match self {
            Value::Array(v) => Ok(v),
            _ => Err(invalid("expected array")),
        }
    }

    pub(crate) fn pair(&self) -> io::Result<(&Value<'t>, &Value<'t>)> {
        match self {
            Value::Pair(p) => Ok((&p.0, &p.1)),
            _ => Err(invalid("expected pair")),
        }
    }
}

/// Reads one value of `node`'s type, applying its trailing alignment.
pub(crate) fn read<'t>(node: &'t TypeNode, c: &mut Cursor) -> io::Result<Value<'t>> {
    let (v, align) = read_unaligned(node, c)?;
    if align {
        c.align(4);
    }
    Ok(v)
}

/// Mirrors UnityPy: an aligned array element type aligns once after the whole array, not per element.
fn read_unaligned<'t>(node: &'t TypeNode, c: &mut Cursor) -> io::Result<(Value<'t>, bool)> {
    let mut align = node.aligned();
    let v = match node.ty.as_str() {
        "SInt8" => Value::Int(c.i8()?.into()),
        "UInt8" | "char" => Value::UInt(c.u8()?.into()),
        "short" | "SInt16" => Value::Int(c.i16()?.into()),
        "unsigned short" | "UInt16" => Value::UInt(c.u16()?.into()),
        "int" | "SInt32" => Value::Int(c.i32()?.into()),
        "unsigned int" | "UInt32" | "Type*" => Value::UInt(c.u32()?.into()),
        "long long" | "SInt64" => Value::Int(c.i64()?),
        "unsigned long long" | "UInt64" | "FileSize" => Value::UInt(c.u64()?),
        "float" => Value::Float(c.f32()?.into()),
        "double" => Value::Float(c.f64()?),
        "bool" => Value::Bool(c.bool()?),
        "string" => {
            let n = c.len_prefix()?;
            let s = String::from_utf8_lossy(c.bytes(n)?).into_owned();
            c.align(4);
            Value::Str(s)
        }
        "TypelessData" => {
            let n = c.len_prefix()?;
            Value::Bytes(c.bytes(n)?.to_vec())
        }
        "pair" => match node.children.as_slice() {
            [a, b] => Value::Pair(Box::new((read(a, c)?, read(b, c)?))),
            _ => return Err(invalid("pair without two children")),
        },
        _ => match node.array_child() {
            Some(array) => {
                align |= array.aligned();
                let elem = array
                    .children
                    .get(1)
                    .ok_or_else(|| invalid("Array without element type"))?;
                read_array(elem, c)?
            }
            None => {
                let fields = node
                    .children
                    .iter()
                    .map(|ch| Ok((ch.name.as_str(), read(ch, c)?)));
                Value::Struct(fields.collect::<io::Result<_>>()?)
            }
        },
    };
    Ok((v, align))
}

fn read_array<'t>(elem: &'t TypeNode, c: &mut Cursor) -> io::Result<Value<'t>> {
    let n = c.len_prefix()?;
    if matches!(elem.ty.as_str(), "UInt8" | "char" | "SInt8") {
        let bytes = c.bytes(n)?.to_vec();
        if elem.aligned() {
            c.align(4);
        }
        return Ok(Value::Bytes(bytes));
    }
    // Capped so a corrupt length cannot reserve gigabytes up front.
    let mut out = Vec::with_capacity(n.min(1 << 16));
    if elem.aligned() {
        for _ in 0..n {
            out.push(read_unaligned(elem, c)?.0);
        }
        c.align(4);
    } else {
        for _ in 0..n {
            out.push(read(elem, c)?);
        }
    }
    Ok(Value::Array(out))
}

#[cfg(test)]
#[path = "type_tree_tests.rs"]
pub(crate) mod tests;
