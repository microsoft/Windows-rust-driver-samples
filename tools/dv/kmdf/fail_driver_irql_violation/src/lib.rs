// Copyright (c) Microsoft Corporation.
// License: MIT OR Apache-2.0

//! # Abstract
//!
//! This KMDF sample contains an intentional error that is designed to
//! demonstrate the capabilities and features of Driver Verifier and the Device
//! Fundamental tests.
//!     
//! The driver is designed to allocate memory using ExAllocatePool2 to its
//! Device Context buffer when a device is added by the PnP manager. However,
//! this buffer is not freed anywhere in the driver, including the driver unload
//! function.
//!
//! By enabling Driver Verifier on this driver, the pool leak
//! violation can be caught when the driver is unloaded and with an active KDNET
//! session, the bug can be analyzed further.

#![no_std]
#![cfg_attr(feature = "nightly", feature(hint_must_use))]
#![deny(clippy::all)]
#![warn(clippy::pedantic)]
#![warn(clippy::nursery)]
#![warn(clippy::cargo)]
#![allow(clippy::missing_safety_doc)]
#![allow(clippy::doc_markdown)]

#[cfg(not(test))]
extern crate wdk_panic;

#[cfg(not(test))]
use wdk_alloc::WDKAllocator;

#[cfg(not(test))]
#[global_allocator]
static GLOBAL_ALLOCATOR: WDKAllocator = WDKAllocator;

use wdk::{println, wdf::SpinLock};
use wdk_sys::{
    GUID,
    PVOID,
    ULONG,
    WDF_OBJECT_ATTRIBUTES,
    _WDF_EXECUTION_LEVEL,
    _WDF_SYNCHRONIZATION_SCOPE,
};

// {B2C3D4E5-F678-9012-3456-7890ABCDEF12}
const GUID_DEVINTERFACE: GUID = GUID {
    Data1: 0xB2C3_D4E5u32,
    Data2: 0xF678u16,
    Data3: 0x9012u16,
    Data4: [
        0x34u8, 0x56u8, 0x78u8, 0x90u8, 0xABu8, 0xCDu8, 0xEFu8, 0x12u8,
    ],
};

// Global Buffer for the driver
static mut GLOBAL_BUFFER: PVOID = core::ptr::null_mut();

// Spinlock to synchronize access to the global buffer
static mut SPINLOCK: Option<SpinLock> = None;

/// `initialize_spinlock` is called by
fn initialize_spinlock() -> Result<(), usize> {
    let mut attributes = WDF_OBJECT_ATTRIBUTES {
        Size: core::mem::size_of::<WDF_OBJECT_ATTRIBUTES>() as ULONG,
        ExecutionLevel: _WDF_EXECUTION_LEVEL::WdfExecutionLevelInheritFromParent,
        SynchronizationScope: _WDF_SYNCHRONIZATION_SCOPE::WdfSynchronizationScopeInheritFromParent,
        ..WDF_OBJECT_ATTRIBUTES::default()
    };

    match SpinLock::create(&mut attributes) {
        Err(status) => {
            println!("SpinLock create failed {status:#010X}");
            return Err(status as usize);
        }
        Ok(spin_lock) => unsafe {
            SPINLOCK = Some(spin_lock);
        },
    }

    Ok(())
}

mod driver;
