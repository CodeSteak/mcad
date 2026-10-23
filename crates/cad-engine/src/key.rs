//! Content-based cache keys.
//!
//! A [`CacheKey`] is a blake3 hash over everything that determines a node's geometry in its
//! own system: the operation with its resolved parameters, `$fillet`/`$chamfer`, the keys of
//! the contributing children together with their relative matrices and modifier flags, and
//! the kernel id. The world matrix of a node is never part of its key, so equal parts at
//! different places share one cache entry.
//!
//! Parameters are fed through a small `serde::Serializer` ([`KeyHasher`]) so every field of
//! `cad_ir::Op` is covered without hand-written encoders. Floats are canonicalised
//! (`-0.0` → `0.0`, all NaNs → one NaN) before hashing.

use std::fmt;

use glam::DMat4;
use serde::ser::{self, Serialize};

/// Stable content hash of a node (32 bytes of blake3).
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CacheKey(pub [u8; 32]);

impl CacheKey {
    pub fn to_hex(&self) -> String {
        self.0.iter().map(|b| format!("{b:02x}")).collect()
    }

    /// Inverse of [`CacheKey::to_hex`] (64 hex digits, any case).
    pub fn from_hex(s: &str) -> Option<CacheKey> {
        let b = s.as_bytes();
        if b.len() != 64 {
            return None;
        }
        let mut out = [0u8; 32];
        for (i, o) in out.iter_mut().enumerate() {
            let hi = (b[2 * i] as char).to_digit(16)?;
            let lo = (b[2 * i + 1] as char).to_digit(16)?;
            *o = (hi * 16 + lo) as u8;
        }
        Some(CacheKey(out))
    }

    /// First 8 bytes as an integer (compact display, `inst_key`-like use).
    pub fn short(&self) -> u64 {
        u64::from_le_bytes(self.0[..8].try_into().unwrap())
    }
}

impl fmt::Debug for CacheKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "CacheKey({})", &self.to_hex()[..16])
    }
}

impl fmt::Display for CacheKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_hex())
    }
}

impl Serialize for CacheKey {
    fn serialize<S: ser::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_hex())
    }
}

/// Canonical bit pattern of a float for hashing.
pub fn canon_f64(v: f64) -> u64 {
    if v == 0.0 {
        0
    } else if v.is_nan() {
        0x7ff8_0000_0000_0000
    } else {
        v.to_bits()
    }
}

/// Incremental key builder.
pub struct KeyHasher {
    h: blake3::Hasher,
}

impl Default for KeyHasher {
    fn default() -> Self {
        Self::new()
    }
}

impl KeyHasher {
    pub fn new() -> Self {
        KeyHasher { h: blake3::Hasher::new() }
    }

    pub fn tag(&mut self, t: u8) -> &mut Self {
        self.h.update(&[t]);
        self
    }

    pub fn u64(&mut self, v: u64) -> &mut Self {
        self.h.update(&v.to_le_bytes());
        self
    }

    pub fn f64(&mut self, v: f64) -> &mut Self {
        self.u64(canon_f64(v))
    }

    pub fn str(&mut self, s: &str) -> &mut Self {
        self.u64(s.len() as u64);
        self.h.update(s.as_bytes());
        self
    }

    pub fn bytes(&mut self, b: &[u8]) -> &mut Self {
        self.u64(b.len() as u64);
        self.h.update(b);
        self
    }

    pub fn mat(&mut self, m: &DMat4) -> &mut Self {
        for v in m.to_cols_array() {
            self.f64(v);
        }
        self
    }

    pub fn key(&mut self, k: &CacheKey) -> &mut Self {
        self.h.update(&k.0);
        self
    }

    /// Hashes any serialisable value (canonical floats).
    pub fn value<T: Serialize + ?Sized>(&mut self, v: &T) -> &mut Self {
        // Serialisation into the hasher cannot fail for the IR types (no maps with
        // non-string keys are rejected; everything is accepted).
        let _ = v.serialize(&mut *self);
        self
    }

    pub fn finish(&self) -> CacheKey {
        CacheKey(*self.h.finalize().as_bytes())
    }
}

#[derive(Debug)]
pub struct KeyError(String);

impl fmt::Display for KeyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for KeyError {}

impl ser::Error for KeyError {
    fn custom<T: fmt::Display>(msg: T) -> Self {
        KeyError(msg.to_string())
    }
}

type R = Result<(), KeyError>;

impl ser::Serializer for &mut KeyHasher {
    type Ok = ();
    type Error = KeyError;
    type SerializeSeq = Self;
    type SerializeTuple = Self;
    type SerializeTupleStruct = Self;
    type SerializeTupleVariant = Self;
    type SerializeMap = Self;
    type SerializeStruct = Self;
    type SerializeStructVariant = Self;

    fn serialize_bool(self, v: bool) -> R {
        self.tag(1).tag(v as u8);
        Ok(())
    }
    fn serialize_i8(self, v: i8) -> R {
        self.serialize_i64(v as i64)
    }
    fn serialize_i16(self, v: i16) -> R {
        self.serialize_i64(v as i64)
    }
    fn serialize_i32(self, v: i32) -> R {
        self.serialize_i64(v as i64)
    }
    fn serialize_i64(self, v: i64) -> R {
        self.tag(2).u64(v as u64);
        Ok(())
    }
    fn serialize_u8(self, v: u8) -> R {
        self.serialize_u64(v as u64)
    }
    fn serialize_u16(self, v: u16) -> R {
        self.serialize_u64(v as u64)
    }
    fn serialize_u32(self, v: u32) -> R {
        self.serialize_u64(v as u64)
    }
    fn serialize_u64(self, v: u64) -> R {
        self.tag(3).u64(v);
        Ok(())
    }
    fn serialize_f32(self, v: f32) -> R {
        self.serialize_f64(v as f64)
    }
    fn serialize_f64(self, v: f64) -> R {
        self.tag(4).f64(v);
        Ok(())
    }
    fn serialize_char(self, v: char) -> R {
        self.serialize_u64(v as u64)
    }
    fn serialize_str(self, v: &str) -> R {
        self.tag(5).str(v);
        Ok(())
    }
    fn serialize_bytes(self, v: &[u8]) -> R {
        self.tag(6).bytes(v);
        Ok(())
    }
    fn serialize_none(self) -> R {
        self.tag(7);
        Ok(())
    }
    fn serialize_some<T: ?Sized + Serialize>(self, v: &T) -> R {
        self.tag(8);
        v.serialize(self)
    }
    fn serialize_unit(self) -> R {
        self.tag(9);
        Ok(())
    }
    fn serialize_unit_struct(self, _name: &'static str) -> R {
        self.serialize_unit()
    }
    fn serialize_unit_variant(self, _n: &'static str, idx: u32, _v: &'static str) -> R {
        self.tag(10).u64(idx as u64);
        Ok(())
    }
    fn serialize_newtype_struct<T: ?Sized + Serialize>(self, _n: &'static str, v: &T) -> R {
        v.serialize(self)
    }
    fn serialize_newtype_variant<T: ?Sized + Serialize>(
        self,
        _n: &'static str,
        idx: u32,
        _v: &'static str,
        value: &T,
    ) -> R {
        self.tag(11).u64(idx as u64);
        value.serialize(self)
    }
    fn serialize_seq(self, len: Option<usize>) -> Result<Self, KeyError> {
        self.tag(12).u64(len.map(|l| l as u64).unwrap_or(u64::MAX));
        Ok(self)
    }
    fn serialize_tuple(self, len: usize) -> Result<Self, KeyError> {
        self.tag(13).u64(len as u64);
        Ok(self)
    }
    fn serialize_tuple_struct(self, _n: &'static str, len: usize) -> Result<Self, KeyError> {
        self.serialize_tuple(len)
    }
    fn serialize_tuple_variant(
        self,
        _n: &'static str,
        idx: u32,
        _v: &'static str,
        len: usize,
    ) -> Result<Self, KeyError> {
        self.tag(14).u64(idx as u64).u64(len as u64);
        Ok(self)
    }
    fn serialize_map(self, len: Option<usize>) -> Result<Self, KeyError> {
        self.tag(15).u64(len.map(|l| l as u64).unwrap_or(u64::MAX));
        Ok(self)
    }
    fn serialize_struct(self, _n: &'static str, len: usize) -> Result<Self, KeyError> {
        self.tag(16).u64(len as u64);
        Ok(self)
    }
    fn serialize_struct_variant(
        self,
        _n: &'static str,
        idx: u32,
        _v: &'static str,
        len: usize,
    ) -> Result<Self, KeyError> {
        self.tag(17).u64(idx as u64).u64(len as u64);
        Ok(self)
    }
}

impl ser::SerializeSeq for &mut KeyHasher {
    type Ok = ();
    type Error = KeyError;
    fn serialize_element<T: ?Sized + Serialize>(&mut self, v: &T) -> R {
        v.serialize(&mut **self)
    }
    fn end(self) -> R {
        self.tag(18);
        Ok(())
    }
}

impl ser::SerializeTuple for &mut KeyHasher {
    type Ok = ();
    type Error = KeyError;
    fn serialize_element<T: ?Sized + Serialize>(&mut self, v: &T) -> R {
        v.serialize(&mut **self)
    }
    fn end(self) -> R {
        Ok(())
    }
}

impl ser::SerializeTupleStruct for &mut KeyHasher {
    type Ok = ();
    type Error = KeyError;
    fn serialize_field<T: ?Sized + Serialize>(&mut self, v: &T) -> R {
        v.serialize(&mut **self)
    }
    fn end(self) -> R {
        Ok(())
    }
}

impl ser::SerializeTupleVariant for &mut KeyHasher {
    type Ok = ();
    type Error = KeyError;
    fn serialize_field<T: ?Sized + Serialize>(&mut self, v: &T) -> R {
        v.serialize(&mut **self)
    }
    fn end(self) -> R {
        Ok(())
    }
}

impl ser::SerializeMap for &mut KeyHasher {
    type Ok = ();
    type Error = KeyError;
    fn serialize_key<T: ?Sized + Serialize>(&mut self, k: &T) -> R {
        k.serialize(&mut **self)
    }
    fn serialize_value<T: ?Sized + Serialize>(&mut self, v: &T) -> R {
        v.serialize(&mut **self)
    }
    fn end(self) -> R {
        self.tag(19);
        Ok(())
    }
}

impl ser::SerializeStruct for &mut KeyHasher {
    type Ok = ();
    type Error = KeyError;
    fn serialize_field<T: ?Sized + Serialize>(&mut self, name: &'static str, v: &T) -> R {
        self.str(name);
        v.serialize(&mut **self)
    }
    fn end(self) -> R {
        Ok(())
    }
}

impl ser::SerializeStructVariant for &mut KeyHasher {
    type Ok = ();
    type Error = KeyError;
    fn serialize_field<T: ?Sized + Serialize>(&mut self, name: &'static str, v: &T) -> R {
        self.str(name);
        v.serialize(&mut **self)
    }
    fn end(self) -> R {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn negative_zero_is_canonical() {
        let a = KeyHasher::new().value(&(0.0f64, [1.0f64, -0.0])).finish();
        let b = KeyHasher::new().value(&(-0.0f64, [1.0f64, 0.0])).finish();
        assert_eq!(a, b);
        let c = KeyHasher::new().value(&(0.0f64, [1.0f64, 1e-300])).finish();
        assert_ne!(a, c);
    }
}
