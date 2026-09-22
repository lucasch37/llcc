use std::collections::HashMap;

use super::register::{Allocation, PhysicalReg, RegisterClass, StackSlot, VRegId};
use crate::semantic::types::Type;

#[derive(Debug, Clone)]
pub struct LiveInterval {
    pub start: usize,
    pub end: usize,
    pub typ: Type,
}

impl LiveInterval {
    pub fn new(start: usize, end: usize, typ: Type) -> Self {
        Self { start, end, typ }
    }

    pub fn overlaps(&self, other: &Self) -> bool {
        self.start <= other.end && other.start <= self.end
    }
}

pub struct RegisterAllocator {
    gprs: Vec<PhysicalReg>,
    xmms: Vec<PhysicalReg>,
    next_spill_offset: usize,
}

pub struct AllocationResult {
    pub allocations: HashMap<VRegId, Allocation>,
    pub stack_size: usize,
}

impl RegisterAllocator {
    pub fn new(stack_size: usize) -> Self {
        Self {
            gprs: vec![
                PhysicalReg::R10,
                PhysicalReg::R11,
                PhysicalReg::R12,
                PhysicalReg::R13,
                PhysicalReg::R14,
                PhysicalReg::R15,
            ],

            xmms: vec![
                PhysicalReg::Xmm2,
                PhysicalReg::Xmm3,
                PhysicalReg::Xmm4,
                PhysicalReg::Xmm5,
                PhysicalReg::Xmm6,
                PhysicalReg::Xmm7,
            ],

            next_spill_offset: stack_size,
        }
    }

    pub fn allocate(mut self, intervals: &HashMap<VRegId, LiveInterval>) -> AllocationResult {
        let mut entries: Vec<(VRegId, LiveInterval)> = intervals
            .iter()
            .map(|(id, interval)| (*id, interval.clone()))
            .collect();

        entries.sort_by_key(|(_, interval)| interval.start);

        let mut allocations: HashMap<VRegId, Allocation> = HashMap::new();

        let mut active_gpr: Vec<(VRegId, LiveInterval, PhysicalReg)> = Vec::new();
        let mut active_xmm: Vec<(VRegId, LiveInterval, PhysicalReg)> = Vec::new();

        for (id, interval) in entries {
            match super::register::register_class(&interval.typ) {
                RegisterClass::Gpr => {
                    Self::expire_old(&interval, &mut active_gpr);

                    if let Some(reg) = self.first_free(&self.gprs, &active_gpr) {
                        allocations.insert(id, Allocation::Register(reg));
                        active_gpr.push((id, interval, reg));
                        active_gpr.sort_by_key(|(_, i, _)| i.end);
                    } else {
                        let spill = self.spill_slot(interval.typ.clone());
                        allocations.insert(id, Allocation::Spill(spill));
                    }
                }

                RegisterClass::Xmm => {
                    Self::expire_old(&interval, &mut active_xmm);

                    if let Some(reg) = self.first_free(&self.xmms, &active_xmm) {
                        allocations.insert(id, Allocation::Register(reg));
                        active_xmm.push((id, interval, reg));
                        active_xmm.sort_by_key(|(_, i, _)| i.end);
                    } else {
                        let spill = self.spill_slot(interval.typ.clone());
                        allocations.insert(id, Allocation::Spill(spill));
                    }
                }
            }
        }

        AllocationResult {
            allocations,
            stack_size: self.next_spill_offset,
        }
    }

    fn expire_old(current: &LiveInterval, active: &mut Vec<(VRegId, LiveInterval, PhysicalReg)>) {
        active.retain(|(_, interval, _)| interval.end >= current.start);
    }

    fn first_free(
        &self,
        pool: &[PhysicalReg],
        active: &[(VRegId, LiveInterval, PhysicalReg)],
    ) -> Option<PhysicalReg> {
        pool.iter()
            .copied()
            .find(|candidate| !active.iter().any(|(_, _, used)| used == candidate))
    }

    fn spill_slot(&mut self, typ: Type) -> StackSlot {
        let size = typ.size();

        self.next_spill_offset += size;

        let align = size.max(1);
        self.next_spill_offset = (self.next_spill_offset + align - 1) / align * align;

        StackSlot {
            offset: self.next_spill_offset,
            typ,
        }
    }
}
