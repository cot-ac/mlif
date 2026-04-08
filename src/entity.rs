//! Typed entity references and arena storage.
//!
//! This module provides the infrastructure for MLIF's arena-based IR.
//! Every IR entity (operation, block, region, value, type) is stored in a
//! central arena and referenced by a lightweight, Copy handle. This gives
//! stable identity, O(1) lookup, and avoids Rust borrow-checker issues
//! with graph structures.
//!
//! Modeled after cranelift-entity's PrimaryMap pattern.

use std::fmt;
use std::hash::Hash;
use std::marker::PhantomData;
use std::ops::{Index, IndexMut};

/// A lightweight, typed reference into an entity arena.
pub trait EntityRef: Copy + Eq + Hash + fmt::Debug {
    fn new(index: usize) -> Self;
    fn index(self) -> usize;
}

/// Dense map from entity references to values. Keys are auto-assigned
/// sequential indices via `push`.
pub struct PrimaryMap<K: EntityRef, V> {
    elems: Vec<V>,
    _marker: PhantomData<K>,
}

impl<K: EntityRef, V> PrimaryMap<K, V> {
    pub fn new() -> Self {
        Self {
            elems: Vec::new(),
            _marker: PhantomData,
        }
    }

    /// Append a value, returning its key.
    pub fn push(&mut self, v: V) -> K {
        let key = K::new(self.elems.len());
        self.elems.push(v);
        key
    }

    pub fn len(&self) -> usize {
        self.elems.len()
    }

    pub fn is_empty(&self) -> bool {
        self.elems.is_empty()
    }

    pub fn get(&self, k: K) -> Option<&V> {
        self.elems.get(k.index())
    }

    pub fn get_mut(&mut self, k: K) -> Option<&mut V> {
        self.elems.get_mut(k.index())
    }

    pub fn keys(&self) -> impl Iterator<Item = K> {
        (0..self.elems.len()).map(K::new)
    }

    pub fn values(&self) -> impl Iterator<Item = &V> {
        self.elems.iter()
    }

    pub fn iter(&self) -> impl Iterator<Item = (K, &V)> {
        self.elems
            .iter()
            .enumerate()
            .map(|(i, v)| (K::new(i), v))
    }

    /// Return the next key that `push` will assign.
    pub fn next_key(&self) -> K {
        K::new(self.elems.len())
    }
}

impl<K: EntityRef, V> Default for PrimaryMap<K, V> {
    fn default() -> Self {
        Self::new()
    }
}

impl<K: EntityRef, V> Index<K> for PrimaryMap<K, V> {
    type Output = V;
    fn index(&self, k: K) -> &V {
        &self.elems[k.index()]
    }
}

impl<K: EntityRef, V> IndexMut<K> for PrimaryMap<K, V> {
    fn index_mut(&mut self, k: K) -> &mut V {
        &mut self.elems[k.index()]
    }
}

// ---------------------------------------------------------------------------
// Entity ID types
// ---------------------------------------------------------------------------

macro_rules! define_entity {
    ($name:ident, $prefix:expr) => {
        #[derive(Clone, Copy, PartialEq, Eq, Hash)]
        #[repr(transparent)]
        pub struct $name(u32);

        impl EntityRef for $name {
            fn new(index: usize) -> Self {
                Self(index as u32)
            }
            fn index(self) -> usize {
                self.0 as usize
            }
        }

        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}{}", $prefix, self.0)
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}{}", $prefix, self.0)
            }
        }
    };
}

define_entity!(OpId, "op");
define_entity!(BlockId, "^bb");
define_entity!(RegionId, "region");
define_entity!(ValueId, "%");
define_entity!(TypeId, "!ty");
