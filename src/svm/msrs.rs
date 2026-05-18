use x86_64::registers::model_specific::Msr;

pub(crate) const VM_CR: Msr = Msr::new(0xC001_0114);
pub(crate) const VM_HSAVE_PA: Msr = Msr::new(0xC001_0117);
