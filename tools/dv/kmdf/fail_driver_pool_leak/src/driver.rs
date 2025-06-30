// Copyright (c) Microsoft Corporation.
// License: MIT OR Apache-2.0

use wdk::{nt_success, paged_code, println};
use wdk_sys::{
    call_unsafe_wdf_function_binding,
    ntddk::ExAllocatePool2,
    DRIVER_OBJECT,
    NTSTATUS,
    PCUNICODE_STRING,
    PDRIVER_OBJECT,
    POOL_FLAG_NON_PAGED,
    SIZE_T,
    ULONG,
    WDFDEVICE,
    WDFDEVICE_INIT,
    WDFDRIVER,
    WDF_DRIVER_CONFIG,
    WDF_NO_HANDLE,
    WDF_NO_OBJECT_ATTRIBUTES,
    WDF_OBJECT_ATTRIBUTES,
    _WDF_EXECUTION_LEVEL,
    _WDF_SYNCHRONIZATION_SCOPE,
};

use crate::{GLOBAL_BUFFER, GUID_DEVINTERFACE};

/// `DriverEntry` initializes the driver and is the first routine called by the
/// system after the driver is loaded. `DriverEntry` specifies the other entry
/// points in the function driver, such as `EvtDevice` and `DriverUnload`.
///
/// # Arguments
///
/// * `driver` - represents the instance of the function driver that is loaded
///   into memory. `DriverEntry` must initialize members of `DriverObject`
///   before it returns to the caller. `DriverObject` is allocated by the system
///   before the driver is loaded, and it is released by the system after the
///   system unloads the function driver from memory.
/// * `registry_path` - represents the driver specific path in the Registry. The
///   function driver can use the path to store driver related data between
///   reboots. The path does not store hardware instance specific data.
///
/// # Return value:
///
/// * `STATUS_SUCCESS` - if successful,
/// * `STATUS_UNSUCCESSFUL` - otherwise.
#[link_section = "INIT"]
#[export_name = "DriverEntry"]
extern "system" fn driver_entry(
    driver: &mut DRIVER_OBJECT,
    registry_path: PCUNICODE_STRING,
) -> NTSTATUS {
    println!("Enter: driver_entry");

    let mut driver_config = {
        let wdf_driver_config_size: ULONG;

        // clippy::cast_possible_truncation cannot currently check compile-time constants: https://github.com/rust-lang/rust-clippy/issues/9613
        #[allow(clippy::cast_possible_truncation)]
        {
            const WDF_DRIVER_CONFIG_SIZE: usize = core::mem::size_of::<WDF_DRIVER_CONFIG>();

            // Manually assert there is not truncation since clippy doesn't work for
            // compile-time constants
            const { assert!(WDF_DRIVER_CONFIG_SIZE <= ULONG::MAX as usize) }

            wdf_driver_config_size = WDF_DRIVER_CONFIG_SIZE as ULONG;
        }

        WDF_DRIVER_CONFIG {
            Size: wdf_driver_config_size,
            EvtDriverDeviceAdd: Some(evt_driver_device_add),
            EvtDriverUnload: Some(evt_driver_unload),
            ..WDF_DRIVER_CONFIG::default()
        }
    };

    let driver_handle_output = WDF_NO_HANDLE.cast::<WDFDRIVER>();

    let nt_status = unsafe {
        call_unsafe_wdf_function_binding!(
            WdfDriverCreate,
            driver as PDRIVER_OBJECT,
            registry_path,
            WDF_NO_OBJECT_ATTRIBUTES,
            &raw mut driver_config,
            driver_handle_output,
        )
    };

    if !nt_success(nt_status) {
        println!("Error: WdfDriverCreate failed {nt_status:#010X}");
        return nt_status;
    }

    println!("Exit: driver_entry");

    nt_status
}

/// `EvtDeviceAdd` is called by the framework in response to `AddDevice`
/// call from the `PnP` manager. We create and initialize a device object to
/// represent a new instance of the device.
///
/// # Arguments:
///
/// * `_driver` - Handle to a framework driver object created in `DriverEntry`
/// * `device_init` - Pointer to a framework-allocated `WDFDEVICE_INIT`
///   structure.
///
/// # Return value:
///
///   * `NTSTATUS`
#[link_section = "PAGE"]
extern "C" fn evt_driver_device_add(
    _driver: WDFDRIVER,
    mut device_init: *mut WDFDEVICE_INIT,
) -> NTSTATUS {
    paged_code!();

    println!("Enter: evt_driver_device_add");

    #[allow(clippy::cast_possible_truncation)]
    let mut attributes = WDF_OBJECT_ATTRIBUTES {
        Size: core::mem::size_of::<WDF_OBJECT_ATTRIBUTES>() as ULONG,
        ExecutionLevel: _WDF_EXECUTION_LEVEL::WdfExecutionLevelInheritFromParent,
        SynchronizationScope: _WDF_SYNCHRONIZATION_SCOPE::WdfSynchronizationScopeInheritFromParent,
        ..WDF_OBJECT_ATTRIBUTES::default()
    };

    let mut device = WDF_NO_HANDLE as WDFDEVICE;
    let mut nt_status = unsafe {
        call_unsafe_wdf_function_binding!(
            WdfDeviceCreate,
            &raw mut device_init,
            &raw mut attributes,
            &raw mut device,
        )
    };

    if !nt_success(nt_status) {
        println!("Error: WdfDeviceCreate failed {nt_status:#010X}");
        return nt_status;
    }

    // Allocate non-paged memory pool of 64 bytes (arbitrarily chosen) for the
    // Global buffer. This pool of memory is intentionally not freed by
    // the driver.
    unsafe {
        const LENGTH: usize = 64;
        GLOBAL_BUFFER = ExAllocatePool2(POOL_FLAG_NON_PAGED, LENGTH as SIZE_T, 's' as u32);
    }

    nt_status = unsafe {
        call_unsafe_wdf_function_binding!(
            WdfDeviceCreateDeviceInterface,
            device,
            &GUID_DEVINTERFACE,
            core::ptr::null_mut(),
        )
    };

    if !nt_success(nt_status) {
        println!("Error: WdfDeviceCreateDeviceInterface failed {nt_status:#010X}");
        return nt_status;
    }

    println!("Exit: evt_driver_device_add");

    nt_status
}

/// This event callback function is called before the driver is unloaded
///
/// The EvtDriverUnload callback function must deallocate any
/// non-device-specific system resources that the driver's DriverEntry routine
/// allocated.
///
/// # Argument:
///
/// * `driver` - Handle to the framework driver object
///
/// # Return Value:
///
/// None
extern "C" fn evt_driver_unload(_driver: WDFDRIVER) {
    println!("Enter: evt_driver_unload");

    // Ideally, the memory allocated to the Global buffer in lib.rs L51 should
    // be freed here by calling the ExFreePool API. But to demonstrate the Driver
    // Verifier's ability to catch pool leaks, the buffer is deliberately not freed.

    // unsafe { wdk_sys::ntddk::ExFreePool(GLOBAL_BUFFER) };

    println!("Exit: evt_driver_unload");
}
