//! The serde bridge between Rust types and [`Value`].
//!
//! Structs and maps become maps, struct fields keywords (`a/b` names are
//! namespaced), sequences and tuples vectors, `None` and `()` nil, unit
//! variants keywords and other variants a one-entry map from the variant's
//! keyword. Integers must be exact in binary64; strings must be Unicode.
use super::{Error, Options, Value, print, print_pretty, read_one};
use ::serde::{
    de::{self, DeserializeOwned, IntoDeserializer, Visitor},
    ser::{self, Serialize},
};
use crate::{Keyword, Symbol};
use std::collections::{BTreeMap, btree_map, btree_set};

impl ser::Error for Error {
    fn custom<T: std::fmt::Display>(message: T) -> Self {
        Error::new(message.to_string())
    }
}
impl de::Error for Error {
    fn custom<T: std::fmt::Display>(message: T) -> Self {
        Error::new(message.to_string())
    }
}

pub fn to_value<T: Serialize + ?Sized>(value: &T) -> Result<Value, Error> {
    value.serialize(Serializer)
}
/// Canonical one-line EDN for `value`.
pub fn to_string<T: Serialize + ?Sized>(value: &T) -> Result<String, Error> {
    print(&to_value(value)?)
}
/// Canonical EDN for `value`, one entry per line where wider than 80 columns.
pub fn to_string_pretty<T: Serialize + ?Sized>(value: &T) -> Result<String, Error> {
    print_pretty(&to_value(value)?)
}
pub fn from_value<T: DeserializeOwned>(value: Value) -> Result<T, Error> {
    T::deserialize(Deserializer(value))
}
/// Read exactly one EDN value with the default options and deserialize it.
pub fn from_str<T: DeserializeOwned>(source: &str) -> Result<T, Error> {
    from_value(read_one(source, &Options::default())?)
}

fn keyword(name: &str) -> Value {
    Value::Keyword(Keyword::parse(name))
}
const EXACT: u128 = 1 << 53;
fn integer(negative: bool, magnitude: u128) -> Result<Value, Error> {
    if magnitude > EXACT {
        let sign = if negative { "-" } else { "" };
        return Err(Error::new(format!(
            "Integer {sign}{magnitude} is not exact in binary64"
        )));
    }
    let x = magnitude as f64;
    Ok(Value::Number(if negative { -x } else { x }))
}

struct Serializer;
impl ser::Serializer for Serializer {
    type Ok = Value;
    type Error = Error;
    type SerializeSeq = Sequence;
    type SerializeTuple = Sequence;
    type SerializeTupleStruct = Sequence;
    type SerializeTupleVariant = Sequence;
    type SerializeMap = Map;
    type SerializeStruct = Map;
    type SerializeStructVariant = Map;

    fn serialize_bool(self, v: bool) -> Result<Value, Error> {
        Ok(Value::Bool(v))
    }
    fn serialize_i8(self, v: i8) -> Result<Value, Error> {
        self.serialize_i128(v.into())
    }
    fn serialize_i16(self, v: i16) -> Result<Value, Error> {
        self.serialize_i128(v.into())
    }
    fn serialize_i32(self, v: i32) -> Result<Value, Error> {
        self.serialize_i128(v.into())
    }
    fn serialize_i64(self, v: i64) -> Result<Value, Error> {
        self.serialize_i128(v.into())
    }
    fn serialize_i128(self, v: i128) -> Result<Value, Error> {
        integer(v < 0, v.unsigned_abs())
    }
    fn serialize_u8(self, v: u8) -> Result<Value, Error> {
        self.serialize_u128(v.into())
    }
    fn serialize_u16(self, v: u16) -> Result<Value, Error> {
        self.serialize_u128(v.into())
    }
    fn serialize_u32(self, v: u32) -> Result<Value, Error> {
        self.serialize_u128(v.into())
    }
    fn serialize_u64(self, v: u64) -> Result<Value, Error> {
        self.serialize_u128(v.into())
    }
    fn serialize_u128(self, v: u128) -> Result<Value, Error> {
        integer(false, v)
    }
    fn serialize_f32(self, v: f32) -> Result<Value, Error> {
        Ok(Value::Number(v.into()))
    }
    fn serialize_f64(self, v: f64) -> Result<Value, Error> {
        Ok(Value::Number(v))
    }
    fn serialize_char(self, v: char) -> Result<Value, Error> {
        Ok(Value::String(v.into()))
    }
    fn serialize_str(self, v: &str) -> Result<Value, Error> {
        Ok(Value::String(v.into()))
    }
    fn serialize_bytes(self, v: &[u8]) -> Result<Value, Error> {
        Ok(Value::Vector(v.iter().map(|&b| Value::Number(b.into())).collect()))
    }
    fn serialize_none(self) -> Result<Value, Error> {
        Ok(Value::Nil)
    }
    fn serialize_some<T: Serialize + ?Sized>(self, value: &T) -> Result<Value, Error> {
        value.serialize(self)
    }
    fn serialize_unit(self) -> Result<Value, Error> {
        Ok(Value::Nil)
    }
    fn serialize_unit_struct(self, _: &'static str) -> Result<Value, Error> {
        Ok(Value::Nil)
    }
    fn serialize_unit_variant(
        self,
        _: &'static str,
        _: u32,
        variant: &'static str,
    ) -> Result<Value, Error> {
        Ok(keyword(variant))
    }
    fn serialize_newtype_struct<T: Serialize + ?Sized>(
        self,
        _: &'static str,
        value: &T,
    ) -> Result<Value, Error> {
        value.serialize(self)
    }
    fn serialize_newtype_variant<T: Serialize + ?Sized>(
        self,
        _: &'static str,
        _: u32,
        variant: &'static str,
        value: &T,
    ) -> Result<Value, Error> {
        Ok(Value::Map(BTreeMap::from([(keyword(variant), to_value(value)?)])))
    }
    fn serialize_seq(self, len: Option<usize>) -> Result<Sequence, Error> {
        Ok(Sequence {
            variant: None,
            items: Vec::with_capacity(len.unwrap_or(0)),
        })
    }
    fn serialize_tuple(self, len: usize) -> Result<Sequence, Error> {
        self.serialize_seq(Some(len))
    }
    fn serialize_tuple_struct(self, _: &'static str, len: usize) -> Result<Sequence, Error> {
        self.serialize_seq(Some(len))
    }
    fn serialize_tuple_variant(
        self,
        _: &'static str,
        _: u32,
        variant: &'static str,
        len: usize,
    ) -> Result<Sequence, Error> {
        Ok(Sequence {
            variant: Some(variant),
            items: Vec::with_capacity(len),
        })
    }
    fn serialize_map(self, _: Option<usize>) -> Result<Map, Error> {
        Ok(Map::default())
    }
    fn serialize_struct(self, _: &'static str, _: usize) -> Result<Map, Error> {
        Ok(Map::default())
    }
    fn serialize_struct_variant(
        self,
        _: &'static str,
        _: u32,
        variant: &'static str,
        _: usize,
    ) -> Result<Map, Error> {
        Ok(Map {
            variant: Some(variant),
            ..Map::default()
        })
    }
}

/// Wrap a variant's content in its one-entry map.
fn variant(variant: Option<&'static str>, value: Value) -> Value {
    match variant {
        Some(name) => Value::Map(BTreeMap::from([(keyword(name), value)])),
        None => value,
    }
}

struct Sequence {
    variant: Option<&'static str>,
    items: Vec<Value>,
}
impl Sequence {
    fn push<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), Error> {
        self.items.push(to_value(value)?);
        Ok(())
    }
    fn finish(self) -> Result<Value, Error> {
        Ok(variant(self.variant, Value::Vector(self.items)))
    }
}
impl ser::SerializeSeq for Sequence {
    type Ok = Value;
    type Error = Error;
    fn serialize_element<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), Error> {
        self.push(value)
    }
    fn end(self) -> Result<Value, Error> {
        self.finish()
    }
}
impl ser::SerializeTuple for Sequence {
    type Ok = Value;
    type Error = Error;
    fn serialize_element<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), Error> {
        self.push(value)
    }
    fn end(self) -> Result<Value, Error> {
        self.finish()
    }
}
impl ser::SerializeTupleStruct for Sequence {
    type Ok = Value;
    type Error = Error;
    fn serialize_field<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), Error> {
        self.push(value)
    }
    fn end(self) -> Result<Value, Error> {
        self.finish()
    }
}
impl ser::SerializeTupleVariant for Sequence {
    type Ok = Value;
    type Error = Error;
    fn serialize_field<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), Error> {
        self.push(value)
    }
    fn end(self) -> Result<Value, Error> {
        self.finish()
    }
}

#[derive(Default)]
struct Map {
    variant: Option<&'static str>,
    entries: BTreeMap<Value, Value>,
    key: Option<Value>,
}
impl Map {
    fn insert(&mut self, key: Value, value: Value) -> Result<(), Error> {
        if self.entries.contains_key(&key) {
            let printed = print(&key).unwrap_or_else(|_| format!("{key:?}"));
            return Err(Error::new(format!("Duplicate key: {printed}")));
        }
        self.entries.insert(key, value);
        Ok(())
    }
    fn finish(self) -> Result<Value, Error> {
        Ok(variant(self.variant, Value::Map(self.entries)))
    }
}
impl ser::SerializeMap for Map {
    type Ok = Value;
    type Error = Error;
    fn serialize_key<T: Serialize + ?Sized>(&mut self, key: &T) -> Result<(), Error> {
        self.key = Some(to_value(key)?);
        Ok(())
    }
    fn serialize_value<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), Error> {
        let key = self.key.take().ok_or_else(|| Error::new("Map value without a key"))?;
        self.insert(key, to_value(value)?)
    }
    fn end(self) -> Result<Value, Error> {
        self.finish()
    }
}
impl ser::SerializeStruct for Map {
    type Ok = Value;
    type Error = Error;
    fn serialize_field<T: Serialize + ?Sized>(
        &mut self,
        name: &'static str,
        value: &T,
    ) -> Result<(), Error> {
        self.insert(keyword(name), to_value(value)?)
    }
    fn end(self) -> Result<Value, Error> {
        self.finish()
    }
}
impl ser::SerializeStructVariant for Map {
    type Ok = Value;
    type Error = Error;
    fn serialize_field<T: Serialize + ?Sized>(
        &mut self,
        name: &'static str,
        value: &T,
    ) -> Result<(), Error> {
        self.insert(keyword(name), to_value(value)?)
    }
    fn end(self) -> Result<Value, Error> {
        self.finish()
    }
}

fn name(namespace: Option<String>, name: String) -> String {
    match namespace {
        Some(namespace) => format!("{namespace}/{name}"),
        None => name,
    }
}
fn describe(value: &Value) -> String {
    print(value).unwrap_or_else(|_| format!("{value:?}"))
}

struct Deserializer(Value);
impl<'de> de::Deserializer<'de> for Deserializer {
    type Error = Error;
    fn deserialize_any<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        match self.0 {
            Value::Nil => visitor.visit_unit(),
            Value::Bool(b) => visitor.visit_bool(b),
            Value::Number(x) if x.fract() == 0.0 && x.abs() <= EXACT as f64 && !(x == 0.0 && x.is_sign_negative()) => {
                if x >= 0.0 {
                    visitor.visit_u64(x as u64)
                } else {
                    visitor.visit_i64(x as i64)
                }
            }
            Value::Number(x) => visitor.visit_f64(x),
            Value::String(s) => visitor.visit_string(s),
            Value::Keyword(Keyword { namespace, name: n }) | Value::Symbol(Symbol { namespace, name: n }) => {
                visitor.visit_string(name(namespace, n))
            }
            Value::List(items) | Value::Vector(items) => {
                visitor.visit_seq(de::value::SeqDeserializer::new(items.into_iter().map(Deserializer)))
            }
            Value::Set(items) => visitor.visit_seq(SetAccess(items.into_iter())),
            Value::Map(entries) => visitor.visit_map(MapAccess {
                entries: entries.into_iter(),
                value: None,
            }),
            value @ Value::Tagged(..) => Err(Error::new(format!(
                "Cannot deserialize the tagged value {}",
                describe(&value)
            ))),
            Value::Meta(value, _) => Deserializer(*value).deserialize_any(visitor),
        }
    }
    fn deserialize_f64<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        match self.0 {
            Value::Number(x) => visitor.visit_f64(x),
            value => Deserializer(value).deserialize_any(visitor),
        }
    }
    fn deserialize_f32<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        self.deserialize_f64(visitor)
    }
    fn deserialize_option<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        match self.0 {
            Value::Nil => visitor.visit_none(),
            Value::Meta(value, _) => Deserializer(*value).deserialize_option(visitor),
            value => visitor.visit_some(Deserializer(value)),
        }
    }
    fn deserialize_newtype_struct<V: Visitor<'de>>(
        self,
        _: &'static str,
        visitor: V,
    ) -> Result<V::Value, Error> {
        visitor.visit_newtype_struct(self)
    }
    fn deserialize_enum<V: Visitor<'de>>(
        self,
        _: &'static str,
        _: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Error> {
        match self.0 {
            variant @ (Value::Keyword(_) | Value::String(_)) => visitor.visit_enum(EnumAccess {
                variant,
                value: None,
            }),
            Value::Map(entries) if entries.len() == 1 => {
                let (variant, value) = entries.into_iter().next().unwrap();
                visitor.visit_enum(EnumAccess {
                    variant,
                    value: Some(value),
                })
            }
            Value::Meta(value, _) => {
                Deserializer(*value).deserialize_enum("", &[], visitor)
            }
            value => Err(Error::new(format!(
                "Expected a keyword or one-entry map for an enum, found {}",
                describe(&value)
            ))),
        }
    }
    ::serde::forward_to_deserialize_any! {
        bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 char str string bytes byte_buf
        unit unit_struct seq tuple tuple_struct map struct identifier ignored_any
    }
}
impl<'de> IntoDeserializer<'de, Error> for Deserializer {
    type Deserializer = Self;
    fn into_deserializer(self) -> Self {
        self
    }
}

struct SetAccess(btree_set::IntoIter<Value>);
impl<'de> de::SeqAccess<'de> for SetAccess {
    type Error = Error;
    fn next_element_seed<T: de::DeserializeSeed<'de>>(
        &mut self,
        seed: T,
    ) -> Result<Option<T::Value>, Error> {
        self.0.next().map(|item| seed.deserialize(Deserializer(item))).transpose()
    }
    fn size_hint(&self) -> Option<usize> {
        Some(self.0.len())
    }
}

struct MapAccess {
    entries: btree_map::IntoIter<Value, Value>,
    value: Option<Value>,
}
impl<'de> de::MapAccess<'de> for MapAccess {
    type Error = Error;
    fn next_key_seed<K: de::DeserializeSeed<'de>>(
        &mut self,
        seed: K,
    ) -> Result<Option<K::Value>, Error> {
        let Some((key, value)) = self.entries.next() else {
            return Ok(None);
        };
        self.value = Some(value);
        seed.deserialize(Deserializer(key)).map(Some)
    }
    fn next_value_seed<V: de::DeserializeSeed<'de>>(&mut self, seed: V) -> Result<V::Value, Error> {
        let value = self.value.take().ok_or_else(|| Error::new("Map value without a key"))?;
        seed.deserialize(Deserializer(value))
    }
    fn size_hint(&self) -> Option<usize> {
        Some(self.entries.len())
    }
}

struct EnumAccess {
    variant: Value,
    value: Option<Value>,
}
impl<'de> de::EnumAccess<'de> for EnumAccess {
    type Error = Error;
    type Variant = VariantAccess;
    fn variant_seed<V: de::DeserializeSeed<'de>>(
        self,
        seed: V,
    ) -> Result<(V::Value, VariantAccess), Error> {
        let variant = seed.deserialize(Deserializer(self.variant))?;
        Ok((variant, VariantAccess(self.value)))
    }
}
struct VariantAccess(Option<Value>);
impl VariantAccess {
    fn content(self) -> Result<Value, Error> {
        self.0.ok_or_else(|| Error::new("Expected variant content"))
    }
}
impl<'de> de::VariantAccess<'de> for VariantAccess {
    type Error = Error;
    fn unit_variant(self) -> Result<(), Error> {
        match self.0 {
            None | Some(Value::Nil) => Ok(()),
            Some(value) => Err(Error::new(format!(
                "Unexpected content {} for a unit variant",
                describe(&value)
            ))),
        }
    }
    fn newtype_variant_seed<T: de::DeserializeSeed<'de>>(self, seed: T) -> Result<T::Value, Error> {
        seed.deserialize(Deserializer(self.content()?))
    }
    fn tuple_variant<V: Visitor<'de>>(self, _: usize, visitor: V) -> Result<V::Value, Error> {
        de::Deserializer::deserialize_seq(Deserializer(self.content()?), visitor)
    }
    fn struct_variant<V: Visitor<'de>>(
        self,
        _: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Error> {
        de::Deserializer::deserialize_map(Deserializer(self.content()?), visitor)
    }
}
