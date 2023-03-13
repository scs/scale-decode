// Copyright (C) 2023 Parity Technologies (UK) Ltd. (admin@parity.io)
// This file is a part of the scale-decode crate.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//         http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use crate::{
    visitor::{DecodeError, IgnoreVisitor, Visitor},
    DecodeAsType,
};
use scale_info::{form::PortableForm, Field, Path, PortableRegistry};

/// This represents a composite type.
pub struct Composite<'scale, 'info> {
    bytes: &'scale [u8],
    item_bytes: &'scale [u8],
    path: &'info Path<PortableForm>,
    fields: &'info [Field<PortableForm>],
    types: &'info PortableRegistry,
}

impl<'scale, 'info> Composite<'scale, 'info> {
    // Used in macros, but not really expected to be used elsewhere.
    #[doc(hidden)]
    pub fn new(
        bytes: &'scale [u8],
        path: &'info Path<PortableForm>,
        fields: &'info [Field<PortableForm>],
        types: &'info PortableRegistry,
    ) -> Composite<'scale, 'info> {
        Composite { bytes, path, item_bytes: bytes, fields, types }
    }
    /// Skip over all bytes associated with this composite type. After calling this,
    /// [`Self::bytes_from_undecoded()`] will represent the bytes after this composite type.
    pub fn skip_decoding(&mut self) -> Result<(), DecodeError> {
        while !self.fields.is_empty() {
            self.decode_item(IgnoreVisitor).transpose()?;
        }
        Ok(())
    }
    /// The bytes representing this composite type and anything following it.
    pub fn bytes_from_start(&self) -> &'scale [u8] {
        self.bytes
    }
    /// The bytes that have not yet been decoded in this composite type and anything
    /// following it.
    pub fn bytes_from_undecoded(&self) -> &'scale [u8] {
        self.item_bytes
    }
    /// The number of un-decoded items remaining in this composite type.
    pub fn remaining(&self) -> usize {
        self.fields.len()
    }
    /// Path to this type.
    pub fn path(&self) -> &'info Path<PortableForm> {
        self.path
    }
    /// The yet-to-be-decoded fields still present in this composite type.
    pub fn fields(&self) -> &'info [Field<PortableForm>] {
        self.fields
    }
    /// Return whether any of the fields are unnamed.
    pub fn has_unnamed_fields(&self) -> bool {
        self.fields.iter().any(|f| f.name().is_none())
    }
    /// Convert the remaining fields in this Composite type into a [`super::Tuple`]. This allows them to
    /// be parsed in the same way as a tuple type, discarding name information.
    pub fn as_tuple(&self) -> super::Tuple<'scale, 'info> {
        super::Tuple::new(self.item_bytes, self.fields, self.types)
    }
    /// Return the name of the next field to be decoded; `None` if either the field has no name,
    /// or there are no fields remaining.
    pub fn peek_name(&self) -> Option<&'info str> {
        self.fields.get(0).and_then(|f| f.name().map(|n| &**n))
    }
    /// Decode the next field in the composite type by providing a visitor to handle it. This is more
    /// efficient than iterating over the key/value pairs if you already know how you want to decode the
    /// values.
    pub fn decode_item<V: Visitor>(
        &mut self,
        visitor: V,
    ) -> Option<Result<V::Value<'scale, 'info>, V::Error>> {
        if self.fields.is_empty() {
            return None;
        }

        let field = &self.fields[0];
        let b = &mut &*self.item_bytes;

        // Decode the bytes:
        let res = crate::visitor::decode_with_visitor(b, field.ty().id(), self.types, visitor);

        // Update self to point to the next item, now:
        self.item_bytes = *b;
        self.fields = &self.fields[1..];

        Some(res)
    }
}

// Iterating returns a representation of each field in the composite type.
impl<'scale, 'info> Iterator for Composite<'scale, 'info> {
    type Item = Result<CompositeField<'scale, 'info>, DecodeError>;
    fn next(&mut self) -> Option<Self::Item> {
        // Record details we need before we decode and skip over the thing:
        let field = self.fields.get(0)?;
        let name = self.peek_name();
        let num_bytes_before = self.item_bytes.len();
        let item_bytes = self.item_bytes;

        if let Err(e) = self.decode_item(IgnoreVisitor)? {
            return Some(Err(e));
        };

        // How many bytes did we skip over? What bytes represent the thing we decoded?
        let num_bytes_after = self.item_bytes.len();
        let res_bytes = &item_bytes[..num_bytes_before - num_bytes_after];

        Some(Ok(CompositeField { bytes: res_bytes, field, name, types: self.types }))
    }
}

/// A single field in the composite type.
#[derive(Copy, Clone)]
pub struct CompositeField<'scale, 'info> {
    name: Option<&'info str>,
    bytes: &'scale [u8],
    field: &'info Field<PortableForm>,
    types: &'info PortableRegistry,
}

impl<'scale, 'info> CompositeField<'scale, 'info> {
    /// The field name.
    pub fn name(&self) -> Option<&'info str> {
        self.name
    }
    /// The bytes associated with this field.
    pub fn bytes(&self) -> &'scale [u8] {
        self.bytes
    }
    /// The type ID associated with this field.
    pub fn type_id(&self) -> u32 {
        self.field.ty().id()
    }
    /// Decode this field using a visitor.
    pub fn decode_with_visitor<V: Visitor>(
        &self,
        visitor: V,
    ) -> Result<V::Value<'scale, 'info>, V::Error> {
        crate::visitor::decode_with_visitor(
            &mut &*self.bytes,
            self.field.ty().id(),
            self.types,
            visitor,
        )
    }
    /// Decode this field into a specific type via [`DecodeAsType`].
    pub fn decode_as_type<T: DecodeAsType>(&self) -> Result<T, crate::Error> {
        T::decode_as_type(&mut &*self.bytes, self.field.ty().id(), self.types)
    }
}

impl<'scale, 'info> crate::visitor::DecodeItemIterator<'scale, 'info> for Composite<'scale, 'info> {
    fn decode_item<'a, V: Visitor>(
        &mut self,
        visitor: V,
    ) -> Option<Result<V::Value<'scale, 'info>, V::Error>> {
        self.decode_item(visitor)
    }
}