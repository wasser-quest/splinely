//! Compact arena for parsed documents plus the two fast-path helpers built on it:
//! a `serde` deserializer over the arena (no intermediate `serde_json::Value`)
//! and a comparing serializer that replaces `to_value` + `check_preserved`.
//!
//! The arena is only ever a *fast path*: whenever the fast path cannot prove the
//! strict result, callers rebuild a `Value` (`Tree::to_value`) and run the
//! reference pipeline, so errors and edge cases stay byte-for-byte identical.
use crate::json_loading::numbers_equivalent;
use serde::de::{
    self, DeserializeSeed, EnumAccess, Error as _, IntoDeserializer, MapAccess, SeqAccess,
    Unexpected, VariantAccess, Visitor,
};
use serde::ser::{
    self, Serialize, SerializeMap, SerializeSeq, SerializeStruct, SerializeStructVariant,
    SerializeTuple, SerializeTupleStruct, SerializeTupleVariant,
};
use serde_json::{Map, Number, Value};

#[derive(Clone, Copy)]
pub(crate) struct Entry<'a> {
    pub key: &'a str,
    /// String-table index of `key` (binary input only; duplicate detection).
    pub idx: u32,
    pub node: u32,
}

#[derive(Clone, Copy)]
pub(crate) enum Node<'a> {
    Null,
    Bool(bool),
    U64(u64),
    /// Always negative when produced by the binary decoder or `from_value`.
    I64(i64),
    F64(f64),
    Str(&'a str),
    /// Index into `Tree::owned` (decoded base64 text).
    Owned(u32),
    Array {
        start: u32,
        len: u32,
    },
    Object {
        start: u32,
        len: u32,
    },
    /// Little-endian f64 payload of a `TAG_FLOATS` array (all finite).
    Floats(&'a [u8]),
}

/// Read-only view of a node with children resolved to slices.
#[derive(Clone, Copy)]
pub(crate) enum View<'t> {
    Null,
    Bool(bool),
    U64(u64),
    I64(i64),
    F64(f64),
    Str(&'t str),
    Array(&'t [u32]),
    Floats(&'t [u8]),
    Object(&'t [Entry<'t>]),
}

#[derive(Default)]
pub(crate) struct Tree<'a> {
    pub nodes: Vec<Node<'a>>,
    pub kids: Vec<u32>,
    pub entries: Vec<Entry<'a>>,
    pub owned: Vec<String>,
}

pub(crate) const ROOT: u32 = 0;

fn float_at(bytes: &[u8], i: usize) -> f64 {
    let mut a = [0u8; 8];
    a.copy_from_slice(&bytes[i * 8..i * 8 + 8]);
    f64::from_le_bytes(a)
}

impl<'a> Tree<'a> {
    pub fn push(&mut self, node: Node<'a>) -> u32 {
        self.nodes.push(node);
        (self.nodes.len() - 1) as u32
    }

    /// Mirror of a `Value` tree; strings borrow from the value. Root is `ROOT`.
    pub fn from_value(value: &'a Value) -> Self {
        fn walk<'a>(tree: &mut Tree<'a>, value: &'a Value) -> u32 {
            match value {
                Value::Null => tree.push(Node::Null),
                Value::Bool(b) => tree.push(Node::Bool(*b)),
                Value::Number(n) => tree.push(if let Some(u) = n.as_u64() {
                    Node::U64(u)
                } else if let Some(i) = n.as_i64() {
                    Node::I64(i)
                } else {
                    Node::F64(n.as_f64().unwrap_or(0.0))
                }),
                Value::String(s) => tree.push(Node::Str(s)),
                Value::Array(items) => {
                    let id = tree.push(Node::Null);
                    let start = tree.kids.len();
                    tree.kids.resize(start + items.len(), 0);
                    for (i, item) in items.iter().enumerate() {
                        let child = walk(tree, item);
                        tree.kids[start + i] = child;
                    }
                    tree.nodes[id as usize] = Node::Array {
                        start: start as u32,
                        len: items.len() as u32,
                    };
                    id
                }
                Value::Object(map) => {
                    let id = tree.push(Node::Null);
                    let start = tree.entries.len();
                    tree.entries.resize(
                        start + map.len(),
                        Entry {
                            key: "",
                            idx: 0,
                            node: 0,
                        },
                    );
                    for (i, (key, item)) in map.iter().enumerate() {
                        let child = walk(tree, item);
                        tree.entries[start + i] = Entry {
                            key,
                            idx: 0,
                            node: child,
                        };
                    }
                    tree.nodes[id as usize] = Node::Object {
                        start: start as u32,
                        len: map.len() as u32,
                    };
                    id
                }
            }
        }
        let mut tree = Tree::default();
        walk(&mut tree, value);
        tree
    }

    pub fn view(&self, id: u32) -> View<'_> {
        match self.nodes[id as usize] {
            Node::Null => View::Null,
            Node::Bool(b) => View::Bool(b),
            Node::U64(u) => View::U64(u),
            Node::I64(i) => View::I64(i),
            Node::F64(f) => View::F64(f),
            Node::Str(s) => View::Str(s),
            Node::Owned(i) => View::Str(&self.owned[i as usize]),
            Node::Array { start, len } => {
                View::Array(&self.kids[start as usize..(start + len) as usize])
            }
            Node::Object { start, len } => {
                View::Object(&self.entries[start as usize..(start + len) as usize])
            }
            Node::Floats(b) => View::Floats(b),
        }
    }

    /// Object member by key (`None` for non-objects).
    pub fn get(&self, id: u32, key: &str) -> Option<u32> {
        match self.view(id) {
            View::Object(entries) => entries.iter().find(|e| e.key == key).map(|e| e.node),
            _ => None,
        }
    }

    /// Array elements as node ids; float arrays yield `None` (no element nodes).
    pub fn array_ids(&self, id: u32) -> Option<&[u32]> {
        match self.view(id) {
            View::Array(kids) => Some(kids),
            _ => None,
        }
    }

    /// True for arrays of zero elements (either representation).
    pub fn is_empty_array(&self, id: u32) -> bool {
        match self.view(id) {
            View::Array(kids) => kids.is_empty(),
            View::Floats(bytes) => bytes.is_empty(),
            _ => false,
        }
    }

    pub fn to_value(&self, id: u32) -> Value {
        match self.view(id) {
            View::Null => Value::Null,
            View::Bool(b) => Value::Bool(b),
            View::U64(u) => Value::from(u),
            View::I64(i) => Value::from(i),
            View::F64(f) => Number::from_f64(f).map_or(Value::Null, Value::Number),
            View::Str(s) => Value::String(s.to_owned()),
            View::Array(kids) => Value::Array(kids.iter().map(|&k| self.to_value(k)).collect()),
            View::Floats(bytes) => Value::Array(
                (0..bytes.len() / 8)
                    .map(|i| {
                        Number::from_f64(float_at(bytes, i)).map_or(Value::Null, Value::Number)
                    })
                    .collect(),
            ),
            View::Object(entries) => {
                let mut map = Map::new();
                for e in entries {
                    map.insert(e.key.to_owned(), self.to_value(e.node));
                }
                Value::Object(map)
            }
        }
    }

    pub fn deserializer(&self) -> De<'_> {
        De {
            tree: self,
            view: self.view(ROOT),
        }
    }
}

// ------------------------------------------------------------ deserializer

/// `serde` deserializer over the arena. Mirrors `impl Deserializer for &Value`:
/// integers are reported as `u64`/`i64`, floats as `f64`, so int-vs-float kinds
/// decide type acceptance exactly as for JSON.
#[derive(Clone, Copy)]
pub(crate) struct De<'t> {
    tree: &'t Tree<'t>,
    view: View<'t>,
}

struct KidsAccess<'t> {
    tree: &'t Tree<'t>,
    iter: std::slice::Iter<'t, u32>,
}
impl<'t> SeqAccess<'t> for KidsAccess<'t> {
    type Error = serde_json::Error;
    fn next_element_seed<S: DeserializeSeed<'t>>(
        &mut self,
        seed: S,
    ) -> Result<Option<S::Value>, Self::Error> {
        match self.iter.next() {
            Some(&id) => seed
                .deserialize(De {
                    tree: self.tree,
                    view: self.tree.view(id),
                })
                .map(Some),
            None => Ok(None),
        }
    }
    fn size_hint(&self) -> Option<usize> {
        Some(self.iter.len())
    }
}

struct FloatsAccess<'t> {
    tree: &'t Tree<'t>,
    bytes: &'t [u8],
    next: usize,
}
impl<'t> SeqAccess<'t> for FloatsAccess<'t> {
    type Error = serde_json::Error;
    fn next_element_seed<S: DeserializeSeed<'t>>(
        &mut self,
        seed: S,
    ) -> Result<Option<S::Value>, Self::Error> {
        if self.next * 8 >= self.bytes.len() {
            return Ok(None);
        }
        let value = float_at(self.bytes, self.next);
        self.next += 1;
        seed.deserialize(De {
            tree: self.tree,
            view: View::F64(value),
        })
        .map(Some)
    }
    fn size_hint(&self) -> Option<usize> {
        Some(self.bytes.len() / 8 - self.next)
    }
}

struct ObjectAccess<'t> {
    tree: &'t Tree<'t>,
    iter: std::slice::Iter<'t, Entry<'t>>,
    value: Option<u32>,
}
impl<'t> MapAccess<'t> for ObjectAccess<'t> {
    type Error = serde_json::Error;
    fn next_key_seed<K: DeserializeSeed<'t>>(
        &mut self,
        seed: K,
    ) -> Result<Option<K::Value>, Self::Error> {
        match self.iter.next() {
            Some(entry) => {
                self.value = Some(entry.node);
                seed.deserialize(KeyDe(entry.key)).map(Some)
            }
            None => Ok(None),
        }
    }
    fn next_value_seed<V: DeserializeSeed<'t>>(
        &mut self,
        seed: V,
    ) -> Result<V::Value, Self::Error> {
        let node = self
            .value
            .take()
            .ok_or_else(|| serde_json::Error::custom("value is missing"))?;
        seed.deserialize(De {
            tree: self.tree,
            view: self.tree.view(node),
        })
    }
    fn size_hint(&self) -> Option<usize> {
        Some(self.iter.len())
    }
}

/// Object keys are always strings (like serde_json's `MapKeyDeserializer`
/// without numeric-key parsing; numeric keys fall back to the reference path).
struct KeyDe<'t>(&'t str);
impl<'t> de::Deserializer<'t> for KeyDe<'t> {
    type Error = serde_json::Error;
    fn deserialize_any<V: Visitor<'t>>(self, visitor: V) -> Result<V::Value, Self::Error> {
        visitor.visit_borrowed_str(self.0)
    }
    fn deserialize_option<V: Visitor<'t>>(self, visitor: V) -> Result<V::Value, Self::Error> {
        visitor.visit_some(self)
    }
    fn deserialize_newtype_struct<V: Visitor<'t>>(
        self,
        _name: &'static str,
        visitor: V,
    ) -> Result<V::Value, Self::Error> {
        visitor.visit_newtype_struct(self)
    }
    fn deserialize_enum<V: Visitor<'t>>(
        self,
        _name: &'static str,
        _variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error> {
        visitor.visit_enum(IntoDeserializer::<serde_json::Error>::into_deserializer(
            self.0,
        ))
    }
    serde::forward_to_deserialize_any! { <V: Visitor<'t>>
        bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string bytes byte_buf
        unit unit_struct seq tuple tuple_struct map struct identifier ignored_any
    }
}

struct EnumAcc<'t> {
    tree: &'t Tree<'t>,
    variant: &'t str,
    value: Option<View<'t>>,
}
impl<'t> EnumAccess<'t> for EnumAcc<'t> {
    type Error = serde_json::Error;
    type Variant = VariantAcc<'t>;
    fn variant_seed<V: DeserializeSeed<'t>>(
        self,
        seed: V,
    ) -> Result<(V::Value, Self::Variant), Self::Error> {
        let variant = seed.deserialize(
            IntoDeserializer::<serde_json::Error>::into_deserializer(self.variant),
        )?;
        Ok((
            variant,
            VariantAcc {
                tree: self.tree,
                value: self.value,
            },
        ))
    }
}
struct VariantAcc<'t> {
    tree: &'t Tree<'t>,
    value: Option<View<'t>>,
}
impl<'t> VariantAcc<'t> {
    fn de(&self) -> Result<De<'t>, serde_json::Error> {
        self.value
            .map(|view| De {
                tree: self.tree,
                view,
            })
            .ok_or_else(|| {
                serde_json::Error::invalid_type(Unexpected::UnitVariant, &"variant data")
            })
    }
}
impl<'t> VariantAccess<'t> for VariantAcc<'t> {
    type Error = serde_json::Error;
    fn unit_variant(self) -> Result<(), Self::Error> {
        match self.de() {
            Ok(de) => de::Deserialize::deserialize(de),
            Err(_) => Ok(()),
        }
    }
    fn newtype_variant_seed<S: DeserializeSeed<'t>>(
        self,
        seed: S,
    ) -> Result<S::Value, Self::Error> {
        seed.deserialize(self.de()?)
    }
    fn tuple_variant<V: Visitor<'t>>(
        self,
        _len: usize,
        visitor: V,
    ) -> Result<V::Value, Self::Error> {
        de::Deserializer::deserialize_seq(self.de()?, visitor)
    }
    fn struct_variant<V: Visitor<'t>>(
        self,
        _fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error> {
        de::Deserializer::deserialize_any(self.de()?, visitor)
    }
}

impl<'t> de::Deserializer<'t> for De<'t> {
    type Error = serde_json::Error;

    fn deserialize_any<V: Visitor<'t>>(self, visitor: V) -> Result<V::Value, Self::Error> {
        match self.view {
            View::Null => visitor.visit_unit(),
            View::Bool(b) => visitor.visit_bool(b),
            View::U64(u) => visitor.visit_u64(u),
            View::I64(i) => visitor.visit_i64(i),
            View::F64(f) => visitor.visit_f64(f),
            View::Str(s) => visitor.visit_borrowed_str(s),
            View::Array(kids) => {
                let mut access = KidsAccess {
                    tree: self.tree,
                    iter: kids.iter(),
                };
                let value = visitor.visit_seq(&mut access)?;
                if access.iter.len() == 0 {
                    Ok(value)
                } else {
                    Err(serde_json::Error::invalid_length(
                        kids.len(),
                        &"fewer elements in array",
                    ))
                }
            }
            View::Floats(bytes) => {
                let mut access = FloatsAccess {
                    tree: self.tree,
                    bytes,
                    next: 0,
                };
                let value = visitor.visit_seq(&mut access)?;
                if access.next * 8 == bytes.len() {
                    Ok(value)
                } else {
                    Err(serde_json::Error::invalid_length(
                        bytes.len() / 8,
                        &"fewer elements in array",
                    ))
                }
            }
            View::Object(entries) => {
                let mut access = ObjectAccess {
                    tree: self.tree,
                    iter: entries.iter(),
                    value: None,
                };
                let value = visitor.visit_map(&mut access)?;
                if access.iter.len() == 0 {
                    Ok(value)
                } else {
                    Err(serde_json::Error::invalid_length(
                        entries.len(),
                        &"fewer elements in map",
                    ))
                }
            }
        }
    }

    fn deserialize_option<V: Visitor<'t>>(self, visitor: V) -> Result<V::Value, Self::Error> {
        match self.view {
            View::Null => visitor.visit_none(),
            _ => visitor.visit_some(self),
        }
    }

    fn deserialize_newtype_struct<V: Visitor<'t>>(
        self,
        _name: &'static str,
        visitor: V,
    ) -> Result<V::Value, Self::Error> {
        visitor.visit_newtype_struct(self)
    }

    fn deserialize_enum<V: Visitor<'t>>(
        self,
        _name: &'static str,
        _variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error> {
        match self.view {
            View::Str(variant) => visitor.visit_enum(EnumAcc {
                tree: self.tree,
                variant,
                value: None,
            }),
            View::Object(entries) if entries.len() == 1 => visitor.visit_enum(EnumAcc {
                tree: self.tree,
                variant: entries[0].key,
                value: Some(self.tree.view(entries[0].node)),
            }),
            _ => Err(serde_json::Error::custom(
                "expected string or map with a single key",
            )),
        }
    }

    fn deserialize_ignored_any<V: Visitor<'t>>(self, visitor: V) -> Result<V::Value, Self::Error> {
        visitor.visit_unit()
    }

    serde::forward_to_deserialize_any! { <V: Visitor<'t>>
        bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string bytes byte_buf
        unit unit_struct seq tuple tuple_struct map struct identifier
    }
}

// ------------------------------------------------------------ comparing serializer

/// "Not provably identical": the caller must use the reference path.
#[derive(Debug)]
pub(crate) struct Bail;
impl std::fmt::Display for Bail {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("fast check bailed")
    }
}
impl std::error::Error for Bail {}
impl ser::Error for Bail {
    fn custom<T: std::fmt::Display>(_: T) -> Self {
        Bail
    }
}
type Fast = Result<(), Bail>;

struct Ctx<'t> {
    tree: &'t Tree<'t>,
    /// Per-object "input member was matched" flags, used as a stack.
    seen: Vec<bool>,
    /// No canonical member was missing from the input.
    complete: bool,
}

/// True only if `check_preserved(<tree as Value>, to_value(value))` would
/// certainly succeed: every input member is present in the canonical form,
/// arrays keep their length and numbers stay exactly representable. `false`
/// means "unknown": use the reference check to learn the exact outcome.
pub(crate) fn preserved_by<T: Serialize>(tree: &Tree<'_>, value: &T) -> bool {
    preservation(tree, value).is_some()
}

/// `preserved_by`, plus `Some(true)` when the input also has every canonical
/// member: then the input is the canonical form up to number spelling and
/// member order.
pub(crate) fn preservation<T: Serialize>(tree: &Tree<'_>, value: &T) -> Option<bool> {
    let mut ctx = Ctx {
        tree,
        seen: Vec::new(),
        complete: true,
    };
    value
        .serialize(Check {
            ctx: &mut ctx,
            view: tree.view(ROOT),
        })
        .ok()?;
    Some(ctx.complete)
}

struct Check<'c, 't> {
    ctx: &'c mut Ctx<'t>,
    view: View<'t>,
}

fn input_number(view: View<'_>) -> Option<Number> {
    match view {
        View::U64(u) => Some(Number::from(u)),
        View::I64(i) => Some(Number::from(i)),
        View::F64(f) => Number::from_f64(f),
        _ => None,
    }
}

impl<'c, 't> Check<'c, 't> {
    fn number(self, canonical: Number) -> Fast {
        let input = input_number(self.view).ok_or(Bail)?;
        if numbers_equivalent(&input, &canonical) {
            Ok(())
        } else {
            Err(Bail)
        }
    }
    fn string(self, canonical: &str) -> Fast {
        match self.view {
            View::Str(s) if s == canonical => Ok(()),
            _ => Err(Bail),
        }
    }
    fn null(self) -> Fast {
        matches!(self.view, View::Null).then_some(()).ok_or(Bail)
    }
    /// Canonical `{variant: …}`: input must be exactly that single member.
    fn single(self, variant: &str) -> Result<Check<'c, 't>, Bail> {
        match self.view {
            View::Object([entry]) if entry.key == variant => Ok(Check {
                view: self.ctx.tree.view(entry.node),
                ctx: self.ctx,
            }),
            _ => Err(Bail),
        }
    }
    fn object(self) -> Result<ObjectCheck<'c, 't>, Bail> {
        match self.view {
            View::Object(entries) => {
                let base = self.ctx.seen.len();
                self.ctx.seen.resize(base + entries.len(), false);
                Ok(ObjectCheck {
                    ctx: self.ctx,
                    entries,
                    base,
                    next: 0,
                    pending: None,
                })
            }
            _ => Err(Bail),
        }
    }
    fn sequence(self) -> Result<SeqCheck<'c, 't>, Bail> {
        match self.view {
            View::Array(_) | View::Floats(_) => Ok(SeqCheck {
                ctx: self.ctx,
                view: self.view,
                index: 0,
            }),
            _ => Err(Bail),
        }
    }
}

struct SeqCheck<'c, 't> {
    ctx: &'c mut Ctx<'t>,
    view: View<'t>,
    index: usize,
}
impl SeqCheck<'_, '_> {
    fn element<T: ?Sized + Serialize>(&mut self, value: &T) -> Fast {
        let view = match self.view {
            View::Array(kids) => self.ctx.tree.view(*kids.get(self.index).ok_or(Bail)?),
            View::Floats(bytes) if self.index * 8 < bytes.len() => {
                View::F64(float_at(bytes, self.index))
            }
            _ => return Err(Bail),
        };
        self.index += 1;
        value.serialize(Check {
            ctx: &mut *self.ctx,
            view,
        })
    }
    fn finish(self) -> Fast {
        let len = match self.view {
            View::Array(kids) => kids.len(),
            View::Floats(bytes) => bytes.len() / 8,
            _ => return Err(Bail),
        };
        if self.index == len {
            Ok(())
        } else {
            Err(Bail)
        }
    }
}

struct ObjectCheck<'c, 't> {
    ctx: &'c mut Ctx<'t>,
    entries: &'t [Entry<'t>],
    base: usize,
    /// Position hint: canonical and input members usually share an order.
    next: usize,
    pending: Option<Option<usize>>,
}
impl ObjectCheck<'_, '_> {
    /// `Ok(None)`: canonical-only member (nothing to compare).
    fn find(&mut self, key: &str) -> Result<Option<usize>, Bail> {
        if self.entries.get(self.next).is_some_and(|e| e.key == key) {
            self.next += 1;
            return Ok(Some(self.next - 1));
        }
        // Avoid quadratic scans over huge objects; the reference path is linear.
        if self.entries.len() > 64 {
            return Err(Bail);
        }
        let found = self.entries.iter().position(|e| e.key == key);
        if let Some(i) = found {
            self.next = i + 1;
        }
        Ok(found)
    }
    fn member<T: ?Sized + Serialize>(&mut self, found: Option<usize>, value: &T) -> Fast {
        let Some(i) = found else {
            self.ctx.complete = false;
            return Ok(());
        };
        // A member written twice would make the last one win in `to_value`.
        if std::mem::replace(&mut self.ctx.seen[self.base + i], true) {
            return Err(Bail);
        }
        let view = self.ctx.tree.view(self.entries[i].node);
        value.serialize(Check {
            ctx: &mut *self.ctx,
            view,
        })
    }
    fn field<T: ?Sized + Serialize>(&mut self, key: &str, value: &T) -> Fast {
        let found = self.find(key)?;
        self.member(found, value)
    }
    fn finish(self) -> Fast {
        let frame = &self.ctx.seen[self.base..];
        let all = frame.iter().all(|&s| s);
        self.ctx.seen.truncate(self.base);
        if all {
            Ok(())
        } else {
            Err(Bail)
        }
    }
}

/// Keys of canonical maps must be strings; locate them among the input members.
struct KeyLookup<'a, 'c, 't>(&'a mut ObjectCheck<'c, 't>);
macro_rules! key_bail {
    ($($name:ident($($ty:ty),*)),* $(,)?) => {$(
        fn $name(self, $(_: $ty),*) -> Result<(), Bail> { Err(Bail) }
    )*};
}
impl ser::Serializer for KeyLookup<'_, '_, '_> {
    type Ok = ();
    type Error = Bail;
    type SerializeSeq = ser::Impossible<(), Bail>;
    type SerializeTuple = ser::Impossible<(), Bail>;
    type SerializeTupleStruct = ser::Impossible<(), Bail>;
    type SerializeTupleVariant = ser::Impossible<(), Bail>;
    type SerializeMap = ser::Impossible<(), Bail>;
    type SerializeStruct = ser::Impossible<(), Bail>;
    type SerializeStructVariant = ser::Impossible<(), Bail>;
    fn serialize_str(self, key: &str) -> Fast {
        let found = self.0.find(key)?;
        self.0.pending = Some(found);
        Ok(())
    }
    key_bail! {
        serialize_bool(bool), serialize_i8(i8), serialize_i16(i16), serialize_i32(i32),
        serialize_i64(i64), serialize_u8(u8), serialize_u16(u16), serialize_u32(u32),
        serialize_u64(u64), serialize_f32(f32), serialize_f64(f64), serialize_char(char),
        serialize_bytes(&[u8]), serialize_none(), serialize_unit(), serialize_unit_struct(&'static str),
    }
    fn serialize_some<T: ?Sized + Serialize>(self, _: &T) -> Fast {
        Err(Bail)
    }
    fn serialize_unit_variant(self, _: &'static str, _: u32, variant: &'static str) -> Fast {
        self.serialize_str(variant)
    }
    fn serialize_newtype_struct<T: ?Sized + Serialize>(self, _: &'static str, value: &T) -> Fast {
        value.serialize(self)
    }
    fn serialize_newtype_variant<T: ?Sized + Serialize>(
        self,
        _: &'static str,
        _: u32,
        _: &'static str,
        _: &T,
    ) -> Fast {
        Err(Bail)
    }
    fn serialize_seq(self, _: Option<usize>) -> Result<Self::SerializeSeq, Bail> {
        Err(Bail)
    }
    fn serialize_tuple(self, _: usize) -> Result<Self::SerializeTuple, Bail> {
        Err(Bail)
    }
    fn serialize_tuple_struct(
        self,
        _: &'static str,
        _: usize,
    ) -> Result<Self::SerializeTupleStruct, Bail> {
        Err(Bail)
    }
    fn serialize_tuple_variant(
        self,
        _: &'static str,
        _: u32,
        _: &'static str,
        _: usize,
    ) -> Result<Self::SerializeTupleVariant, Bail> {
        Err(Bail)
    }
    fn serialize_map(self, _: Option<usize>) -> Result<Self::SerializeMap, Bail> {
        Err(Bail)
    }
    fn serialize_struct(self, _: &'static str, _: usize) -> Result<Self::SerializeStruct, Bail> {
        Err(Bail)
    }
    fn serialize_struct_variant(
        self,
        _: &'static str,
        _: u32,
        _: &'static str,
        _: usize,
    ) -> Result<Self::SerializeStructVariant, Bail> {
        Err(Bail)
    }
}

impl SerializeSeq for SeqCheck<'_, '_> {
    type Ok = ();
    type Error = Bail;
    fn serialize_element<T: ?Sized + Serialize>(&mut self, value: &T) -> Fast {
        self.element(value)
    }
    fn end(self) -> Fast {
        self.finish()
    }
}
impl SerializeTuple for SeqCheck<'_, '_> {
    type Ok = ();
    type Error = Bail;
    fn serialize_element<T: ?Sized + Serialize>(&mut self, value: &T) -> Fast {
        self.element(value)
    }
    fn end(self) -> Fast {
        self.finish()
    }
}
impl SerializeTupleStruct for SeqCheck<'_, '_> {
    type Ok = ();
    type Error = Bail;
    fn serialize_field<T: ?Sized + Serialize>(&mut self, value: &T) -> Fast {
        self.element(value)
    }
    fn end(self) -> Fast {
        self.finish()
    }
}
impl SerializeTupleVariant for SeqCheck<'_, '_> {
    type Ok = ();
    type Error = Bail;
    fn serialize_field<T: ?Sized + Serialize>(&mut self, value: &T) -> Fast {
        self.element(value)
    }
    fn end(self) -> Fast {
        self.finish()
    }
}
impl SerializeMap for ObjectCheck<'_, '_> {
    type Ok = ();
    type Error = Bail;
    fn serialize_key<T: ?Sized + Serialize>(&mut self, key: &T) -> Fast {
        key.serialize(KeyLookup(self))
    }
    fn serialize_value<T: ?Sized + Serialize>(&mut self, value: &T) -> Fast {
        let found = self.pending.take().ok_or(Bail)?;
        self.member(found, value)
    }
    fn end(self) -> Fast {
        self.finish()
    }
}
impl SerializeStruct for ObjectCheck<'_, '_> {
    type Ok = ();
    type Error = Bail;
    fn serialize_field<T: ?Sized + Serialize>(&mut self, key: &'static str, value: &T) -> Fast {
        self.field(key, value)
    }
    fn end(self) -> Fast {
        self.finish()
    }
}
impl SerializeStructVariant for ObjectCheck<'_, '_> {
    type Ok = ();
    type Error = Bail;
    fn serialize_field<T: ?Sized + Serialize>(&mut self, key: &'static str, value: &T) -> Fast {
        self.field(key, value)
    }
    fn end(self) -> Fast {
        self.finish()
    }
}

impl<'c, 't> ser::Serializer for Check<'c, 't> {
    type Ok = ();
    type Error = Bail;
    type SerializeSeq = SeqCheck<'c, 't>;
    type SerializeTuple = SeqCheck<'c, 't>;
    type SerializeTupleStruct = SeqCheck<'c, 't>;
    type SerializeTupleVariant = SeqCheck<'c, 't>;
    type SerializeMap = ObjectCheck<'c, 't>;
    type SerializeStruct = ObjectCheck<'c, 't>;
    type SerializeStructVariant = ObjectCheck<'c, 't>;

    fn serialize_bool(self, v: bool) -> Fast {
        match self.view {
            View::Bool(b) if b == v => Ok(()),
            _ => Err(Bail),
        }
    }
    fn serialize_i8(self, v: i8) -> Fast {
        self.number(Number::from(v))
    }
    fn serialize_i16(self, v: i16) -> Fast {
        self.number(Number::from(v))
    }
    fn serialize_i32(self, v: i32) -> Fast {
        self.number(Number::from(v))
    }
    fn serialize_i64(self, v: i64) -> Fast {
        self.number(Number::from(v))
    }
    fn serialize_u8(self, v: u8) -> Fast {
        self.number(Number::from(v))
    }
    fn serialize_u16(self, v: u16) -> Fast {
        self.number(Number::from(v))
    }
    fn serialize_u32(self, v: u32) -> Fast {
        self.number(Number::from(v))
    }
    fn serialize_u64(self, v: u64) -> Fast {
        self.number(Number::from(v))
    }
    fn serialize_f32(self, _: f32) -> Fast {
        Err(Bail)
    }
    fn serialize_f64(self, v: f64) -> Fast {
        match Number::from_f64(v) {
            Some(n) => self.number(n),
            None => self.null(), // `to_value` maps non-finite floats to null
        }
    }
    fn serialize_char(self, v: char) -> Fast {
        self.string(v.encode_utf8(&mut [0; 4]))
    }
    fn serialize_str(self, v: &str) -> Fast {
        self.string(v)
    }
    fn serialize_bytes(self, _: &[u8]) -> Fast {
        Err(Bail)
    }
    fn serialize_none(self) -> Fast {
        self.null()
    }
    fn serialize_some<T: ?Sized + Serialize>(self, value: &T) -> Fast {
        value.serialize(self)
    }
    fn serialize_unit(self) -> Fast {
        self.null()
    }
    fn serialize_unit_struct(self, _: &'static str) -> Fast {
        self.null()
    }
    fn serialize_unit_variant(self, _: &'static str, _: u32, variant: &'static str) -> Fast {
        self.string(variant)
    }
    fn serialize_newtype_struct<T: ?Sized + Serialize>(self, _: &'static str, value: &T) -> Fast {
        value.serialize(self)
    }
    fn serialize_newtype_variant<T: ?Sized + Serialize>(
        self,
        _: &'static str,
        _: u32,
        variant: &'static str,
        value: &T,
    ) -> Fast {
        value.serialize(self.single(variant)?)
    }
    fn serialize_seq(self, _: Option<usize>) -> Result<Self::SerializeSeq, Bail> {
        self.sequence()
    }
    fn serialize_tuple(self, _: usize) -> Result<Self::SerializeTuple, Bail> {
        self.sequence()
    }
    fn serialize_tuple_struct(
        self,
        _: &'static str,
        _: usize,
    ) -> Result<Self::SerializeTupleStruct, Bail> {
        self.sequence()
    }
    fn serialize_tuple_variant(
        self,
        _: &'static str,
        _: u32,
        variant: &'static str,
        _: usize,
    ) -> Result<Self::SerializeTupleVariant, Bail> {
        self.single(variant)?.sequence()
    }
    fn serialize_map(self, _: Option<usize>) -> Result<Self::SerializeMap, Bail> {
        self.object()
    }
    fn serialize_struct(self, _: &'static str, _: usize) -> Result<Self::SerializeStruct, Bail> {
        self.object()
    }
    fn serialize_struct_variant(
        self,
        _: &'static str,
        _: u32,
        variant: &'static str,
        _: usize,
    ) -> Result<Self::SerializeStructVariant, Bail> {
        self.single(variant)?.object()
    }
}
