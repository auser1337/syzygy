#![no_main]
#![no_std]
extern crate alloc;

mod registers;
mod svm;

use crate::svm::msrs;
use alloc::boxed::Box;
use core::alloc::{GlobalAlloc, Layout};
use core::ffi::c_void;
use log::info;
use uefi::allocator::Allocator;
use uefi::boot::PAGE_SIZE;
use uefi::prelude::*;
use uefi::proto::pi::mp::MpServices;

#[global_allocator]
static ALLOCATOR: Allocator = Allocator;

/// Each page is 4`KB`. We need:
/// - 4`KB` for `VM_HSAVE_PA`
/// - 4`KB` for VMCB
const PAGES_PER_PROCESSOR: usize = 2;

struct ProcessorData {
    host_state_save_area_pa: *mut u8,
    vmcb_pa: *mut u8,
}

extern "efiapi" fn ap_procedure(argument: *mut c_void) {
    let ProcessorData {
        host_state_save_area_pa,
        vmcb_pa: _,
    } = unsafe { &*(argument as *const ProcessorData) };

    unsafe { svm::enable() }.unwrap();

    let mut vm_hsave_pa = msrs::VM_HSAVE_PA;
    unsafe {
        // We dereference because `host_state_save_area_pa` is `&*mut u8`
        vm_hsave_pa.write(*host_state_save_area_pa as u64);
    }

    loop {
        unsafe {
            core::arch::asm!("hlt", options(nomem, nostack));
        }
    }
}

#[entry]
fn efi_main() -> Status {
    uefi::helpers::init().expect("failed to initialize UEFI helpers");

    let mp_services_handle =
        boot::get_handle_for_protocol::<MpServices>().expect("failed to get handle for MpServices");
    let mp_services = boot::open_protocol_exclusive::<MpServices>(mp_services_handle)
        .expect("failed to open MpServices protocol");

    let enabled_processor_count = mp_services
        .get_number_of_processors()
        .expect("failed to get number of processors")
        .enabled;

    // Allocate 8KB for each processor
    let memory = unsafe {
        ALLOCATOR.alloc_zeroed(
            Layout::from_size_align(
                enabled_processor_count * PAGES_PER_PROCESSOR * PAGE_SIZE,
                PAGE_SIZE,
            )
            .expect("failed to create memory layout for processor data"),
        )
    };

    let host_state_save_area_pa = memory;
    let mut vm_hsave_pa = msrs::VM_HSAVE_PA;
    unsafe {
        vm_hsave_pa.write(host_state_save_area_pa as u64);
    }

    unsafe { svm::enable() }.expect("failed to enable SVM on BSP");

    // We start at 1 to skip the BSP (bootstrap processor)
    for processor_number in 1..enabled_processor_count {
        let page_offset = processor_number * PAGES_PER_PROCESSOR;

        let host_state_save_area_pa = unsafe { memory.add(page_offset * PAGE_SIZE) };
        let vmcb_pa = unsafe { memory.add((page_offset + 1) * PAGE_SIZE) };

        // We use `Box::leak` to prevent `ProcessorData` from being freed
        let processor_data = Box::leak(Box::new(ProcessorData {
            host_state_save_area_pa,
            vmcb_pa,
        }));

        let ap_event = unsafe {
            boot::create_event(boot::EventType::empty(), boot::Tpl::APPLICATION, None, None)
        }
        .expect("failed to create AP event");

        mp_services
            .startup_this_ap(
                processor_number,
                ap_procedure,
                processor_data as *mut _ as *mut c_void,
                Some(ap_event),
                None,
            )
            .expect("failed to start AP");
    }

    info!("enabled SVM on {} processors", enabled_processor_count);

    loop {
        unsafe {
            core::arch::asm!("hlt", options(nomem, nostack));
        }
    }
}
