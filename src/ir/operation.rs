use crate::entity::{BlockId, OpId, RegionId, ValueId};

use super::attributes::{Attribute, NamedAttribute};
use super::location::Location;

/// Data stored for each operation in the arena.
///
/// Operations form an intrusive doubly-linked list within their parent block,
/// enabling O(1) insertion and removal with stable identity.
pub struct OperationData {
    pub(crate) name: String,
    pub(crate) operands: Vec<ValueId>,
    pub(crate) results: Vec<ValueId>,
    pub(crate) attributes: Vec<NamedAttribute>,
    pub(crate) regions: Vec<RegionId>,
    pub(crate) location: Location,
    // Navigation — parent + linked list
    pub(crate) parent_block: Option<BlockId>,
    pub(crate) prev_op: Option<OpId>,
    pub(crate) next_op: Option<OpId>,
}

impl OperationData {
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn operands(&self) -> &[ValueId] {
        &self.operands
    }
    pub fn results(&self) -> &[ValueId] {
        &self.results
    }
    pub fn result(&self, index: usize) -> ValueId {
        self.results[index]
    }
    pub fn num_results(&self) -> usize {
        self.results.len()
    }
    pub fn num_operands(&self) -> usize {
        self.operands.len()
    }
    pub fn attributes(&self) -> &[NamedAttribute] {
        &self.attributes
    }
    pub fn get_attribute(&self, name: &str) -> Option<&Attribute> {
        self.attributes
            .iter()
            .find(|a| a.name == name)
            .map(|a| &a.value)
    }
    pub fn set_attribute(&mut self, name: impl Into<String>, value: Attribute) {
        let name = name.into();
        if let Some(attr) = self.attributes.iter_mut().find(|a| a.name == name) {
            attr.value = value;
        } else {
            self.attributes.push(NamedAttribute::new(name, value));
        }
    }
    pub fn remove_attribute(&mut self, name: &str) -> bool {
        let len = self.attributes.len();
        self.attributes.retain(|a| a.name != name);
        self.attributes.len() < len
    }
    pub fn regions(&self) -> &[RegionId] {
        &self.regions
    }
    pub fn region(&self, index: usize) -> RegionId {
        self.regions[index]
    }
    pub fn num_regions(&self) -> usize {
        self.regions.len()
    }
    pub fn location(&self) -> &Location {
        &self.location
    }
    pub fn parent_block(&self) -> Option<BlockId> {
        self.parent_block
    }
    pub fn next_op(&self) -> Option<OpId> {
        self.next_op
    }
    pub fn prev_op(&self) -> Option<OpId> {
        self.prev_op
    }
    pub fn is_a(&self, op_name: &str) -> bool {
        self.name == op_name
    }
}
