// Copyright (c) Microsoft Corporation.
// License: MIT OR Apache-2.0

//! # Abstract
//!
//!    This driver demonstrates use of a default I/O Queue, its
//!    request start events, cancellation event, and a synchronized DPC.
//!
//!    To demonstrate asynchronous operation, the I/O requests are not completed
//!    immediately, but stored in the drivers private data structure, and a
//!    timer DPC will complete it next time the DPC runs.
//!
//!    During the time the request is waiting for the DPC to run, it is
//!    made cancellable by the call `WdfRequestMarkCancelable`. This
//!    allows the test program to cancel the request and exit instantly.
//!
//!    This rather complicated set of events is designed to demonstrate
//!    the driver frameworks synchronization of access to a device driver
//!    data structure, and a pointer which can be a proxy for device hardware
//!    registers or resources.
//!
//!    This common data structure, or resource is accessed by new request
//!    events arriving, the DPC that completes it, and cancel processing.
//!
//!    Notice the lack of specific lock/unlock operations.
//!
//!    Even though this example utilizes a serial queue, a parallel queue
//!    would not need any additional explicit synchronization, just a
//!    strategy for managing multiple requests outstanding.

#![no_std]
#![deny(clippy::all)]
#![warn(clippy::pedantic)]
#![warn(clippy::nursery)]
#![warn(clippy::cargo)]
#![allow(clippy::missing_safety_doc)]

mod device;
mod driver;
mod queue;

#[cfg(not(test))]
extern crate wdk_panic;

use wdk::wdf;
#[cfg(not(test))]
use wdk_alloc::WdkAllocator;
use wdk_sys::{
    call_unsafe_wdf_function_binding,
    GUID,
    NTSTATUS,
    PVOID,
    ULONG,
    WDFOBJECT,
    WDFREQUEST,
    WDF_DRIVER_CONFIG,
    WDF_DRIVER_VERSION_AVAILABLE_PARAMS,
    WDF_IO_QUEUE_CONFIG,
    WDF_OBJECT_ATTRIBUTES,
    WDF_OBJECT_CONTEXT_TYPE_INFO,
    WDF_PNPPOWER_EVENT_CALLBACKS,
    WDF_TIMER_CONFIG,
};
mod wdf_object_context;
use core::sync::atomic::AtomicI32;

use wdf_object_context::{wdf_declare_context_type, wdf_declare_context_type_with_name};

#[cfg(not(test))]
#[global_allocator]
static GLOBAL_ALLOCATOR: WdkAllocator = WdkAllocator;

// {CDC35B6E-0BE4-4936-BF5F-5537380A7C1A}
const GUID_DEVINTERFACE_ECHO: GUID = GUID {
    Data1: 0xCDC3_5B6Eu32,
    Data2: 0x0BE4u16,
    Data3: 0x4936u16,
    Data4: [
        0xBFu8, 0x5Fu8, 0x55u8, 0x37u8, 0x38u8, 0x0Au8, 0x7Cu8, 0x1Au8,
    ],
};

// Declare queue context.
//
// ====== CONTEXT SETUP ========//

// The device context performs the same job as
// a WDM device extension in the driver frameworks
pub struct DeviceContext {
    private_device_data: ULONG, // just a placeholder
}
wdf_declare_context_type!(DeviceContext);

pub struct QueueContext {
    buffer: PVOID,
    length: usize,
    timer: wdf::Timer,
    current_request: WDFREQUEST,
    current_status: NTSTATUS,
    spin_lock: wdf::SpinLock,
}
wdf_declare_context_type_with_name!(QueueContext, queue_get_context);

pub struct RequestContext {
    cancel_completion_ownership_count: AtomicI32,
}
wdf_declare_context_type_with_name!(RequestContext, request_get_context);

// None of the below SIZE constants should be needed after an equivalent `WDF_STRUCTURE_SIZE` macro is added to `wdk-sys`: https://github.com/microsoft/windows-drivers-rs/issues/242

#[allow(
    clippy::cast_possible_truncation,
    reason = "size_of::<WDF_DRIVER_CONFIG>() is known to fit in ULONG due to below const assert"
)]
const WDF_DRIVER_CONFIG_SIZE: ULONG = {
    const S: usize = core::mem::size_of::<WDF_DRIVER_CONFIG>();
    const {
        assert!(
            S <= ULONG::MAX as usize,
            "size_of::<WDF_DRIVER_CONFIG>() should fit in ULONG"
        );
    };
    S as ULONG
};

#[allow(
    clippy::cast_possible_truncation,
    reason = "size_of::<WDF_DRIVER_VERSION_AVAILABLE_PARAMS>() is known to fit in ULONG due to \
              below const assert"
)]
const WDF_DRIVER_VERSION_AVAILABLE_PARAMS_SIZE: ULONG = {
    const S: usize = core::mem::size_of::<WDF_DRIVER_VERSION_AVAILABLE_PARAMS>();
    const {
        assert!(
            S <= ULONG::MAX as usize,
            "size_of::<WDF_DRIVER_VERSION_AVAILABLE_PARAMS>() should fit in ULONG"
        );
    };
    S as ULONG
};

#[allow(
    clippy::cast_possible_truncation,
    reason = "size_of::<WDF_IO_QUEUE_CONFIG>() is known to fit in ULONG due to below const assert"
)]
const WDF_IO_QUEUE_CONFIG_SIZE: ULONG = {
    const S: usize = core::mem::size_of::<WDF_IO_QUEUE_CONFIG>();
    const {
        assert!(
            S <= ULONG::MAX as usize,
            "size_of::<WDF_IO_QUEUE_CONFIG>() should fit in ULONG"
        );
    };
    S as ULONG
};

#[allow(
    clippy::cast_possible_truncation,
    reason = "size_of::<WDF_OBJECT_ATTRIBUTES>() is known to fit in ULONG due to below const \
              assert"
)]
const WDF_OBJECT_ATTRIBUTES_SIZE: ULONG = {
    const S: usize = core::mem::size_of::<WDF_OBJECT_ATTRIBUTES>();
    const {
        assert!(
            S <= ULONG::MAX as usize,
            "size_of::<WDF_OBJECT_ATTRIBUTES>() should fit in ULONG"
        );
    };
    S as ULONG
};

#[allow(
    clippy::cast_possible_truncation,
    reason = "size_of::<WDF_OBJECT_CONTEXT_TYPE_INFO>() is known to fit in ULONG due to below \
              const assert"
)]
const WDF_OBJECT_CONTEXT_TYPE_INFO_SIZE: ULONG = {
    const S: usize = core::mem::size_of::<WDF_OBJECT_CONTEXT_TYPE_INFO>();
    const {
        assert!(
            S <= ULONG::MAX as usize,
            "size_of::<WDF_OBJECT_CONTEXT_TYPE_INFO>() should fit in ULONG"
        );
    };
    S as ULONG
};

#[allow(
    clippy::cast_possible_truncation,
    reason = "size_of::<WDF_PNPPOWER_EVENT_CALLBACKS>() is known to fit in ULONG due to below \
              const assert"
)]
const WDF_PNPPOWER_EVENT_CALLBACKS_SIZE: ULONG = {
    const S: usize = core::mem::size_of::<WDF_PNPPOWER_EVENT_CALLBACKS>();
    const {
        assert!(
            S <= ULONG::MAX as usize,
            "size_of::<WDF_PNPPOWER_EVENT_CALLBACKS>() should fit in ULONG"
        );
    };
    S as ULONG
};

#[allow(
    clippy::cast_possible_truncation,
    reason = "size_of::<WDF_TIMER_CONFIG>() is known to fit in ULONG due to below const assert"
)]
const WDF_TIMER_CONFIG_SIZE: ULONG = {
    const S: usize = core::mem::size_of::<WDF_TIMER_CONFIG>();
    const {
        assert!(
            S <= ULONG::MAX as usize,
            "size_of::<WDF_TIMER_CONFIG>() should fit in ULONG"
        );
    };
    S as ULONG
};
