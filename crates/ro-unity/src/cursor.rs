use std::io;

pub(crate) fn invalid(msg: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, msg.into())
}

/// Bounds-checked reader over a byte slice; alignment is relative to the slice start.
pub(crate) struct Cursor<'a> {
    buf: &'a [u8],
    pos: usize,
    big_endian: bool,
}

macro_rules! read_num {
    ($name:ident, $ty:ty) => {
        pub(crate) fn $name(&mut self) -> io::Result<$ty> {
            let b: [u8; size_of::<$ty>()] = self.bytes(size_of::<$ty>())?.try_into().unwrap();
            Ok(if self.big_endian {
                <$ty>::from_be_bytes(b)
            } else {
                <$ty>::from_le_bytes(b)
            })
        }
    };
}

impl<'a> Cursor<'a> {
    pub(crate) fn new(buf: &'a [u8], big_endian: bool) -> Self {
        Self {
            buf,
            pos: 0,
            big_endian,
        }
    }

    pub(crate) fn pos(&self) -> usize {
        self.pos
    }

    pub(crate) fn set_big_endian(&mut self, big_endian: bool) {
        self.big_endian = big_endian;
    }

    pub(crate) fn big_endian(&self) -> bool {
        self.big_endian
    }

    pub(crate) fn seek(&mut self, pos: usize) -> io::Result<()> {
        if pos > self.buf.len() {
            return Err(invalid(format!(
                "seek to {pos} past end {}",
                self.buf.len()
            )));
        }
        self.pos = pos;
        Ok(())
    }

    pub(crate) fn bytes(&mut self, n: usize) -> io::Result<&'a [u8]> {
        let end = self.pos.checked_add(n).filter(|&e| e <= self.buf.len());
        let end =
            end.ok_or_else(|| invalid(format!("read of {n} bytes at {} past end", self.pos)))?;
        let s = &self.buf[self.pos..end];
        self.pos = end;
        Ok(s)
    }

    pub(crate) fn skip(&mut self, n: usize) -> io::Result<()> {
        self.bytes(n).map(|_| ())
    }

    pub(crate) fn align(&mut self, to: usize) {
        self.pos = self.pos.next_multiple_of(to).min(self.buf.len());
    }

    pub(crate) fn u8(&mut self) -> io::Result<u8> {
        Ok(self.bytes(1)?[0])
    }

    pub(crate) fn bool(&mut self) -> io::Result<bool> {
        Ok(self.u8()? != 0)
    }

    read_num!(i8, i8);
    read_num!(i16, i16);
    read_num!(u16, u16);
    read_num!(i32, i32);
    read_num!(u32, u32);
    read_num!(i64, i64);
    read_num!(u64, u64);
    read_num!(f32, f32);
    read_num!(f64, f64);

    /// Non-negative i32 length prefix.
    pub(crate) fn len_prefix(&mut self) -> io::Result<usize> {
        let n = self.i32()?;
        usize::try_from(n).map_err(|_| invalid(format!("negative length {n}")))
    }

    pub(crate) fn cstr(&mut self) -> io::Result<String> {
        let rest = &self.buf[self.pos..];
        let end = rest
            .iter()
            .position(|&b| b == 0)
            .ok_or_else(|| invalid("unterminated string"))?;
        let s = String::from_utf8_lossy(&rest[..end]).into_owned();
        self.pos += end + 1;
        Ok(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn endian_cstr_align_bounds() {
        let buf = [0, 0, 1, 2, b'h', b'i', 0, 9, 3, 0, 0, 0];
        let mut c = Cursor::new(&buf, true);
        assert_eq!(c.u32().unwrap(), 0x0102);
        assert_eq!(c.cstr().unwrap(), "hi");
        c.align(4);
        assert_eq!(c.pos(), 8);
        c.set_big_endian(false);
        assert_eq!(c.i32().unwrap(), 3);
        assert!(c.u8().is_err());
        assert!(Cursor::new(&[1, 2], false).cstr().is_err());
        assert!(
            Cursor::new(&[0xff, 0xff, 0xff, 0xff], false)
                .len_prefix()
                .is_err()
        );
    }
}
