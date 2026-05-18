pub(crate) mod msrs;
mod vcpu;

use crate::registers::Registers;
use core::arch::global_asm;
use core::ffi::c_void;
use raw_cpuid::CpuId;
use uefi::Status;
use x86_64::registers::model_specific::{Efer, EferFlags};

global_asm!(include_str!("run_vm.asm"));

pub(crate) unsafe fn enable() -> Result<(), Status> {
    let cpuid = CpuId::new();
    let svm_supported = cpuid
        .get_extended_processor_and_feature_identifiers()
        .is_some_and(|f| f.has_svm());
    if !svm_supported {
        return Err(Status::UNSUPPORTED);
    }

    let vm_cr = msrs::VM_CR;
    let svmdis = unsafe { vm_cr.read() } & (1 << 4);
    if svmdis != 0 {
        return Err(Status::UNSUPPORTED);
    }

    unsafe { Efer::update(|efer| efer.insert(EferFlags::SECURE_VIRTUAL_MACHINE_ENABLE)) };

    Ok(())
}

unsafe extern "efiapi" {
    // TODO: Make `guest_vmcb_pa` a `*mut Vmcb`
    fn run_vm(registers: &mut Registers, guest_vmcb_pa: *mut c_void);
}
